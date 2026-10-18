use serde::Serialize;

use crate::types::{Order, OrderBookSnapshot, Side, Trade};

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum ServerEvent {
    Snapshot(OrderBookSnapshot),
    DepthUpdate {
        side: Side,
        price: u64,
        quantity: u64,
        sequence: u64,
    },
    Heartbeat {
        timestamp: i64,
    },
    Trade(Trade),
    OrderUpdate(Order),
}
