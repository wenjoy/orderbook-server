use crate::{
    api::settlement::SettlementService,
    engine::{self, MatchingEngine, book::OrderBook, order::Trade},
    event::ServerEvent,
};
use sqlx::PgPool;
use std::{collections::VecDeque, env, sync::Arc};
use tokio::sync::{RwLock, broadcast};

pub type SharedEngine = Arc<RwLock<MatchingEngine>>;

const MAX_RECENT_TRADES: usize = 200;

#[derive(Clone)]
pub struct AppState {
    pub engine: SharedEngine,
    pub recent_trades: Arc<RwLock<VecDeque<Trade>>>,
    pub tx: broadcast::Sender<ServerEvent>,
    pub settlement: SettlementService,
    pub pool: PgPool,
}
impl AppState {
    pub async fn new() -> Self {
        dotenv::dotenv().expect("load env config failed");
        let database_url = env::var("DATABASE_URL").expect("databse no config");
        let (tx, _) = broadcast::channel(1024);
        Self {
            engine: Arc::new(RwLock::new(MatchingEngine::new())),
            tx,
            recent_trades: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_RECENT_TRADES))),
            settlement: SettlementService::from_env(),
            pool: sqlx::postgres::PgPool::connect(&database_url)
                .await
                .expect("connect to database error"),
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
