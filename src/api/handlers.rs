use axum::{Json, extract::State, http::StatusCode};
use uuid::Uuid;

use crate::{
    api::state::SharedEngine,
    types::{Order, OrderStatus, OrderType, Side},
};

pub struct PlaceOrderRequst {
    pub side: Side,
    pub order_type: OrderType,
    pub price: Option<u64>,
    pub quantity: u64,
}

pub async fn place_order(
    State(engine): State<SharedEngine>,
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

    let mut engine = engine.write().await;
    match engine.place_order(order.clone()) {
        Ok((order, _)) => Ok(Json(order)),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}
