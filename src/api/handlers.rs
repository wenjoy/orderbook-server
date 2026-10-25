use axum::{
    Json,
    extract::{
        Path, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::{
        settlement::{SettlementJob, SettlementStatus},
        state::AppState,
    },
    error::{ApiError, ApiResult},
    event::ServerEvent,
    types::{Order, OrderBookSnapshot, OrderStatus, OrderType, Side, Trade},
};

#[derive(Deserialize)]
pub struct PlaceOrderRequst {
    pub side: Side,
    pub order_type: OrderType,
    pub price: Option<u64>,
    pub quantity: u64,
}

#[axum::debug_handler]
pub async fn place_order(
    State(state): State<AppState>,
    Json(req): Json<PlaceOrderRequst>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let order = Order {
        id: Uuid::new_v4(),
        side: req.side,
        order_type: req.order_type,
        price: req.price,
        quantity: req.quantity,
        filled_quantity: 0,
        status: OrderStatus::Open,
        timestamp: chrono::Utc::now(),
    };

    let mut engine = state.engine.write().await;
    match engine.place_order(order.clone()) {
        Ok((order, trades)) => {
            for trade in &trades {
                let _ = state.tx.send(ServerEvent::Trade(trade.clone()));
            }

            let _ = state.tx.send(ServerEvent::OrderUpdate(order.clone()));
            let snapshot = engine.get_snapshot(20);
            let _ = state.tx.send(ServerEvent::Snapshot(snapshot));
            Ok(Json(serde_json::json!({
                "order": order,
                "trades": trades
            })))
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}
#[derive(Deserialize)]
pub struct CancelOrdedrRequest {
    pub order_id: Uuid,
}
#[axum::debug_handler]
pub async fn cancel_order(
    State(state): State<AppState>,
    Json(req): Json<CancelOrdedrRequest>,
) -> Result<Json<Order>, StatusCode> {
    let order_id = req.order_id;
    let mut engine = state.engine.write().await;

    match engine.cancel_order(order_id) {
        Ok(order) => Ok(Json(order)),
        Err(_) => Err(StatusCode::NOT_FOUND),
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
                    if let Ok(msg) = serde_json::to_string(&heartbeat) {
                        if sender.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
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

struct ConfirmSettlementRequest {
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

pub async fn settlement_config(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let c = state.settlement.config;

    Ok(Json(serde_json::json!({
        "mode": c.mode,
        "program_id": c.program_id,
        "market_pubkey": c.market_pubkey,
        "authority_pubkey": c.authority_pubkey,
        "rpc_url": c.rpc_url,
        "base_mint": c.base_mint,
        "quote_mint": c.quote_mint,
        "statuses": [
            SettlementStatus::Pending,
            SettlementStatus::Simulated,
            SettlementStatus::Submitted,
            SettlementStatus::Confirmed,
            SettlementStatus::Failed,
        ],
    })))
}
