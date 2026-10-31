use std::{env, fs, str::FromStr};

use solana_client::rpc_client::RpcClient;
use solana_program::example_mocks::solana_sdk::system_program;
use solana_sdk::{
    message::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};

use crate::api::settlement::pack_settle_trade;

pub fn load_settle_keypair() -> Result<Keypair, String> {
    if let Ok(path) = env::var("SETTLEMENT_KEYPAIR") {
        let data = fs::read_to_string(path)
            .map_err(|e| format!("read settlement keypair file error : {}", e))?;
        return keypair_from_json(&data);
    };

    if let Ok(json) = env::var("SETTLEMENT_KEYPAIR_JSON") {
        return keypair_from_json(&json);
    }
    Err("hot wallet require SETTLEMENT_KEYPAIR or SETTLEMENT_KEYPAIR_JSON".into())
}

fn keypair_from_json(data: &str) -> Result<Keypair, String> {
    let bytes: std::vec::Vec<u8> = serde_json::from_str(data.trim())
        .map_err(|e| format!("parse keypair json error: {}", e))?;
    keypair_from_bytes(&bytes)
}

fn keypair_from_bytes(bytes: &[u8]) -> Result<Keypair, String> {
    Keypair::try_from(bytes).map_err(|e| format!("invalid keypair bytes {}", e))
}

fn parse_pubkey(s: &str, label: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(s).map_err(|e| format!("parse {label}-{s} pubkey error: {e}"))
}

fn derive_user_vault(program_id: &Pubkey, market: &Pubkey, owner: &Pubkey) -> (Pubkey, u8) {
    let seeds = &[b"user_vault", market.as_ref(), owner.as_ref()];
    Pubkey::find_program_address(seeds, program_id)
}
fn derive_market(program_id: &Pubkey, base_mint: &Pubkey, quote_mint: &Pubkey) -> (Pubkey, u8) {
    let seeds = &[b"market", base_mint.as_ref(), quote_mint.as_ref()];
    Pubkey::find_program_address(seeds, program_id)
}

#[derive(Debug)]
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
    println!("{:?}", params);
    let keypair = load_settle_keypair()?;
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

struct AccountMetaJson {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
}
//TODO: may not necessary
pub struct BuiltInstruction {
    // pub name: String,
    // pub program_id: String,
    // pub ix_data_hex: String,
    // pub accounts: Vec<AccountMetaJson>,
    // pub pdas: serde_json::Value,
    // pub required_signer: String,
}
fn pack_initialize_market() -> Vec<u8> {
    vec![0]
}
fn pack_initialize_vault() -> Vec<u8> {
    vec![1]
}
pub fn build_initialize_market(
    program_id: &str,
    authority: &str,
    base_mint: &str,
    quote_mint: &str,
) -> Result<(Instruction, BuiltInstruction), String> {
    let program_id = parse_pubkey(program_id, "program_id")?;
    let authority = parse_pubkey(authority, "authority")?;
    let base_mint = parse_pubkey(base_mint, "base_mint")?;
    let quote_mint = parse_pubkey(quote_mint, "quote_mint")?;

    let (market, _) = derive_market(&program_id, &base_mint, &quote_mint);
    println!("market pda: {}", market);

    let ix = Instruction {
        program_id,
        data: pack_initialize_market(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(market, false),
            AccountMeta::new_readonly(base_mint, false),
            AccountMeta::new_readonly(quote_mint, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
    };
    //TODO: implement if necessary
    let built = BuiltInstruction {};
    Ok((ix, built))
}
pub fn build_initialize_vault(
    program_id: &str,
    owner: &str,
    market: &str,
) -> Result<Instruction, String> {
    let program_id = parse_pubkey(program_id, "program_id")?;
    let owner = parse_pubkey(owner, "owner")?;
    let market = parse_pubkey(market, "market")?;
    let (user_vault, _) = derive_user_vault(&program_id, &market, &owner);

    let ix = Instruction {
        program_id,
        data: pack_initialize_vault(),
        accounts: vec![
            AccountMeta::new(owner, true),
            AccountMeta::new(user_vault, false),
            AccountMeta::new_readonly(market, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
    };

    Ok(ix)
}
pub fn submit_instruction(rpc_url: &str, ix: Instruction) -> Result<String, String> {
    let keypair = load_settle_keypair()?;
    let payer = keypair.pubkey();
    let client = RpcClient::new(rpc_url);
    let recent = client
        .get_latest_blockhash()
        .map_err(|e| format!("get recent error: {e}"))?;
    //TODO:  check 要求热钱包公钥 == 指令里 is_signer 的账户之一
    let signing_keypairs = &[keypair];

    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer), signing_keypairs, recent);
    let sig = client
        .send_and_confirm_transaction(&tx)
        .map_err(|e| format!("send tx error: {e}"))?;
    Ok(sig.to_string())
}
#[cfg(test)]
mod tests {}
