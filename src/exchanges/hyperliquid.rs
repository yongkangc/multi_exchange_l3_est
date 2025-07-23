use super::{DepthUpdate, Exchange, ExchangeMessage, OrderBookSnapshot};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio::sync::mpsc::{self, Receiver};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};

#[derive(Serialize)]
struct HyperliquidSubscription {
    method: String,
    subscription: HyperliquidSubscriptionData,
}

#[derive(Serialize)]
struct HyperliquidSubscriptionData {
    #[serde(rename = "type")]
    sub_type: String,
    coin: String,
}

#[derive(Deserialize)]
struct HyperliquidWsBook {
    coin: String,
    levels: [Vec<HyperliquidWsLevel>; 2], // [bids, asks]
    time: u64,
}

#[derive(Deserialize)]
struct HyperliquidWsLevel {
    px: String,  // price
    sz: String,  // size
    n: u32,      // number of orders
}

#[derive(Serialize)]
struct HyperliquidInfoRequest {
    #[serde(rename = "type")]
    req_type: String,
    coin: String,
}

#[derive(Deserialize)]
struct HyperliquidL2Book {
    levels: [Vec<HyperliquidLevel>; 2], // [bids, asks]
}

#[derive(Deserialize)]
struct HyperliquidLevel {
    px: String,
    sz: String,
    n: u32,
}

pub struct HyperliquidExchange {}

impl HyperliquidExchange {
    pub fn new() -> Self {
        Self {}
    }

    fn convert_ws_book_to_snapshot(&self, book: HyperliquidWsBook) -> OrderBookSnapshot {
        let mut bids = Vec::new();
        let mut asks = Vec::new();

        // Convert bids (index 0)
        for level in &book.levels[0] {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str(&level.px),
                Decimal::from_str(&level.sz),
            ) {
                bids.push(vec![price, size]);
            }
        }

        // Convert asks (index 1)
        for level in &book.levels[1] {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str(&level.px),
                Decimal::from_str(&level.sz),
            ) {
                asks.push(vec![price, size]);
            }
        }

        OrderBookSnapshot {
            last_update_id: book.time,
            bids,
            asks,
        }
    }

    fn convert_ws_book_to_update(&self, book: HyperliquidWsBook) -> DepthUpdate {
        let mut bids = Vec::new();
        let mut asks = Vec::new();

        // Convert bids
        for level in &book.levels[0] {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str(&level.px),
                Decimal::from_str(&level.sz),
            ) {
                bids.push(vec![price, size]);
            }
        }

        // Convert asks
        for level in &book.levels[1] {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str(&level.px),
                Decimal::from_str(&level.sz),
            ) {
                asks.push(vec![price, size]);
            }
        }

        DepthUpdate {
            event_time: book.time,
            transaction_time: book.time,
            symbol: book.coin.clone(),
            capital_u: book.time,
            small_u: book.time,
            pu: (book.time - 1) as i64,
            bids,
            asks,
        }
    }
}

#[async_trait::async_trait]
impl Exchange for HyperliquidExchange {
    async fn connect(&self, symbol: &str) -> Result<Receiver<ExchangeMessage>, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(1000);
        let ws_url = "wss://api.hyperliquid.xyz/ws";
        let symbol = symbol.to_uppercase();

