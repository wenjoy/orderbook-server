use crate::{event::ServerEvent, orderbook::MatchingEngine};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

pub type SharedEngine = Arc<RwLock<MatchingEngine>>;

#[derive(Clone)]
pub struct AppState {
    pub engine: SharedEngine,
    pub tx: broadcast::Sender<ServerEvent>,
}
impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            engine: Arc::new(RwLock::new(MatchingEngine::new())),
            tx,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
