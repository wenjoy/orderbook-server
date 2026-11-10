use std::{
    collections::HashMap,
    env,
    sync::{Arc, atomic::AtomicU64},
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    chain::{SubmitParams, submit_settle_trade},
    engine::order::{Side, Trade},
};

#[derive(Clone, Serialize, Debug, Deserialize)]
pub enum SettlementStatus {
    Pending,
    Simulated,
    Submitted,
    Confirmed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementJob {
    pub id: u64,
    pub status: SettlementStatus,
    pub trade: Trade,
    pub ix_data_hex: String,
    pub signature: Option<String>,
    pub error: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Serialize)]
pub enum SettlementMode {
    DryRun,
    Queue,
    HotWallet,
}
impl SettlementMode {
    fn from_env() -> Self {
        match env::var("SETTLEMENT_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "queue" => Self::Queue,
            "hot_wallet" | "hotwallet" => Self::HotWallet,
            _ => Self::DryRun,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SettlementAccounts {
    authority: String,
    market: String,
    maker_vault: String,
    taker_vault: String,
}

#[derive(Clone, Serialize)]
pub struct SettlementConfig {
    pub mode: SettlementMode,
    pub program_id: String,
    pub rpc_url: String,
}
impl SettlementConfig {
    fn from_env() -> Self {
        let mode = SettlementMode::from_env();
        Self {
            mode,
            program_id: env::var("SETTLEMENT_PROGRAM_ID").unwrap(),
            rpc_url: env::var("SOLANA_RPC_URL").unwrap(),
        }
    }
}
#[derive(Clone)]
pub struct SettlementService {
    pub config: SettlementConfig,
    jobs: Arc<RwLock<HashMap<u64, SettlementJob>>>,
    next_id: Arc<AtomicU64>,
}
impl SettlementService {
    fn new(config: SettlementConfig) -> Self {
        Self {
            config,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    async fn create_job(&self, trade: Trade) -> SettlementJob {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let taker_is_buy = matches!(trade.side, Side::Bid);
        let ix_data = pack_settle_trade(trade.price, trade.quantity, taker_is_buy);
        let ts = now_ms();

        SettlementJob {
            id,
            status: SettlementStatus::Pending,
            trade: trade.clone(),
            ix_data_hex: hex_encode(&ix_data),
            signature: None,
            error: None,
            created_at_ms: ts,
            updated_at_ms: ts,
        }
    }

    async fn process_job(&self, mut job: SettlementJob) -> SettlementJob {
        match self.config.mode {
            //TODO: get rid of DryRun
            SettlementMode::DryRun => {
                job.status = SettlementStatus::Simulated;
                job.signature = Some(format!("dry_run_sig_{}", job.id));
                job.updated_at_ms = now_ms();
            }
            SettlementMode::Queue => {
                //stay Pending
                println!("job id: {}, settlement queued", job.id);
            }
            SettlementMode::HotWallet => {
                job = self.submit_hot_wallet(job).await;
            }
        }

        let mut map = self.jobs.write().await;
        map.insert(job.id, job.clone());
        job
    }

    async fn submit_hot_wallet(&self, mut job: SettlementJob) -> SettlementJob {
        let config = &self.config;
        let trade = &job.trade;
        let taker_is_buy = matches!(trade.side, Side::Bid);

        let params = SubmitParams {
            program_id: config.program_id.clone(),
            rpc_url: config.rpc_url.clone(),
            market: trade.market.to_string(),
            maker_user: trade.maker_user.clone(),
            taker_user: trade.taker_user.clone(),
            price: trade.price,
            quantity: trade.quantity,
            taker_is_buy,
        };
        let result = submit_settle_trade(params);
        job.updated_at_ms = now_ms();

        match result {
            Ok(res) => {
                job.status = SettlementStatus::Confirmed;
                job.signature = Some(res.signature);
                job.error = None;
            }
            Err(e) => {
                job.status = SettlementStatus::Failed;
                job.error = Some(format!("join: {e}"));
                eprintln!("submit to hot wallet error: {e}");
            }
        }
        job
    }

    pub async fn enqueue_trades(&self, trades: &[Trade]) -> Vec<SettlementJob> {
        let mut created = Vec::with_capacity(trades.len());
        for trade in trades {
            let job = self.create_job(trade.clone()).await;
            let job = self.process_job(job).await;
            created.push(job);
        }
        created
    }

    pub fn from_env() -> Self {
        Self::new(SettlementConfig::from_env())
    }

    pub async fn list(&self, limit: usize) -> Vec<SettlementJob> {
        let map = self.jobs.read().await;
        let mut jobs: Vec<_> = map.values().cloned().collect();
        jobs.sort_by_key(|a| a.id);
        jobs.truncate(limit);
        jobs
    }

    pub async fn get(&self, id: u64) -> Option<SettlementJob> {
        self.jobs.read().await.get(&id).cloned()
    }

    pub async fn confirm(
        &self,
        id: u64,
        signature: String,
        success: bool,
        error: Option<String>,
    ) -> Result<SettlementJob, String> {
        let mut map = self.jobs.write().await;
        let job = map
            .get_mut(&id)
            .ok_or(format!("settlement id {} not found", id))?;

        job.updated_at_ms = now_ms();
        if success {
            job.status = SettlementStatus::Confirmed;
            job.signature = Some(signature);
            job.error = None;
        } else {
            job.status = SettlementStatus::Failed;
            job.signature = Some(signature);
            job.error = error.or(Some("confirm reported failure".into()));
        }
        Ok(job.clone())
    }

    pub async fn load_snapshot(&self, jobs: Vec<SettlementJob>) {
        //TODO: handle next id
        let mut map = self.jobs.write().await;
        map.clear();
        for j in jobs {
            map.insert(j.id, j);
        }
    }

    pub async fn snapshot_state(&self) -> (u64, Vec<SettlementJob>) {
        //TODO:replace with real next
        let next = 1;
        let map = self.jobs.read().await;
        let jobs = map.values().cloned().collect();
        (next, jobs)
    }
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn pack_settle_trade(price: u64, quantity: u64, taker_is_buy: bool) -> Vec<u8> {
    let mut created = Vec::with_capacity(18);
    created.push(4);
    created.extend_from_slice(&price.to_le_bytes());
    created.extend_from_slice(&quantity.to_le_bytes());
    created.push(u8::from(taker_is_buy));
    created
}
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn placeholder_vault(program_id: &str, market: &str, user: &str) -> String {
    format!(
        "vault:{}:{}:{}",
        &program_id[..8.min(program_id.len())],
        &market[..8.min(market.len())],
        user
    )
}
