use serde::Serialize;

use crate::types::OrderBookSnapshot;

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum ServerEvent {
    Snapshot(OrderBookSnapshot),
    Heartbeat { timestamp: i64 },
}
