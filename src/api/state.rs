use crate::{
    api::settlement::SettlementService, event::ServerEvent, orderbook::MatchingEngine, types::Trade,
};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

pub type SharedEngine = Arc<RwLock<MatchingEngine>>;

#[derive(Clone)]
pub struct AppState {
    pub engine: SharedEngine,
    pub tx: broadcast::Sender<ServerEvent>,
    pub settlement: SettlementService,
}
impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            engine: Arc::new(RwLock::new(MatchingEngine::new())),
            tx,
            settlement: SettlementService::from_env(),
        }
    }

    pub fn broadcast(&self, event: ServerEvent) {
        self.tx.send(event);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
