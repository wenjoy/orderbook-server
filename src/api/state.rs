use crate::{
    api::settlement::SettlementService,
    engine::{MatchingEngine, order::Trade},
    event::ServerEvent,
    persist,
};
use reqwest::Client;
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
    pub rpc_proxy: ProxyConfig,
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

        let rpc_proxy = ProxyConfig::from_env();

        Self {
            rpc_proxy,
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

#[derive(Clone)]
pub struct ProxyConfig {
    pub client: Client,
    pub rpc_url: String,
}

impl ProxyConfig {
    pub fn from_env() -> Self {
        let rpc_url = env::var("SOLANA_RPC_URL").expect("solana rpc url must be set in .env");

        Self {
            client: Client::new(),
            rpc_url,
        }
    }
}
