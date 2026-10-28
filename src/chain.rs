use core::hash;
use std::{env, fs, str::FromStr};

use solana_client::{rpc_client::RpcClient, rpc_response::transaction::containers::Vec};
use solana_sdk::{
    message::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};

use crate::api::settlement::pack_settle_trade;

pub fn load_keypair() -> Result<Keypair, String> {
    if let Ok(path) = env::var("SETTLEMENT_KEYPAIR") {
        // let data = fs::read_to_string(path)
        //     .map_err(|e| format!("read settlement keypair file error : {}", e))?;
        // return keypair_from_json(&data);
        let data =
            fs::read(path).map_err(|e| format!("read settlement keypair file error : {}", e))?;
        return keypair_from_bytes(&data);
    };

    if let Ok(json) = env::var("SETTLEMENT_KEYPAIR_JSON") {
        return keypair_from_json(&json);
    }
    Err("hot wallet require SETTLEMENT_KEYPAIR or SETTLEMENT_KEYPAIR_JSON".into())
}

fn keypair_from_json(data: &str) -> Result<Keypair, String> {
    let bytes: &[u8] =
        serde_json::from_str(data.trim()).map_err(|e| format!("parse keypari json error {}", e))?;
    keypair_from_bytes(bytes)
}

fn keypair_from_bytes(bytes: &[u8]) -> Result<Keypair, String> {
    Keypair::try_from(bytes).map_err(|e| format!("invalid keypair bytes {}", e))
}

fn parse_pubkey(s: &str, label: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(s).map_err(|e| format!("parse {label} pubkey error {e}"))
}

fn derive_user_vault(program_id: &Pubkey, market: &Pubkey, owner: &Pubkey) -> (Pubkey, u8) {
    let seeds = &[b"user_vault", market.as_ref(), owner.as_ref()];
    Pubkey::find_program_address(seeds, program_id)
}

pub struct SubmitParams {
    pub program_id: String,
    pub rpc_url: String,
    pub market: String,
    pub maker_user: String,
    pub taker_user: String,
    pub price: u64,
    pub quantity: u64,
    pub taker_is_buy: bool,
}

pub struct SubmitResult {
    pub signature: String,
    pub maker_vault: String,
    pub taker_vault: String,
    pub authority: String,
}

pub fn submit_settle_trade(params: SubmitParams) -> Result<SubmitResult, String> {
    let keypair = load_keypair()?;
    let authority = keypair.pubkey();

    let program_id = parse_pubkey(&params.program_id, "program_id")?;
    let market = parse_pubkey(&params.market, "market")?;
    let maker_owner = parse_pubkey(&params.maker_user, "maker_user")?;
    let taker_owner = parse_pubkey(&params.taker_user, "taker_user")?;

    let (maker_vault, _) = derive_user_vault(&program_id, &market, &maker_owner);
    let (taker_vault, _) = derive_user_vault(&program_id, &market, &taker_owner);

    let data = pack_settle_trade(params.price, params.quantity, params.taker_is_buy);
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(market, false),
            AccountMeta::new(maker_vault, false),
            AccountMeta::new(taker_vault, false),
        ],
        data,
    };
    let client = RpcClient::new(params.rpc_url);
    let recent = client
        .get_latest_blockhash()
        .map_err(|e| format!("get_latest_blockhash error: {e}"))?;
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&authority), &[keypair], recent);
    let sig = client
        .send_and_confirm_transaction(&tx)
        .map_err(|e| format!("send and confimr error: {}", e))?;
    Ok(SubmitResult {
        signature: sig.to_string(),
        authority: authority.to_string(),
        maker_vault: maker_vault.to_string(),
        taker_vault: taker_vault.to_string(),
    })
}

#[cfg(test)]
mod tests {}
