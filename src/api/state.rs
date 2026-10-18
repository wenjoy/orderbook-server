use crate::orderbook::MatchingEngine;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedEngine = Arc<RwLock<MatchingEngine>>;

#[derive(Clone)]
pub struct AppState {
    pub engine: SharedEngine,
}
impl AppState {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(RwLock::new(MatchingEngine::new())),
        }
    }
}
