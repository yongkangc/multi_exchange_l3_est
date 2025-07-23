use super::{DepthUpdate, Exchange, ExchangeMessage, OrderBookSnapshot};
use futures_util::{SinkExt, StreamExt};
use reqwest::blocking;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::VecDeque;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};

#[derive(Deserialize)]
struct BinanceExchangeInfo {
    symbols: Vec<BinanceSymbolInfo>,
}

#[derive(Deserialize)]
struct BinanceSymbolInfo {
    symbol: String,
    filters: Vec<BinanceFilter>,
}

#[derive(Deserialize)]
struct BinanceFilter {
    #[serde(rename = "filterType")]
    filter_type: String,
    #[serde(rename = "tickSize")]
    tick_size: Option<String>,
    #[serde(rename = "stepSize")]
    step_size: Option<String>,
}

#[derive(Deserialize)]
struct BinanceOrderBookSnapshot {
    #[serde(rename = "lastUpdateId")]
    last_update_id: u64,
    bids: Vec<Vec<Decimal>>,
    asks: Vec<Vec<Decimal>>,
}

#[derive(Deserialize, Clone)]
struct BinanceDepthUpdate {
    e: String,
    #[serde(rename = "E")]
    event_time: u64,
    #[serde(rename = "T")]
    transaction_time: u64,
    s: String,
    #[serde(rename = "U")]
    capital_u: u64,
    #[serde(rename = "u")]
    small_u: u64,
    pu: i64,
    b: Vec<Vec<Decimal>>,
    a: Vec<Vec<Decimal>>,
}

pub struct BinanceExchange {}

impl BinanceExchange {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl Exchange for BinanceExchange {
    async fn connect(&self, symbol: &str) -> Result<Receiver<ExchangeMessage>, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(1000);
        let ws_url = format!("wss://fstream.binance.com/ws/{}@depth@0ms", symbol.to_lowercase());
        let symbol = symbol.to_string();

        tokio::spawn(async move {
            if let Ok((ws_stream, _)) = connect_async(&ws_url).await {
                let (_, mut read) = ws_stream.split();
                
                while let Some(message) = read.next().await {
                    match message {
                        Ok(WsMessage::Text(text)) => {
                            if let Ok(update) = serde_json::from_str::<BinanceDepthUpdate>(&text) {
                                let depth_update = DepthUpdate {
                                    event_time: update.event_time,
                                    transaction_time: update.transaction_time,
                                    symbol: update.s,
                                    capital_u: update.capital_u,
                                    small_u: update.small_u,
                                    pu: update.pu,
                                    bids: update.b,
                                    asks: update.a,
                                };
                                let _ = tx.send(ExchangeMessage::Update(depth_update)).await;
                            }
                        }
                        Ok(WsMessage::Ping(payload)) => {
                            // Handle ping if needed
                        }
                        Ok(WsMessage::Close(_)) => break,
                        Err(e) => {
                            println!("Binance WebSocket error: {:?}", e);
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
        let url = format!(
            "https://fapi.binance.com/fapi/v1/depth?symbol={}&limit=1000",
            symbol.to_uppercase()
        );
        
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        let snapshot: BinanceOrderBookSnapshot = response.json().await?;
        
        Ok(OrderBookSnapshot {
            last_update_id: snapshot.last_update_id,
            bids: snapshot.bids,
            asks: snapshot.asks,
        })
    }

    fn get_precision(&self, symbol: &str) -> (usize, usize) {
        let mut price_prec = 2;
        let mut qty_prec = 2;
        
        let url = "https://fapi.binance.com/fapi/v1/exchangeInfo".to_string();
        if let Ok(resp) = blocking::get(&url) {
            if let Ok(info) = resp.json::<BinanceExchangeInfo>() {
                if let Some(sym_info) = info.symbols.into_iter().find(|s| s.symbol == symbol.to_uppercase()) {
                    for filter in sym_info.filters {
                        if filter.filter_type == "PRICE_FILTER" {
                            if let Some(ts) = filter.tick_size {
                                let tick_size = ts.parse::<f64>().unwrap_or(1.0);
                                if tick_size > 0.0 {
                                    price_prec = (-tick_size.log10()).ceil() as usize;
                                }
                            }
                        } else if filter.filter_type == "LOT_SIZE" {
                            if let Some(ss) = filter.step_size {
                                let step_size = ss.parse::<f64>().unwrap_or(1.0);
                                if step_size > 0.0 {
                                    qty_prec = (-step_size.log10()).ceil() as usize;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        (price_prec, qty_prec)
    }

    fn format_symbol(&self, symbol: &str) -> String {
        symbol.to_lowercase()
    }

    fn get_name(&self) -> &'static str {
        "Binance"
    }
}