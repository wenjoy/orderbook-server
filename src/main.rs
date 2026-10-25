use axum::{
    Router,
    routing::{get, post},
};
use orderbook_engine::api::{handlers, state::AppState};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    let state = AppState::new();
    let app = Router::new()
        .route("/", get(|| async {}))
        .route("/order", post(handlers::place_order))
        .route("/order/{id}/cancel", post(handlers::cancel_order))
        .route("/orderbook", get(handlers::get_orderbook))
        .route("/trades", get(handlers::get_recent_trades))
        .route("/settlements", get(handlers::list_settlements))
        .route("/ws", get(handlers::ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Starting server at {addr}");

    axum::serve(listener, app).await.unwrap();
}
