use axum::{
    Router,
    routing::{get, post},
};
use orderbook_engine::orderbook::MatchingEngine;

#[tokio::main]
async fn main() {
    let engine = MatchingEngine::new();
    let app = Router::new()
        .route("/", get(|| async {}))
        .with_state(engine);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
