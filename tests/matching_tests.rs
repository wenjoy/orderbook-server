use chrono::Utc;
use orderbook_engine::orderbook::{MatchingEngine, engine};
use orderbook_engine::types::*;
use uuid::Uuid;

fn make_limit_order(side: Side, price: u64, qty: u64) -> Order {
    Order {
        id: Uuid::new_v4(),
        side,
        order_type: OrderType::Limit,
        price: Some(price),
        quantity: qty,
        filled_quantity: 0,
        status: OrderStatus::Open,
        timestamp: Utc::now(),
    }
}

fn make_market_order(side: Side, qty: u64) -> Order {
    Order {
        id: Uuid::new_v4(),
        side,
        order_type: OrderType::Market,
        price: None,
        quantity: qty,
        filled_quantity: 0,
        status: OrderStatus::Open,
        timestamp: Utc::now(),
    }
}

#[test]
fn test_simple_full_fill() {
    let mut engine = MatchingEngine::new();

    let sell = make_limit_order(Side::Ask, 100, 10);
    let (sell_order, trades) = engine.place_order(sell).unwrap();
    assert!(trades.is_empty());
    assert_eq!(sell_order.status, OrderStatus::Open);

    let buy = make_limit_order(Side::Bid, 100, 10);
    let (buy_order, trades) = engine.place_order(buy).unwrap();

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].price, 100);
    assert_eq!(trades[0].quantity, 10);
    assert_eq!(buy_order.status, OrderStatus::Filled);
    assert_eq!(buy_order.filled_quantity, 10);

    let snap = engine.get_snapshot(10);
    assert!(snap.bids.is_empty());
    assert!(snap.bids.is_empty());
}

#[test]
fn test_partial_fill() {
    let mut engine = MatchingEngine::new();

    let sell = make_limit_order(Side::Ask, 100, 10);
    engine.place_order(sell).unwrap();
    let buy = make_limit_order(Side::Bid, 100, 6);
    let (buy_order, trades) = engine.place_order(buy).unwrap();

    assert_eq!(trades[0].price, 100);
    assert_eq!(trades[0].quantity, 6);
    assert_eq!(buy_order.filled_quantity, 6);
    assert_eq!(buy_order.status, OrderStatus::Filled);

    let snap = engine.get_snapshot(2);
    assert_eq!(snap.asks.len(), 1);
    assert_eq!(snap.asks[0].quantity, 4);
    assert_eq!(snap.bids.len(), 0);
}
