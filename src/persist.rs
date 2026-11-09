use std::env;

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::{
    api::settlement::SettlementJob,
    engine::order::{Order, OrderStatus, Side, Trade},
};

#[derive(Default)]
pub struct Snapshot {
    pub next_order_id: u64,
    pub next_settlement_id: u64,
    pub open_orders: Vec<Order>,
    pub recent_trades: Vec<Trade>,
    pub settlements: Vec<SettlementJob>,
}

static POOL: OnceCell<Option<PgPool>> = OnceCell::const_new();
pub async fn pool() -> Option<&'static PgPool> {
    POOL.get_or_init(|| async {
        let _ = dotenv::dotenv();
        let url = match env::var("DATABASE_URL") {
            Ok(u) if !u.is_empty() => u,
            _ => {
                eprintln!("Warning: database is not setup, perist not work");
                return None;
            }
        };

        match PgPoolOptions::new().max_connections(5).connect(&url).await {
            Err(e) => {
                eprintln!("connect to postgres failed {e}");
                None
            }
            Ok(p) => {
                if let Err(e) = migrate(&p).await {
                    eprintln!("postgres migrate failed {e}");
                    return None;
                }
                Some(p)
            }
        }
    })
    .await
    .as_ref()
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    for stmt in [
        r#"create table if not exists engine_meta (key text not null primary key, value bigint not null) "#,
        r#"create table if not exists open_orders (
            id bigint primary key ,
            side text not null,
            price bigint not null,
            quantity bigint not null,
            filled_quantity bigint not null,
            user text not null,
            status text not null,
            market text not null,
            timestamp bigint not null,
            ) "#,
        r#"create table if not exists recent_trades (
            seq bigserial primary key ,
            taker_order_id text not null,
            maker_order_id text not null,
            price bigint not null,
            quantity bigint not null,
            taker_side text not null,
            taker_user text not null,
            maker_user text not null,
            market text not null,
            side text not null,
            ) "#,
        r#"create table if not exists settlements (
            id bigint primary key ,
            payload jsonb not null,
            ) "#,
    ] {
        sqlx::query(stmt).execute(pool).await?;
    }
    Ok(())
}

fn parse_side(s: &str) -> Option<Side> {
    match s {
        "Bid" | "bid" => Some(Side::Bid),
        "Ask" | "ask" => Some(Side::Ask),
        _ => None,
    }
}
fn parse_status(s: &str) -> Option<OrderStatus> {
    match s {
        "Open" => Some(OrderStatus::Open),
        "PartiallyFilled" => Some(OrderStatus::PartiallyFilled),
        "Filled" => Some(OrderStatus::Filled),
        "Cancelled" => Some(OrderStatus::Cancelled),
        "Rejected" => Some(OrderStatus::Rejected),
        _ => None,
    }
}
pub async fn load() -> Snapshot {
    let Some(pool) = pool().await else {
        return Snapshot {
            next_order_id: 1,
            next_settlement_id: 1,
            ..Default::default()
        };
    };

    let row = sqlx::query("select value from engine_meta")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let next_order_id = row[0].get::<i64, _>("value") as u64;
    let next_settlement_id = row[1].get::<i64, _>("value") as u64;

    let order_rows = sqlx::query(
        "select id,side,price,quantity,remaining,user_id,ts_ns from open_orders order by id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut open_orders = Vec::with_capacity(order_rows.len());
    for r in order_rows {
        let side_s: String = r.get("side");

        let Some(side) = parse_side(&side_s) else {
            continue;
        };

        let Some(status) = parse_status(r.get("status")) else {
            continue;
        };

        open_orders.push(Order {
            id: Uuid::new_v4(),
            side,
            price: Some(r.get::<i64, _>("price") as u64),
            quantity: r.get::<i64, _>("quantity") as u64,
            filled_quantity: r.get::<i64, _>("filled_quantity") as u64,
            user: r.get("user"),
            timestamp: chrono::DateTime::from_timestamp_millis(r.get::<i64, _>("timestamp"))
                .unwrap_or_default(),
            status,
            market: r.get("market"),
        });
    }

    let trade_rows = sqlx::query(
        r#"
        select taker_order_id, maker_order_id, price, quantity, taker_side, taker_user, maker_user
        from recent_trades
        order by sed DESC
        limit 500
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut recent_trades = Vec::with_capacity(trade_rows.len());
    for r in trade_rows {
        let side_s: String = r.get("side");

        let Some(side) = parse_side(&side_s) else {
            continue;
        };

        recent_trades.push(Trade {
            id: Uuid::new_v4(),
            taker_user: r.get("taker_user"),
            maker_user: r.get("maker_user"),
            market: r.get("market"),
            maker_order_id: Uuid::parse_str(r.get("maker_order_id")).unwrap_or_default(),
            taker_order_id: Uuid::parse_str(r.get("taker_order_id")).unwrap_or_default(),
            side,
            timestamp: chrono::DateTime::from_timestamp_millis(r.get::<i64, _>("timestamp"))
                .unwrap_or_default(),
            quantity: r.get::<i64, _>("quantity") as u64,
            price: r.get::<i64, _>("price") as u64,
        });
    }

    let settlement_rows = sqlx::query("select id, paylaod from settlements")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    let mut settlements = Vec::with_capacity(settlement_rows.len());

    for r in settlement_rows {
        let payload = r.get("paylaod");
        match serde_json::from_value::<SettlementJob>(payload) {
            Ok(j) => {
                settlements.push(j);
            }
            Err(e) => {
                eprintln!("Skip bad settlement payload {e}");
            }
        }
    }

    Snapshot {
        next_order_id,
        next_settlement_id,
        open_orders,
        recent_trades,
        settlements,
    }
}
