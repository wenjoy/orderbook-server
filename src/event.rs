use serde::Serialize;

use crate::{
    api::settlement::SettlementStatus,
    types::{Order, OrderBookSnapshot, Side, Trade},
};

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
    SettlementUpdate {
        settlement_id: u64,
        status: SettlementStatus,
        signature: Option<String>,
        trade_price: u64,
        trade_quantity: u64,
    },
}
