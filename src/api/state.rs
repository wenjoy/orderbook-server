use crate::{
    api::settlement::SettlementService,
    engine::{MatchingEngine, order::Trade},
    event::ServerEvent,
    persist,
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
    pub fn broadcast(&self, event: ServerEvent) {
        let _ = self.tx.send(event);
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
        for job in jobs {
            self.broadcast(ServerEvent::SettlementUpdate {
                settlement_id: job.id,
                status: job.status,
                signature: job.signature,
                trade_price: job.trade.price,
                trade_quantity: job.trade.quantity,
            });
        }
    }

    pub async fn load_persisted() -> Self {
        dotenv::dotenv().expect("load env config failed");
        let database_url = env::var("DATABASE_URL").expect("databse no config");
        let snap = persist::load().await;

        let mut engine = MatchingEngine::new();
        engine.restore_order(&snap);

        let mut recent = VecDeque::with_capacity(MAX_RECENT_TRADES);
        for t in snap.recent_trades.iter().take(MAX_RECENT_TRADES) {
            recent.push_back(t.clone());
        }

        let settlement = SettlementService::from_env();
        settlement.load_snapshot(snap.settlements).await;
        let (tx, _) = broadcast::channel(1024);

        Self {
            tx,
            engine: Arc::new(RwLock::new(engine)),
            recent_trades: Arc::new(RwLock::new(recent)),
            settlement,
            pool: sqlx::postgres::PgPool::connect(&database_url)
                .await
                .expect("connect to database error"),
        }
    }

    pub async fn persist(&self) {
        let book = self.engine.read().await.book.clone();
        let recent = self.recent_trades.read().await;
        persist::persist_now(&book, &recent, &self.settlement).await;
    }
}
