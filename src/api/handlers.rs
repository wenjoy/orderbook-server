use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::state::{AppState, SharedEngine},
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
) -> Result<Json<Order>, StatusCode> {
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
        Ok((order, _)) => Ok(Json(order)),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}
pub struct CancelOrdedrRequest {
    pub order_id: Uuid,
}
pub async fn cancel_order(
    State(engine): State<SharedEngine>,
    Json(req): Json<CancelOrdedrRequest>,
) -> Result<Json<Order>, StatusCode> {
    let order_id = req.order_id;
    let mut engine = engine.write().await;

    match engine.cancel_order(order_id) {
        Ok(order) => Ok(Json(order)),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_orderbook(
    State(engine): State<SharedEngine>,
) -> Result<Json<OrderBookSnapshot>, StatusCode> {
    let engine = engine.write().await;

    let snap = engine.get_snapshot(10);

    Ok(Json(snap))
}

pub async fn ws_handler() {
    todo!()
}
