use axum::{
    Router,
    routing::{get, post},
};
use orderbook_engine::api::{handlers, state::AppState};

#[tokio::main]
async fn main() {
    let state = AppState::new();
    let app = Router::new()
        .route("/", get(|| async {}))
        .route("/order", post(handlers::place_order))
        .with_state(state);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
