use serde::Serialize;

use crate::{
    api::settlement::SettlementStatus,
    engine::order::{DepthLevel, Order, OrderBookSnapshot, Side, Trade},
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
    //TODO: deprecated
    OrderUpdate(Order),
    OrderbookUpdate {
        bids: Vec<DepthLevel>,
        asks: Vec<DepthLevel>,
    },
    SettlementUpdate {
        settlement_id: u64,
        status: SettlementStatus,
        signature: Option<String>,
        trade_price: u64,
        trade_quantity: u64,
    },
}
