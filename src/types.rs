use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Side {
    Bid,
    Ask,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub id: Uuid,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Option<u64>, // Market 单可以为 None，单位：最小价格单位（比如 0.01 USDC = 1）
    pub quantity: u64,      // 原始数量
    pub filled_quantity: u64, // 已成交数量
    pub status: OrderStatus,
    pub timestamp: DateTime<Utc>,
}

impl Order {
    pub fn remaining(&self) -> u64 {
        self.quantity.saturating_sub(self.filled_quantity)
    }
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            OrderStatus::Open | OrderStatus::PartiallyFilled
        )
    }
}

#[derive(Clone, PartialEq, Debug, Serialize)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Clone)]
pub struct Trade {
    pub id: Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub price: u64,
    pub quantity: u64,
    pub timestamp: DateTime<Utc>,
    pub side: Side, // taker 的方向
}

#[derive(Debug, Clone, Serialize)]
pub struct DepthLevel {
    pub price: u64,
    pub quantity: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderBookSnapshot {
    pub bids: Vec<DepthLevel>,
    pub asks: Vec<DepthLevel>,
    pub sequence: u64,
}
