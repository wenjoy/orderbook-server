use std::str::FromStr;

use axum::{
    Json,
    body::Bytes,
    extract::{
        Path, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use reqwest::header;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signer::Signer};
use uuid::Uuid;

use crate::{
    api::{
        settlement::{SettlementJob, SettlementStatus},
        state::{AppState, ProxyConfig},
    },
    chain::{
        build_initialize_market, build_initialize_vault, load_settle_keypair, submit_instruction,
        transfer_token,
    },
    engine::order::{Order, OrderBookSnapshot, OrderStatus, Side, Trade},
    error::{ApiError, ApiResult},
    event::ServerEvent,
};

#[derive(Deserialize)]
pub struct PlaceOrderRequst {
    pub side: Side,
    pub price: Option<u64>,
    pub quantity: u64,
    pub user: String,
    pub market: String,
}

#[derive(Deserialize)]
pub struct ListOrderRequest {
    user: Option<String>,
}

#[derive(Serialize)]
pub struct ListOrderResponse {
    id: Uuid,
    side: Side,
    price: u64,
    user: String,
    remaining: u64,
}
#[axum::debug_handler]
pub async fn list_orders(
    State(state): State<AppState>,
    Query(req): Query<ListOrderRequest>,
) -> Result<Json<Vec<ListOrderResponse>>, StatusCode> {
    let user_filter = req.user.as_deref();
    let engine = state.engine.read().await;
    let open_orders = engine
        .open_orders(user_filter)
        .iter()
        .map(|o| ListOrderResponse {
            id: o.id,
            price: o.price.unwrap_or_default(),
            side: o.side.clone(),
            user: o.user.clone(),
            remaining: o.remaining(),
        })
        .collect();

    Ok(Json(open_orders))
}

#[axum::debug_handler]
pub async fn place_order(
    State(state): State<AppState>,
    Json(req): Json<PlaceOrderRequst>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let order = Order {
        id: Uuid::new_v4(),
        side: req.side,
        user: req.user,
        price: req.price,
        quantity: req.quantity,
        filled_quantity: 0,
        status: OrderStatus::Open,
        timestamp: chrono::Utc::now(),
        market: req.market,
    };

    let mut engine = state.engine.write().await;
    match engine.place_order(order.clone()) {
        Ok((order, trades)) => {
            //NOTE: easy to fall into dead lock trap
            drop(engine);
            for trade in &trades {
                let _ = state.tx.send(ServerEvent::Trade(trade.clone()));
            }

            let order_id = order.id;
            let remaining = order.remaining();

            state.record_trades(&trades).await;
            state.broadcast_orderbook().await;

            state.persist().await;

            Ok(Json(serde_json::json!({
                "order_id": order_id,
                "remaining": remaining,
                "trades": trades
            })))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}
#[axum::debug_handler]
pub async fn cancel_order(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Order>, StatusCode> {
    let order_id = id;
    let mut engine = state.engine.write().await;

    match engine.cancel_order(order_id) {
        Ok(order) => {
            drop(engine);
            let event = ServerEvent::OrderCancelled { order_id: order.id };
            state.broadcast(event);
            state.broadcast_orderbook().await;
            state.persist().await;
            Ok(Json(order))
        }
        Err(e) => {
            eprintln!("cancel order failed {:?}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

#[axum::debug_handler]
pub async fn get_orderbook(
    State(state): State<AppState>,
) -> Result<Json<OrderBookSnapshot>, StatusCode> {
    let engine = state.engine.write().await;

    let snap = engine.get_snapshot(10);

    Ok(Json(snap))
}

#[derive(Serialize, Debug, Clone)]
pub struct TradesResponse {
    pub trades: Vec<Trade>,
}
#[axum::debug_handler]
pub async fn get_recent_trades(State(state): State<AppState>) -> ApiResult<Json<TradesResponse>> {
    let trades = state.recent_trades.read().await;
    Ok(Json(TradesResponse {
        trades: trades.iter().cloned().collect(),
    }))
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

pub async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    {
        let engine = state.engine.read().await;
        let snapshot = engine.get_snapshot(20);
        let event = ServerEvent::Snapshot(snapshot);
        if let Ok(msg) = serde_json::to_string(&event) {
            let _ = sender.send(Message::Text(msg.into())).await;
        }
    }

    let mut rx = state.tx.subscribe();

    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if let Ok(msg)= serde_json::to_string(&event) {
                                if sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                _= tokio::time::sleep(tokio::time::Duration::from_secs(15)) => {
                    let heartbeat = ServerEvent::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    //TODO: why need heart beat?
                    // if let Ok(msg) = serde_json::to_string(&heartbeat) {
                    //     if sender.send(Message::Text(msg.into())).await.is_err() {
                    //         break;
                    //     }
                    // }
                }
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => break,
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}

#[derive(Deserialize)]
pub struct ListQuery {
    limit: usize,
}

#[axum::debug_handler]
pub async fn list_settlements(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<SettlementJob>>> {
    let limit = q.limit.clamp(1, 200);
    Ok(Json(state.settlement.list(limit).await))
}

pub async fn get_settlement(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> ApiResult<Json<SettlementJob>> {
    state
        .settlement
        .get(id)
        .await
        .map(Json)
        .ok_or(ApiError::NotFound(format!("settlement {id} not found")))
}

pub struct ConfirmSettlementRequest {
    signature: String,
    success: bool,
    error: Option<String>,
}

pub async fn confirm_settlement(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<ConfirmSettlementRequest>,
) -> ApiResult<Json<SettlementJob>> {
    if req.signature.trim().is_empty() && req.success {
        return Err(ApiError::BadRequest(
            "signature is required when success=true".into(),
        ));
    }

    let job = state
        .settlement
        .confirm(id, req.signature, req.success, req.error)
        .await
        .map_err(ApiError::NotFound)?;

    state.broadcast(ServerEvent::SettlementUpdate {
        settlement_id: id,
        status: SettlementStatus::Confirmed,
        signature: job.signature.clone(),
        trade_price: job.trade.price,
        trade_quantity: job.trade.quantity,
    });

    Ok(Json(job))
}

#[derive(Deserialize)]
pub struct InitMarketReq {
    pub base_mint: String,
    pub quote_mint: String,
    pub authority: Option<String>,
}
#[derive(Serialize)]
pub struct OnchainSubmitResult {
    pub submitted: bool,
    pub signature: Option<String>,
    pub error: Option<String>,
}

#[axum::debug_handler]
pub async fn initialize_market(
    State(state): State<AppState>,
    Json(req): Json<InitMarketReq>,
) -> ApiResult<Json<OnchainSubmitResult>> {
    //TODO: no need to pass into authority
    let authority = req.authority.unwrap();
    let (ix, _) = build_initialize_market(
        &state.settlement.config.program_id,
        &authority,
        &req.base_mint,
        &req.quote_mint,
    )
    .map_err(|e| ApiError::BadRequest(format!("build instruction of market error: {e}")))?;

    let rpc_url = &state.settlement.config.rpc_url;
    let sig = submit_instruction(rpc_url, ix)
        .map_err(|e| ApiError::BadRequest(format!("submit instruction error: {e}")))?;

    Ok(Json(OnchainSubmitResult {
        submitted: true,
        signature: Some(sig),
        error: None,
    }))
}

#[derive(Debug, Deserialize)]
pub struct InitVaultReq {
    pub owner: String,
    pub market: String,
}

pub async fn initialize_vault(
    State(state): State<AppState>,
    Json(req): Json<InitVaultReq>,
) -> ApiResult<Json<OnchainSubmitResult>> {
    let ix = build_initialize_vault(&state.settlement.config.program_id, &req.owner, &req.market)
        .map_err(|e| ApiError::BadRequest(format!("build init vault ix error: {e}")))?;
    let rpc_url = &state.settlement.config.rpc_url;
    let sig = submit_instruction(rpc_url, ix)
        .map_err(|e| ApiError::BadRequest(format!("submit instruction error: {e}")))?;

    Ok(Json(OnchainSubmitResult {
        submitted: true,
        signature: Some(sig),
        error: None,
    }))
}

#[derive(Debug, Serialize)]
pub struct GetMarketsResult {
    id: i64,
    symbol: String,
    base_symbol: String,
    quote_symbol: String,
    base_mint: String,
    quote_mint: String,
}
pub async fn get_markets(State(state): State<AppState>) -> ApiResult<Json<Vec<GetMarketsResult>>> {
    let pool = state.pool;
    let res = sqlx::query_as!(
        GetMarketsResult,
        r#"
        select * from markets;
        "#
    )
    .fetch_all(&pool)
    .await;

    match res {
        Ok(data) => Ok(Json(data)),
        Err(_) => Err(ApiError::NotFound("no markets exists".into())),
    }
}

#[derive(Deserialize)]
pub struct TokenAirdropRequest {
    destination: String,
    amount: u64,
    mint: String,
}
pub async fn request_token_airdrop(Json(req): Json<TokenAirdropRequest>) -> ApiResult<()> {
    let token_pid = &spl_token_interface::id();
    let source = load_settle_keypair().map_err(|_| ApiError::Internale("unexpected 1".into()))?;
    let source_pub = source.pubkey();
    let mint_pub =
        Pubkey::from_str(&req.mint).map_err(|_| ApiError::Internale("unexpected 3".into()))?;
    let source_ata = spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&source_pub, &mint_pub, token_pid);

    let dest_pub = Pubkey::from_str(&req.destination)
        .map_err(|_| ApiError::Internale("unexpected 4".into()))?;
    let dest_ata = spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&dest_pub, &mint_pub, token_pid);

    let decimal = 9;
    transfer_token(
        &source_ata,
        &source_pub,
        &dest_ata,
        req.amount,
        &mint_pub,
        decimal,
    )
    .map_err(|e| {
        eprintln!("transfer error {e}");
        ApiError::BadRequest("not a valid transfer".into())
    })?;
    Ok(())
}

#[axum::debug_handler]
pub async fn rpc_proxy(State(state): State<AppState>, body: Bytes) -> ApiResult<Response> {
    let ProxyConfig { client, rpc_url } = state.rpc_proxy;
    let upstream_res = client
        .post(rpc_url)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| {
            eprintln!("post to helius error {e}");
            ApiError::BadRequest("bad request".into())
        })?;

    let status = upstream_res.status();
    let body = upstream_res.bytes().await.map_err(|e| {
        eprintln!("post to helius got error {e}");
        ApiError::BadRequest("bad request".into())
    })?;

    Ok((status, [(header::CONTENT_TYPE, "application/json")], body).into_response())
}
