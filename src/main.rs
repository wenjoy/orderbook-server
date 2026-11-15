use axum::{
    Router,
    routing::{delete, get, post},
};
use orderbook_engine::api::{handlers, state::AppState};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("laod env file failed");
    let state = AppState::load_persisted().await;
    let app = Router::<AppState>::new()
        .route(
            "/orders",
            get(handlers::list_orders).post(handlers::place_order),
        )
        .route("/orders/{id}", delete(handlers::cancel_order))
        .route("/orderbook", get(handlers::get_orderbook))
        .route("/trades", get(handlers::get_recent_trades))
        .route("/settlements", get(handlers::list_settlements))
        .route("/markets", get(handlers::get_markets))
        .route("/ws", get(handlers::ws_handler))
        .route(
            "/onchain/request_token_airdrop",
            post(handlers::request_token_airdrop),
        )
        //NOTE: this is used for prepare script
        .route(
            "/onchain/initialize-market",
            post(handlers::initialize_market),
        )
        // Deprecated
        // .route(
        //     "/onchain/initialize-vault",
        //     post(handlers::initialize_vault),
        // )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Starting server at {addr}");

    axum::serve(listener, app).await.unwrap();
}
