use crate::orderbook::MatchingEngine;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedEngine = Arc<RwLock<MatchingEngine>>;
