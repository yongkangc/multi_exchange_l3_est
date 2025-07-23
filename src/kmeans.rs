use rand::Rng;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, VecDeque};

// Point structure for clustering (using qty only for simplicity)
#[derive(Clone, Copy, Debug)]
struct Point {
    qty: f64,
}

fn euclidean_distance(a: &Point, b: &Point) -> f64 {
    (a.qty - b.qty).abs()
}

fn normalize(points: &mut [Point]) {
    if points.is_empty() {
        return;
    }

    let mut min_q = f64::MAX;
    let mut max_q = f64::MIN;

    for p in points.iter() {
        min_q = min_q.min(p.qty);
        max_q = max_q.max(p.qty);
    }

    let range_q = max_q - min_q;

    if range_q > 0.0 {
        for p in points.iter_mut() {
            p.qty = (p.qty - min_q) / range_q;
        }
    }
}

// Mini-batch K-means with stability: uses previous centroids if provided, deterministic init if not, and label sorting
pub struct MiniBatchKMeans {
    num_clusters: usize,
    batch_size: usize,
    max_iter: usize,
    centroids: Vec<Point>,
}

impl MiniBatchKMeans {
    pub fn new(num_clusters: usize, batch_size: usize, max_iter: usize) -> Self {
        Self {
            num_clusters,
            batch_size,
            max_iter,
            centroids: vec![],
        }
    }

    // Fit on data, using previous centroids if available
    pub fn fit(&mut self, order_book: &BTreeMap<Decimal, VecDeque<Decimal>>) -> Vec<usize> {
        let mut points: Vec<Point> = vec![];
        let mut order_list: Vec<(Decimal, Decimal)> = vec![];

        for (&price, deq) in order_book.iter() {
            for &qty in deq.iter() {
                if qty > Decimal::ZERO {
                    let qty_f64 = qty.to_f64().unwrap_or(0.0);
                    points.push(Point { qty: qty_f64 });
                    order_list.push((price, qty));
                }
            }
        }

        if points.is_empty() {
            return vec![];
        }

        normalize(&mut points);

        // Initialize centroids if not already set
        if self.centroids.is_empty() || self.centroids.len() != self.num_clusters {
            self.centroids = self.initialize_centroids(&points);
        }

        // Mini-batch updates
        let mut rng = rand::rng();
        for _ in 0..self.max_iter {
            // Select mini-batch
            let batch_indices: Vec<usize> = (0..self.batch_size.min(points.len()))
                .map(|_| rng.random_range(0..points.len()))
                .collect();

            let mut counts = vec![0; self.num_clusters];
            let mut sums = vec![0.0; self.num_clusters];

            for &idx in &batch_indices {
                let p = points[idx];
                let closest = self.closest_centroid(&p);
                sums[closest] += p.qty;
                counts[closest] += 1;
            }

            for i in 0..self.num_clusters {
                if counts[i] > 0 {
                    let lr = 1.0 / counts[i] as f64; // Learning rate
                    self.centroids[i].qty =
                        (1.0 - lr) * self.centroids[i].qty + lr * (sums[i] / counts[i] as f64);
                }
            }
        }

        // Assign labels
        let mut labels = vec![0; points.len()];
        for (i, p) in points.iter().enumerate() {
            labels[i] = self.closest_centroid(p);
        }

        // Stabilize labels by sorting based on centroid qty
        let mut centroid_indices: Vec<usize> = (0..self.num_clusters).collect();
        centroid_indices.sort_by(|&a, &b| {
            self.centroids[a]
                .qty
                .partial_cmp(&self.centroids[b].qty)
                .unwrap_or(Ordering::Equal)
        });

        let mut label_map = HashMap::new();
        for (new_label, &old_label) in centroid_indices.iter().enumerate() {
            label_map.insert(old_label, new_label);
        }

        for label in labels.iter_mut() {
            *label = *label_map.get(label).unwrap_or(&0);
        }

        labels
    }

    fn closest_centroid(&self, p: &Point) -> usize {
        let mut min_dist = f64::INFINITY;
        let mut min_idx = 0;
        for (i, c) in self.centroids.iter().enumerate() {
            let dist = euclidean_distance(p, c);
            if dist < min_dist {
                min_dist = dist;
                min_idx = i;
            }
        }
        min_idx
    }

    fn initialize_centroids(&self, points: &[Point]) -> Vec<Point> {
        let mut centroids = vec![];

        // Deterministic initialization: sort by qty and pick evenly spaced points
        let mut sorted: Vec<Point> = points.to_vec();
        sorted.sort_by(|a, b| a.qty.partial_cmp(&b.qty).unwrap_or(Ordering::Equal));

        let step = (sorted.len() - 1) / (self.num_clusters.max(1) - 1).max(1);
        for i in 0..self.num_clusters {
            let idx = (i * step).min(sorted.len() - 1);
            centroids.push(sorted[idx]);
        }

        while centroids.len() < self.num_clusters && !sorted.is_empty() {
            centroids.push(sorted[0]); // Fill remaining with first point if needed
        }

        centroids
    }
}

// Usage in cluster_order_book
#[allow(dead_code)]
pub fn cluster_order_book(
    order_book: &BTreeMap<Decimal, VecDeque<Decimal>>,
    num_classes: usize,
    batch_size: usize,
    max_iter: usize,
) -> BTreeMap<Decimal, VecDeque<(Decimal, usize)>> {
    let mut kmeans = MiniBatchKMeans::new(num_classes, batch_size, max_iter);

    let labels = kmeans.fit(order_book);

    let mut clustered_orders: BTreeMap<Decimal, VecDeque<(Decimal, usize)>> = BTreeMap::new();

    let mut idx = 0;
    for (&price, deq) in order_book.iter() {
        let entry = clustered_orders.entry(price).or_default();
        for &qty in deq.iter() {
            if qty > Decimal::ZERO {
                entry.push_back((qty, labels[idx]));
                idx += 1;
            }
        }
    }

    clustered_orders
}

// Helper function to build clustered orders (assuming it's defined in kmeans.rs)
pub fn build_clustered_orders(
    order_book: &BTreeMap<Decimal, VecDeque<Decimal>>,
    labels: &[usize],
) -> BTreeMap<Decimal, VecDeque<(Decimal, usize)>> {
    let mut clustered_orders: BTreeMap<Decimal, VecDeque<(Decimal, usize)>> = BTreeMap::new();
    let mut idx = 0;

    for (&price, deq) in order_book.iter() {
        let entry = clustered_orders.entry(price).or_default();
        for &qty in deq.iter() {
            if qty > Decimal::ZERO {
                entry.push_back((qty, labels[idx]));
                idx += 1;
            }
        }
    }

    clustered_orders
}
