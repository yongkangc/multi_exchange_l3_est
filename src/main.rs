mod kmeans;
mod exchanges;

use eframe::egui;
use egui::{Align2, Color32};
use egui_plot::{Bar, BarChart, Plot, PlotPoint, Text};
use exchanges::{Exchange, ExchangeMessage, ExchangeType};
use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::sync::mpsc::{self as std_mpsc, Receiver as StdReceiver, Sender as StdSender};
use std::thread;
use tokio::sync::mpsc::{self, Receiver, Sender};

enum AppMessage {
    Snapshot(exchanges::OrderBookSnapshot),
    Update(exchanges::DepthUpdate),
}

enum Control {
    Refetch,
    ChangeSymbol(String),
    ChangeExchange(ExchangeType),
}

static BID_COLORS: Lazy<Vec<Color32>> = Lazy::new(|| {
    vec![
        Color32::from_rgb(222, 235, 247), // Light Blue
        Color32::from_rgb(204, 227, 245), // Lighter Blue
        Color32::from_rgb(158, 202, 225), // Blue
        Color32::from_rgb(129, 189, 231), // Light Medium Blue
        Color32::from_rgb(107, 174, 214), // Medium Blue
        Color32::from_rgb(78, 157, 202),  // Medium Deep Blue
        Color32::from_rgb(49, 130, 189),  // Deep Blue
        Color32::from_rgb(33, 113, 181),  // Darker Deep Blue
        Color32::from_rgb(16, 96, 168),   // Dark Blue
        Color32::from_rgb(8, 81, 156),    // Darkest Blue
    ]
});

static ASK_COLORS: Lazy<Vec<Color32>> = Lazy::new(|| {
    vec![
        Color32::from_rgb(254, 230, 206), // Light Orange
        Color32::from_rgb(253, 216, 186), // Lighter Orange
        Color32::from_rgb(253, 174, 107), // Orange
        Color32::from_rgb(253, 159, 88),  // Light Deep Orange
        Color32::from_rgb(253, 141, 60),  // Deep Orange
        Color32::from_rgb(245, 126, 47),  // Medium Red-Orange
        Color32::from_rgb(230, 85, 13),   // Red-Orange
        Color32::from_rgb(204, 75, 12),   // Darker Red-Orange
        Color32::from_rgb(179, 65, 10),   // Dark Red
        Color32::from_rgb(166, 54, 3),    // Darkest Red
    ]
});

fn main() -> eframe::Result {
    // Fetch the symbol from command-line arguments or default to appropriate symbol per exchange
    let args: Vec<String> = env::args().collect();
    let symbol: String = if args.len() > 1 {
        args[1].to_ascii_lowercase()
    } else {
        "dogeusdt".to_string() // Default for Binance, will be adjusted per exchange
    };

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Multi-Exchange Order Book Visualizer",
        options,
        Box::new(move |cc| Ok(Box::new(MyApp::new(cc, symbol)))),
    )
}

