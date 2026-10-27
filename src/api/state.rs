use crate::{
    api::settlement::SettlementService,
    engine::{self, MatchingEngine, book::OrderBook, order::Trade},
    event::ServerEvent,
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

    pub async fn broadcast_orderbook(&self) {
        let engine = self.engine.read().await;
        let book = &engine.book;
        let event = ServerEvent::OrderbookUpdate {
            bids: book.bid_depth(20),
            asks: book.ask_depth(20),
        };
        self.broadcast(event);
    }

    pub async fn record_trades(&self, trades: &Vec<Trade>) {
        if trades.is_empty() {
            return;
        }

        let mut recent = self.recent_trades.write().await;
        for t in trades {
            if recent.len() > MAX_RECENT_TRADES {
                recent.pop_front();
            }
            recent.push_back(t.clone());
        }

        let jobs = self.settlement.enqueue_trades(trades).await;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