        tokio::spawn(async move {
            if let Ok((ws_stream, _)) = connect_async(ws_url).await {
                let (mut write, mut read) = ws_stream.split();
                
                // Subscribe to order book
                let subscription = HyperliquidSubscription {
                    method: "subscribe".to_string(),
                    subscription: HyperliquidSubscriptionData {
                        sub_type: "l2Book".to_string(),
                        coin: symbol.clone(),
                    },
                };

                if let Ok(sub_msg) = serde_json::to_string(&subscription) {
                    let _ = write.send(WsMessage::Text(sub_msg.into())).await;
                }

                let mut first_message = true;
                while let Some(message) = read.next().await {
                    match message {
                        Ok(WsMessage::Text(text)) => {
                            if let Ok(book) = serde_json::from_str::<HyperliquidWsBook>(&text) {
                                if first_message {
                                    // Send first message as snapshot
                                    let snapshot = OrderBookSnapshot {
                                        last_update_id: book.time,
                                        bids: book.levels[0]
                                            .iter()
                                            .filter_map(|level| {
                                                match (Decimal::from_str(&level.px), Decimal::from_str(&level.sz)) {
                                                    (Ok(price), Ok(size)) => Some(vec![price, size]),
                                                    _ => None,
                                                }
                                            })
                                            .collect(),
                                        asks: book.levels[1]
                                            .iter()
                                            .filter_map(|level| {
                                                match (Decimal::from_str(&level.px), Decimal::from_str(&level.sz)) {
                                                    (Ok(price), Ok(size)) => Some(vec![price, size]),
                                                    _ => None,
                                                }
                                            })
                                            .collect(),
                                    };
                                    let _ = tx.send(ExchangeMessage::Snapshot(snapshot)).await;
                                    first_message = false;
                                } else {
                                    // Send subsequent messages as updates
                                    let update = DepthUpdate {
                                        event_time: book.time,
                                        transaction_time: book.time,
                                        symbol: book.coin.clone(),
                                        capital_u: book.time,
                                        small_u: book.time,
                                        pu: (book.time - 1) as i64,
                                        bids: book.levels[0]
                                            .iter()
                                            .filter_map(|level| {
                                                match (Decimal::from_str(&level.px), Decimal::from_str(&level.sz)) {
                                                    (Ok(price), Ok(size)) => Some(vec![price, size]),
                                                    _ => None,
                                                }
                                            })
                                            .collect(),
                                        asks: book.levels[1]
                                            .iter()
                                            .filter_map(|level| {
                                                match (Decimal::from_str(&level.px), Decimal::from_str(&level.sz)) {
                                                    (Ok(price), Ok(size)) => Some(vec![price, size]),
                                                    _ => None,
                                                }
                                            })
                                            .collect(),
                                    };
                                    let _ = tx.send(ExchangeMessage::Update(update)).await;
                                }
                            }
                        }
                        Ok(WsMessage::Ping(payload)) => {
                            let _ = write.send(WsMessage::Pong(payload)).await;
                        }
                        Ok(WsMessage::Close(_)) => break,
                        Err(e) => {
                            println!("Hyperliquid WebSocket error: {:?}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn get_snapshot(&self, symbol: &str) -> Result<OrderBookSnapshot, Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let url = "https://api.hyperliquid.xyz/info";
        
        let request = HyperliquidInfoRequest {
            req_type: "l2Book".to_string(),
            coin: symbol.to_uppercase(),
        };

        let response = client.post(url).json(&request).send().await?;
        let l2_book: HyperliquidL2Book = response.json().await?;

        let mut bids = Vec::new();
        let mut asks = Vec::new();

        // Convert bids
        for level in &l2_book.levels[0] {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str(&level.px),
                Decimal::from_str(&level.sz),
            ) {
                bids.push(vec![price, size]);
            }
        }

        // Convert asks
        for level in &l2_book.levels[1] {
            if let (Ok(price), Ok(size)) = (
                Decimal::from_str(&level.px),
                Decimal::from_str(&level.sz),
            ) {
                asks.push(vec![price, size]);
            }
        }

        Ok(OrderBookSnapshot {
            last_update_id: chrono::Utc::now().timestamp_millis() as u64,
            bids,
            asks,
        })
    }

    fn get_precision(&self, _symbol: &str) -> (usize, usize) {
        // Hyperliquid typically uses higher precision
        // This could be made dynamic by fetching from API
        (4, 4)
    }

    fn format_symbol(&self, symbol: &str) -> String {
        symbol.to_uppercase()
    }

    fn get_name(&self) -> &'static str {
        "Hyperliquid"
    }
}