struct MyApp {
    symbol: String,
    edited_symbol: String,
    bids: BTreeMap<Decimal, VecDeque<Decimal>>,
    asks: BTreeMap<Decimal, VecDeque<Decimal>>,
    last_applied_u: u64,
    is_synced: bool,
    rx: StdReceiver<AppMessage>,
    update_buffer: VecDeque<exchanges::DepthUpdate>,
    control_tx: Sender<Control>,
    kmeans_mode: bool,
    price_prec: usize,
    qty_prec: usize,
    batch_size: usize,
    max_iter: usize,
    current_exchange: ExchangeType,
    exchange_names: Vec<&'static str>,
    selected_exchange_idx: usize,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>, symbol: String) -> Self {
        let (tx, rx) = std_mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel(1);
        let ctx = cc.egui_ctx.clone();
        let s = symbol.clone();
        let initial_exchange = ExchangeType::Binance;
        let current_exchange = initial_exchange;
        
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                Self::fetch_and_stream_loop(&tx, &ctx, control_rx, s, initial_exchange).await;
            });
        });

        let exchange = current_exchange.create_exchange();
        let (price_prec, qty_prec) = exchange.get_precision(&symbol);
        let exchange_names = vec!["Binance", "Hyperliquid"];

        Self {
            symbol: symbol.clone(),
            edited_symbol: symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_applied_u: 0,
            is_synced: false,
            rx,
            update_buffer: VecDeque::new(),
            control_tx,
            kmeans_mode: false,
            price_prec,
            qty_prec,
            batch_size: 1024,
            max_iter: 1024,
            current_exchange,
            exchange_names,
            selected_exchange_idx: 0,
        }
    }

    async fn fetch_and_stream_loop(
        tx: &StdSender<AppMessage>,
        ctx: &egui::Context,
        mut control_rx: Receiver<Control>,
        mut symbol: String,
        mut exchange_type: ExchangeType,
    ) {
        loop {
            let exchange = exchange_type.create_exchange();
            let formatted_symbol = exchange.format_symbol(&symbol);
            
            // Connect to exchange WebSocket
            match exchange.connect(&formatted_symbol).await {
                Ok(mut rx) => {
                    println!("Connected to {} WebSocket for {}", exchange.get_name(), formatted_symbol);
                    
                    // Fetch initial snapshot
                    match exchange.get_snapshot(&formatted_symbol).await {
                        Ok(snapshot) => {
                            println!("Snapshot fetched successfully from {}", exchange.get_name());
                            tx.send(AppMessage::Snapshot(snapshot)).unwrap();
                        }
                        Err(e) => println!("Snapshot request error: {e:?}"),
                    }
                    
                    // Process WebSocket messages
                    let tx_clone = tx.clone();
                    let ctx_clone = ctx.clone();
                    let ws_handle = tokio::spawn(async move {
                        while let Some(message) = rx.recv().await {
                            match message {
                                ExchangeMessage::Snapshot(snapshot) => {
                                    tx_clone.send(AppMessage::Snapshot(snapshot)).unwrap();
                                    ctx_clone.request_repaint();
                                }
                                ExchangeMessage::Update(update) => {
                                    tx_clone.send(AppMessage::Update(update)).unwrap();
                                    ctx_clone.request_repaint();
                                }
                            }
                        }
                    });

                    if let Some(ctrl) = control_rx.recv().await {
                        ws_handle.abort();
                        match ctrl {
                            Control::Refetch => {
                                println!("Refetch triggered, restarting connection.");
                            }
                            Control::ChangeSymbol(new_symbol) => {
                                symbol = new_symbol;
                                println!("Changing symbol to {symbol}, restarting connection.");
                            }
                            Control::ChangeExchange(new_exchange) => {
                                exchange_type = new_exchange;
                                println!("Changing exchange to {:?}, restarting connection.", exchange_type);
                            }
                        }
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    println!("Failed to connect to {} WebSocket: {e:?}", exchange.get_name());
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    fn process_update(&mut self, update: exchanges::DepthUpdate) {
        if update.small_u < self.last_applied_u {
            return;
        }

        if self.is_synced {
            if update.pu >= 0 && (update.pu as u64) != self.last_applied_u {
                println!(
                    "Warning: Message gap detected! pu: {}, last: {}",
                    update.pu, self.last_applied_u
                );
                self.update_buffer.clear();
                let _ = self.control_tx.try_send(Control::Refetch);
                return;
            }
            self.apply_update(&update);
            self.last_applied_u = update.small_u;
        } else if update.capital_u <= self.last_applied_u && self.last_applied_u <= update.small_u {
            self.apply_update(&update);
            self.last_applied_u = update.small_u;
            self.is_synced = true;
        } else {
            println!(
                "Initial gap detected! U: {}, u: {}, last: {}",
                update.capital_u, update.small_u, self.last_applied_u
            );
            self.update_buffer.clear();
            let _ = self.control_tx.try_send(Control::Refetch);
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::Snapshot(snap) => {
                    self.bids.clear();
                    self.asks.clear();
                    for bid in &snap.bids {
                        let price = bid[0];
                        let qty = bid[1];
                        if qty > Decimal::ZERO {
                            self.bids.insert(price, VecDeque::from(vec![qty]));
                        }
                    }
                    for ask in &snap.asks {
                        let price = ask[0];
                        let qty = ask[1];
                        if qty > Decimal::ZERO {
                            self.asks.insert(price, VecDeque::from(vec![qty]));
                        }
                    }
                    self.last_applied_u = snap.last_update_id;
                    self.is_synced = false;

                    while let Some(update) = self.update_buffer.pop_front() {
                        self.process_update(update);
                    }
                }
                AppMessage::Update(update) => {
                    if self.last_applied_u == 0 {
                        self.update_buffer.push_back(update);
                    } else {
                        self.process_update(update);
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!(
                "{} {} Perpetual Order Book",
                self.exchange_names[self.selected_exchange_idx],
                self.symbol.to_uppercase()
            ));
            if ui.button("Toggle K-Means Mode").clicked() {
                self.kmeans_mode = !self.kmeans_mode;
            }

            ui.horizontal(|ui| {
                ui.label("Exchange:");
                egui::ComboBox::from_label("")
                    .selected_text(self.exchange_names[self.selected_exchange_idx])
                    .show_ui(ui, |ui| {
                        for (i, &name) in self.exchange_names.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_exchange_idx, i, name).clicked() {
                                let new_exchange = match i {
                                    0 => ExchangeType::Binance,
                                    1 => ExchangeType::Hyperliquid,
                                    _ => ExchangeType::Binance,
                                };
                                if new_exchange as u8 != self.current_exchange as u8 {
                                    self.current_exchange = new_exchange;
                                    let exchange = self.current_exchange.create_exchange();
                                    let (price_prec, qty_prec) = exchange.get_precision(&self.symbol);
                                    self.price_prec = price_prec;
                                    self.qty_prec = qty_prec;
                                    
                                    // Update symbol for exchange-specific formats
                                    if matches!(new_exchange, ExchangeType::Hyperliquid) && self.symbol.contains("usdt") {
                                        self.symbol = "SOL".to_string(); // Default to SOL for Hyperliquid
                                        self.edited_symbol = self.symbol.clone();
                                    }
                                    
                                    let _ = self.control_tx.try_send(Control::ChangeExchange(new_exchange));
                                    self.bids.clear();
                                    self.asks.clear();
                                    self.last_applied_u = 0;
                                    self.is_synced = false;
                                }
                            }
                        }
                    });
            });
            
            ui.horizontal(|ui| {
                ui.label("Symbol:");
                ui.text_edit_singleline(&mut self.edited_symbol);
                if ui.button("Change Symbol").clicked() && self.edited_symbol != self.symbol {
                    let exchange = self.current_exchange.create_exchange();
                    let (price_prec, qty_prec) = exchange.get_precision(&self.edited_symbol);
                    self.price_prec = price_prec;
                    self.qty_prec = qty_prec;
                    
                    let _ = self
                        .control_tx
                        .try_send(Control::ChangeSymbol(self.edited_symbol.clone()));
                    self.symbol = self.edited_symbol.clone();
                    self.bids.clear();
                    self.asks.clear();
                    self.last_applied_u = 0;
                    self.is_synced = false;
                }
            });

            if self.kmeans_mode {
                ui.horizontal(|ui| {
                    ui.label("Batch Size:");
                    ui.add(egui::Slider::new(&mut self.batch_size, 32..=2048));
                });
                ui.horizontal(|ui| {
                    ui.label("Max Iter:");
                    ui.add(egui::Slider::new(&mut self.max_iter, 64..=2048));
                });
            }

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    egui::Grid::new("order_book_grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Asks");
                            ui.label("Price");
                            ui.label("Quantity");
                            ui.end_row();

                            for (price, qty) in self.asks.iter().take(20).rev() {
                                ui.label("");
                                ui.label(format!(
                                    "{:.1$}",
                                    price.to_f64().unwrap_or(0.0),
                                    self.price_prec
                                ));
                                ui.label(format!(
                                    "{:.1$}",
                                    qty.iter().sum::<Decimal>().to_f64().unwrap_or(0.0),
                                    self.qty_prec
                                ));
                                ui.end_row();
                            }

                            ui.label("Bids");
                            ui.label("Price");
                            ui.label("Quantity");
                            ui.end_row();

                            for (price, qty) in self.bids.iter().rev().take(20) {
                                ui.label("");
                                ui.label(format!(
                                    "{:.1$}",
                                    price.to_f64().unwrap_or(0.0),
                                    self.price_prec
                                ));
                                ui.label(format!(
                                    "{:.1$}",
                                    qty.iter().sum::<Decimal>().to_f64().unwrap_or(0.0),
                                    self.qty_prec
                                ));
                                ui.end_row();
                            }
                        });
                });

                ui.vertical(|ui| {
                    let bid_levels: Vec<(&Decimal, Decimal)> = self
                        .bids
                        .iter()
                        .rev()
                        .take(100)
                        .map(|(key, deque)| {
                            let sum = deque.iter().cloned().sum::<Decimal>(); // Sum the VecDeque<Decimal>
                            (key, sum)
                        })
                        .collect();
                    let ask_levels: Vec<(&Decimal, Decimal)> = self
                        .asks
                        .iter()
                        .take(100)
                        .map(|(key, deque)| {
                            let sum = deque.iter().cloned().sum::<Decimal>(); // Sum the VecDeque<Decimal>
                            (key, sum)
                        })
                        .collect();
                    let mut max_qty: f64 = 0.0;
                    for (_, qty) in &bid_levels {
                        max_qty = max_qty.max(qty.to_f64().unwrap_or(0.0));
                    }
                    for (_, qty) in &ask_levels {
                        max_qty = max_qty.max(qty.to_f64().unwrap_or(0.0));
                    }

                    let step = 1.0;
                    let mut bars: Vec<Bar> = Vec::new();

                    let max_bid_order: Decimal = self
                        .bids
                        .values()
                        .rev()
                        .take(100)
                        .flat_map(|dq| dq.iter())
                        .cloned()
                        .max()
                        .unwrap_or(Decimal::ZERO);
                    let max_ask_order: Decimal = self
                        .asks
                        .values()
                        .take(100)
                        .flat_map(|dq| dq.iter())
                        .cloned()
                        .max()
                        .unwrap_or(Decimal::ZERO);
                    let second_max_bid_order = {
                        let mut orders: Vec<_> = self
                            .bids
                            .values()
                            .rev()
                            .take(100)
                            .flat_map(|dq| dq.iter())
                            .cloned()
                            .collect();
                        orders.sort_by(|a, b| b.cmp(a)); // Sort in descending order
                        orders.get(1).cloned().unwrap_or(Decimal::ZERO)
                    };
                    let second_max_ask_order = {
                        let mut orders: Vec<_> = self
                            .asks
                            .values()
                            .take(100)
                            .flat_map(|dq| dq.iter())
                            .cloned()
                            .collect();
                        orders.sort_by(|a, b| b.cmp(a)); // Sort in descending order
                        orders.get(1).cloned().unwrap_or(Decimal::ZERO)
                    };

                    if !self.kmeans_mode {
                        for (i, (_, qty_deq)) in self.asks.iter().take(100).enumerate() {
                            let x = (i as f64 + 0.5) * step + 0.5;
                            let mut offset = 0.0;

                            for (j, &qty) in qty_deq.iter().enumerate() {
                                if qty <= dec!(0.0) {
                                    continue;
                                }
                                let color = if qty == max_ask_order {
                                    Color32::GOLD
                                } else if qty == second_max_ask_order {
                                    Color32::from_rgb(184, 134, 11)
                                } else {
                                    self.get_order_color(j, Color32::DARK_RED)
                                };
                                let bar = Bar::new(x, qty.to_f64().unwrap_or(0.0))
                                    .fill(color)
                                    .base_offset(offset)
                                    .width(step * 0.9);
                                bars.push(bar);
                                offset += qty.to_f64().unwrap_or(0.0);
                            }
                        }

                        // Color Mapping for Bids
                        for (i, (_, qty_deq)) in self.bids.iter().rev().take(100).enumerate() {
                            let x = -(i as f64 + 0.5) * step - 0.5;
                            let mut offset = 0.0;

                            for (j, &qty) in qty_deq.iter().enumerate() {
                                if qty <= dec!(0.0) {
                                    continue;
                                }
                                let color = if qty == max_bid_order {
                                    Color32::GOLD
                                } else if qty == second_max_bid_order {
                                    Color32::from_rgb(184, 134, 11)
                                } else {
                                    self.get_order_color(j, Color32::DARK_GREEN)
                                };
                                let bar = Bar::new(x, qty.to_f64().unwrap_or(0.0))
                                    .fill(color)
                                    .base_offset(offset)
                                    .width(step * 0.9);
                                bars.push(bar);
                                offset += qty.to_f64().unwrap_or(0.0);
                            }
                        }
                    } else {
                        let asks_for_cluster: BTreeMap<Decimal, VecDeque<Decimal>> = self
                            .asks
                            .iter()
                            .take(100)
                            .map(|(&k, v)| (k, v.clone()))
                            .collect();
                        let mut kmeans_asks =
                            kmeans::MiniBatchKMeans::new(10, self.batch_size, self.max_iter);
                        let labels_asks = kmeans_asks.fit(&asks_for_cluster);
                        let clustered_asks =
                            kmeans::build_clustered_orders(&asks_for_cluster, &labels_asks);

                        let bids_for_cluster: BTreeMap<Decimal, VecDeque<Decimal>> = self
                            .bids
                            .iter()
                            .rev()
                            .take(100)
                            .map(|(&k, v)| (k, v.clone()))
                            .collect();
                        let mut kmeans_bids =
                            kmeans::MiniBatchKMeans::new(10, self.batch_size, self.max_iter);
                        let labels_bids = kmeans_bids.fit(&bids_for_cluster);
                        let clustered_bids =
                            kmeans::build_clustered_orders(&bids_for_cluster, &labels_bids);

                        // Asks in K-Means mode
                        for (i, (_, qty_deq)) in clustered_asks.iter().enumerate() {
                            let x = (i as f64 + 0.5) * step + 0.5;
                            let mut offset = 0.0;

                            for &(qty, cluster) in qty_deq.iter() {
                                if qty <= dec!(0.0) {
                                    continue;
                                }
                                let color = if qty == max_ask_order {
                                    Color32::GOLD
                                } else {
                                    ASK_COLORS
                                        .get(cluster % ASK_COLORS.len())
                                        .cloned()
                                        .unwrap_or(Color32::GRAY)
                                };
                                let bar = Bar::new(x, qty.to_f64().unwrap_or(0.0))
                                    .fill(color)
                                    .base_offset(offset)
                                    .width(step * 0.9);
                                bars.push(bar);
                                offset += qty.to_f64().unwrap_or(0.0);
                            }
                        }

                        // Bids in K-Means mode
                        for (i, (_, qty_deq)) in clustered_bids.iter().rev().enumerate() {
                            let x = -(i as f64 + 0.5) * step - 0.5;
                            let mut offset = 0.0;

                            for &(qty, cluster) in qty_deq.iter() {
                                if qty <= dec!(0.0) {
                                    continue;
                                }
                                let color = if qty == max_bid_order {
                                    Color32::GOLD
                                } else {
                                    BID_COLORS
                                        .get(cluster % BID_COLORS.len())
                                        .cloned()
                                        .unwrap_or(Color32::GRAY)
                                };
                                let bar = Bar::new(x, qty.to_f64().unwrap_or(0.0))
                                    .fill(color)
                                    .base_offset(offset)
                                    .width(step * 0.9);
                                bars.push(bar);
                                offset += qty.to_f64().unwrap_or(0.0);
                            }
                        }
                    }

                    Plot::new("orderbook_chart")
                        .allow_drag(false)
                        .allow_scroll(false)
                        .allow_zoom(false)
                        .show_axes([true, true])
                        .show(ui, |plot_ui| {
                            plot_ui.bar_chart(BarChart::new("ob", bars));

                            for (i, (price, _)) in bid_levels.iter().enumerate() {
                                if i.is_multiple_of(20) {
                                    // Show label every 20th level
                                    let x = -(i as f64 + 0.5) * step - 0.5;
                                    plot_ui.text(
                                        Text::new(
                                            "bid",
                                            PlotPoint::new(x, -max_qty * 0.05),
                                            format!(
                                                "{:.1$}",
                                                price.to_f64().unwrap_or(0.0),
                                                self.price_prec
                                            ),
                                        )
                                        .anchor(Align2::CENTER_BOTTOM),
                                    );
                                }
                            }

                            for (i, (price, _)) in ask_levels.iter().enumerate() {
                                if i.is_multiple_of(20) {
                                    // Show label every 20th level
                                    if i == 0 {
                                        continue;
                                    }
                                    let x = (i as f64 + 0.5) * step + 0.5;
                                    plot_ui.text(
                                        Text::new(
                                            "ask",
                                            PlotPoint::new(x, -max_qty * 0.05),
                                            format!(
                                                "{:.1$}",
                                                price.to_f64().unwrap_or(0.0),
                                                self.price_prec
                                            ),
                                        )
                                        .anchor(Align2::CENTER_BOTTOM),
                                    );
                                }
                            }
                        });
                });
            });
        });
    }
}

