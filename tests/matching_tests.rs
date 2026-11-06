use chrono::Utc;
use orderbook_engine::engine::MatchingEngine;
use orderbook_engine::engine::order::*;
use uuid::Uuid;

fn make_limit_order(side: Side, price: u64, qty: u64) -> Order {
    Order {
        market: "market".into(),
        id: Uuid::new_v4(),
        side,
        user: "limit".into(),
        price: Some(price),
        quantity: qty,
        filled_quantity: 0,
        status: OrderStatus::Open,
        timestamp: Utc::now(),
    }
}

fn make_market_order(side: Side, qty: u64) -> Order {
    Order {
        market: "market".into(),
        id: Uuid::new_v4(),
        side,
        user: "market".into(),
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

#[test]
fn test_multi_level_match() {
    let mut engine = MatchingEngine::new();

    engine
        .place_order(make_limit_order(Side::Ask, 100, 10))
        .unwrap();
    engine
        .place_order(make_limit_order(Side::Ask, 101, 10))
        .unwrap();
    engine
        .place_order(make_limit_order(Side::Ask, 102, 10))
        .unwrap();

    let buy = make_limit_order(Side::Bid, 102, 22);
    let (buy_order, trades) = engine.place_order(buy).unwrap();

    assert_eq!(buy_order.price, Some(102));
    assert_eq!(buy_order.filled_quantity, 22);
    assert_eq!(buy_order.status, OrderStatus::Filled);

    assert_eq!(trades.len(), 3);
    assert_eq!(trades[0].price, 100);
    assert_eq!(trades[0].quantity, 10);
    assert_eq!(trades[1].price, 101);
    assert_eq!(trades[1].quantity, 10);
    assert_eq!(trades[2].price, 102);
    assert_eq!(trades[2].quantity, 2);

    let snap = engine.get_snapshot(1);
    assert_eq!(snap.asks[0].price, 102);
    assert_eq!(snap.asks[0].quantity, 8);
}

#[test]
fn test_cancel_order() {
    let mut engine = MatchingEngine::new();

    let order = make_limit_order(Side::Bid, 100, 10);
    let order_id = order.id;
    engine.place_order(order).unwrap();

    let cancelled = engine.cancel_order(order_id).unwrap();
    assert_eq!(cancelled.status, OrderStatus::Cancelled);

    let snap = engine.get_snapshot(2);
    assert!(snap.bids.is_empty());

    assert!(engine.cancel_order(order_id).is_err());
}

#[test]
fn test_market_order_insufficient_liquidity() {
    let mut engine = MatchingEngine::new();

    engine
        .place_order(make_limit_order(Side::Ask, 100, 5))
        .unwrap();

    let buy = make_market_order(Side::Bid, 10);
    let (buy_order, trades) = engine.place_order(buy).unwrap();

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 5);
    assert_eq!(buy_order.status, OrderStatus::PartiallyFilled);
    assert_eq!(buy_order.filled_quantity, 5);
}
#[test]
fn test_price_time_priority() {
    let mut engine = MatchingEngine::new();

    let sell1 = make_limit_order(Side::Ask, 100, 5);
    let sell_id = sell1.id;
    engine.place_order(sell1).unwrap();

    let sell2 = make_limit_order(Side::Ask, 100, 5);
    engine.place_order(sell2).unwrap();

    let buy = make_limit_order(Side::Bid, 100, 5);
    let (_, trades) = engine.place_order(buy).unwrap();

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].maker_order_id, sell_id);
}
