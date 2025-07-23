pub mod binance;
pub mod hyperliquid;

use rust_decimal::Decimal;
use serde::Deserialize;
use std::collections::{BTreeMap, VecDeque};
use tokio::sync::mpsc::Receiver;

#[derive(Clone, Debug)]
pub enum ExchangeMessage {
    Snapshot(OrderBookSnapshot),
    Update(DepthUpdate),
}

#[derive(Deserialize, Clone, Debug)]
pub struct OrderBookSnapshot {
    pub last_update_id: u64,
    pub bids: Vec<Vec<Decimal>>,
    pub asks: Vec<Vec<Decimal>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DepthUpdate {
    pub event_time: u64,
    pub transaction_time: u64,
    pub symbol: String,
    pub capital_u: u64,
    pub small_u: u64,
    pub pu: i64,
    pub bids: Vec<Vec<Decimal>>,
    pub asks: Vec<Vec<Decimal>>,
}

#[async_trait::async_trait]
pub trait Exchange: Send + Sync {
    async fn connect(&self, symbol: &str) -> Result<Receiver<ExchangeMessage>, Box<dyn std::error::Error>>;
    async fn get_snapshot(&self, symbol: &str) -> Result<OrderBookSnapshot, Box<dyn std::error::Error>>;
    fn get_precision(&self, symbol: &str) -> (usize, usize);
    fn format_symbol(&self, symbol: &str) -> String;
    fn get_name(&self) -> &'static str;
}

#[derive(Clone, Copy, Debug)]
pub enum ExchangeType {
    Binance,
    Hyperliquid,
}

impl ExchangeType {
    pub fn create_exchange(&self) -> Box<dyn Exchange> {
        match self {
            ExchangeType::Binance => Box::new(binance::BinanceExchange::new()),
            ExchangeType::Hyperliquid => Box::new(hyperliquid::HyperliquidExchange::new()),
        }
    }
}