impl MyApp {
    // Function to calculate color based on the order index
    fn get_order_color(&self, index: usize, base_color: Color32) -> Color32 {
        // Brighten the color by 5% for each order index
        let brightening_factor = 1.0 + 0.05 * index as f32; // 5% brighter per order
        let r = (base_color.r() as f32 * brightening_factor).min(255.0) as u8;
        let g = (base_color.g() as f32 * brightening_factor).min(255.0) as u8;
        let b = (base_color.b() as f32 * brightening_factor).min(255.0) as u8;

        Color32::from_rgb(r, g, b)
    }
}

impl MyApp {
    fn apply_update(&mut self, update: &exchanges::DepthUpdate) {
        for bid in &update.bids {
            let price = bid[0];
            let qty = bid[1];
            if qty == Decimal::ZERO {
                self.bids.remove(&price);
            } else {
                let price = bid[0];
                let qty = bid[1];
                if qty > Decimal::ZERO {
                    if let Some(old_qty) = self.bids.get_mut(&price) {
                        let old_sum = old_qty.iter().sum::<Decimal>();
                        if old_sum > qty {
                            let change = old_sum - qty;
                            if let Some(pos) = old_qty.iter().rposition(|&x| x == change) {
                                old_qty.remove(pos); // Removes the last occurrence of the value
                            } else {
                                let largest_order = *old_qty.iter().max().unwrap();
                                let largest_pos =
                                    old_qty.iter().position(|&x| x == largest_order).unwrap();
                                old_qty.remove(largest_pos);
                                old_qty.push_back(largest_order - change);
                            }
                        } else if old_sum < qty {
                            if old_sum < qty {
                                let change = qty - old_sum;
                                old_qty.push_back(change);
                            }
                        } else {
                            // ??
                            continue;
                        }
                    } else {
                        self.bids.insert(price, VecDeque::from(vec![qty]));
                    }
                }
            }
        }
        for ask in &update.asks {
            let price = ask[0];
            let qty = ask[1];
            if qty == Decimal::ZERO {
                self.asks.remove(&price);
            } else if let Some(old_qty) = self.asks.get_mut(&price) {
                let old_sum = old_qty.iter().sum::<Decimal>();
                if old_sum > qty {
                    let change = old_sum - qty;
                    if let Some(pos) = old_qty.iter().rposition(|&x| x == change) {
                        old_qty.remove(pos); // Removes the last occurrence of the value
                    } else {
                        let largest_order = *old_qty.iter().max().unwrap();
                        let largest_pos = old_qty.iter().position(|&x| x == largest_order).unwrap();
                        old_qty.remove(largest_pos);
                        old_qty.push_back(largest_order - change);
                    }
                } else if old_sum < qty {
                    if old_sum < qty {
                        let change = qty - old_sum;
                        old_qty.push_back(change);
                    }
                } else {
                    // ??
                    continue;
                }
            } else {
                self.asks.insert(price, VecDeque::from(vec![qty]));
            }
        }
    }
}