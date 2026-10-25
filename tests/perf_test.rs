#[test]
fn test_matching_throughput() {
    use chrono::Utc;
    use orderbook_engine::engine::MatchingEngine;
    use orderbook_engine::engine::order::*;
    use std::time::Instant;
    use uuid::Uuid;

    let mut engine = MatchingEngine::new();

    // 先挂很多卖单
    for i in 0..1000 {
        let order = Order {
            id: Uuid::new_v4(),
            side: Side::Ask,
            user: "limit".into(),
            price: Some(100 + (i % 10) as u64),
            quantity: 10,
            filled_quantity: 0,
            status: OrderStatus::Open,
            timestamp: Utc::now(),
        };
        engine.place_order(order).unwrap();
    }

    let start = Instant::now();
    let iterations = 5000;

    for _ in 0..iterations {
        let buy = Order {
            id: Uuid::new_v4(),
            side: Side::Bid,
            user: "limit".into(),
            price: Some(110),
            quantity: 5,
            filled_quantity: 0,
            status: OrderStatus::Open,
            timestamp: Utc::now(),
        };
        let _ = engine.place_order(buy);
    }

    let elapsed = start.elapsed();
    let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

    println!(
        "Matched {} orders in {:.3}s → {:.0} orders/sec",
        iterations,
        elapsed.as_secs_f64(),
        ops_per_sec
    );
    // 在普通笔记本上通常能到几万 ~ 十几万 orders/sec（纯内存匹配）
}
