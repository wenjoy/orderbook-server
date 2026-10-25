use crate::{
    api::settlement::SettlementService, event::ServerEvent, orderbook::MatchingEngine, types::Trade,
};
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{RwLock, broadcast};

pub type SharedEngine = Arc<RwLock<MatchingEngine>>;

const MAX_RECENT_TRADES: usize = 200;

#[derive(Clone)]
pub struct AppState {
    pub engine: SharedEngine,
    pub recent_trades: Arc<RwLock<VecDeque<Trade>>>,
    pub tx: broadcast::Sender<ServerEvent>,
    pub settlement: SettlementService,
}
impl AppState {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            engine: Arc::new(RwLock::new(MatchingEngine::new())),
            tx,
            recent_trades: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_RECENT_TRADES))),
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
