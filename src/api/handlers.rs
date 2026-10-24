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
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::{settlement::SettlementJob, state::AppState},
    error::{ApiError, ApiResult},
    event::ServerEvent,
    types::{Order, OrderBookSnapshot, OrderStatus, OrderType, Side},
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

struct ListQuery {
    limit: usize,
}
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
