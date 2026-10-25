use std::{
    collections::HashMap,
    env,
    sync::{Arc, atomic::AtomicU64},
};

use serde::Serialize;
use tokio::sync::RwLock;

use crate::types::Trade;

#[derive(Clone, Serialize, Debug)]
pub enum SettlementStatus {
    Pending,
    Simulated,
    Submitted,
    Confirmed,
    Failed,
}

#[derive(Clone, Serialize)]
pub struct SettlementJob {
    pub id: u64,
    pub status: SettlementStatus,
    pub trade: Trade,
    pub ix_data_hex: String,
    pub accounts: SettlementAccounts,
    pub signature: Option<String>,
    pub error: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

enum SettlementMode {
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
#[derive(Clone, Serialize)]
pub struct SettlementAccounts {
    authority: String,
    market: String,
    maker: String,
    taker: String,
}

#[derive(Clone)]
pub struct SettlementConfig {
    pub mode: String,
    pub program_id: String,
    pub market_pubkey: String,
    pub authority_pubkey: String,
    pub rpc_url: String,
    pub base_mint: String,
    pub quote_mint: String,
}
impl SettlementConfig {
    fn from_env() -> Self {
        let mode = SettlementMode::from_env();
        Self {
            mode: match mode {
                SettlementMode::DryRun => "dry_run".into(),
                SettlementMode::Queue => "dry_run".into(),
                SettlementMode::HotWallet => "dry_run".into(),
            },
            program_id: env::var("SETTLEMENT_PROGRAM_ID")
                .unwrap_or("Settlement11111111111111111111111111111111".into()),
            market_pubkey: env::var("SETTLEMENT_MARKET")
                .unwrap_or_else(|_| "Market1111111111111111111111111111111111111".into()),
            authority_pubkey: env::var("SETTLEMENT_AUTHORITY")
                .unwrap_or_else(|_| "Authority111111111111111111111111111111111".into()),
            rpc_url: env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8899".into()),
            base_mint: env::var("BASE_MINT")
                .unwrap_or_else(|_| "BaseMint11111111111111111111111111111111111".into()),
            quote_mint: env::var("QUOTE_MINT")
                .unwrap_or_else(|_| "QuoteMint111111111111111111111111111111111".into()),
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

    pub fn from_env() -> Self {
        Self::new(SettlementConfig::from_env())
    }

    pub async fn list(&self, limit: usize) -> Vec<SettlementJob> {
        let map = self.jobs.read().await;
        let mut jobs: Vec<_> = map.values().cloned().collect();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
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
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
