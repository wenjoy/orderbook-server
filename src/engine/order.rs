use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub id: Uuid,
    pub side: Side,
    pub price: Option<u64>,
    pub quantity: u64,
    pub timestamp: DateTime<Utc>,
    pub user: String,
    pub filled_quantity: u64,
    pub status: OrderStatus,
    pub market: String,
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

#[derive(Clone, Debug, Serialize)]
pub struct Trade {
    pub id: Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub price: u64,
    pub quantity: u64,
    pub timestamp: DateTime<Utc>,
    pub side: Side, // taker 的方向
    pub taker_user: String,
    pub maker_user: String,
    pub market: String,
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
