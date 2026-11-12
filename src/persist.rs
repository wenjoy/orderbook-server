use std::{collections::VecDeque, env};

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::{
    api::settlement::{SettlementJob, SettlementService},
    engine::{
        book::OrderBook,
        order::{Order, OrderStatus, Side, Trade},
    },
};

#[derive(Default, Debug)]
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
        r#"create table if not exists engine_meta (key text not null primary key, value bigint not null)"#,
        r#"create table if not exists open_orders (
            id bigserial primary key,
            side text not null,
            price bigint not null,
            quantity bigint not null,
            filled_quantity bigint not null,
            user_wallet text not null,
            status text not null,
            market text not null,
            timestamp bigint not null
            )"#,
        r#"create table if not exists recent_trades (
            seq bigserial primary key ,
            taker_order_id text not null,
            maker_order_id text not null,
            price bigint not null,
            quantity bigint not null,
            taker_side text not null,
            taker_user text not null,
            maker_user text not null,
            timestamp bigint null,
            market text not null
            ) "#,
        r#"create table if not exists settlements (
            id bigserial primary key ,
            payload jsonb not null
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
        eprintln!("failed to connect to pool");
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
        "select side,status,price,quantity,filled_quantity,user_wallet,timestamp,market from open_orders order by id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("fetch orders error: {}", e);
        vec![]
    });

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
            user: r.get("user_wallet"),
            timestamp: chrono::DateTime::from_timestamp_millis(r.get::<i64, _>("timestamp"))
                .unwrap_or_default(),
            status,
            market: r.get("market"),
        });
    }

    let trade_rows = sqlx::query(
        r#"
        select taker_order_id, maker_order_id, price, quantity, taker_side, taker_user, maker_user,market,timestamp 
        from recent_trades
        order by seq DESC
        limit 500
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        eprintln!("fetch trades error {}", e);
        vec![]
    });
    let mut recent_trades = Vec::with_capacity(trade_rows.len());
    for r in trade_rows {
        let side_s: String = r.get("taker_side");

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
            timestamp: chrono::DateTime::from_timestamp_millis(
                r.try_get::<i64, _>("timestamp").unwrap_or_default(),
            )
            .unwrap_or_default(),
            quantity: r.get::<i64, _>("quantity") as u64,
            price: r.get::<i64, _>("price") as u64,
        });
    }

    let settlement_rows = sqlx::query("select id, payload from settlements")
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| {
            eprintln!("fetch settlement error {}", e);
            vec![]
        });
    let mut settlements = Vec::with_capacity(settlement_rows.len());

    for r in settlement_rows {
        let payload = r.get("payload");
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

async fn save_snapshot(pool: &PgPool, snapshot: &Snapshot) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let next_order_id = snapshot.next_order_id as i64;
    let next_settlement_id = snapshot.next_settlement_id as i64;
    // let next_order_id = 1;
    // let next_settlement_id = 1;
    sqlx::query(
        "insert into engine_meta (key, value) values ($1, $2) on conflict (key) do update set value=excluded.value",
    ).bind("next_order_id").bind(next_order_id).execute(pool).await.expect("insert next_order_id error");
    sqlx::query(
        "insert into engine_meta (key, value) values ($1, $2) on conflict (key) do update set value=excluded.value",
    ).bind("next_settlement_id").bind(next_settlement_id).execute(pool).await.expect("insert next_settlement_id error");

    sqlx::query("delete from open_orders")
        .execute(&mut *tx)
        .await?;
    for o in &snapshot.open_orders {
        if !o.is_active() {
            continue;
        }

        sqlx::query(
            r#"insert into open_orders (side,price,quantity,filled_quantity,user_wallet, status,market,timestamp) values ($1 ,$2 ,$3 ,$4 ,$5 ,$6 ,$7 ,$8)"#)
            .bind(o.side.to_str())
            .bind(o.price.unwrap_or_default() as i64)
            .bind(o.quantity as i64)
            .bind(o.filled_quantity as i64)
            .bind(o.user.clone())
            .bind(o.status.to_str())
            .bind(o.market.clone())
            .bind(o.timestamp.timestamp_millis())
            .execute(&mut *tx).await?;
    }

    sqlx::query("delete from recent_trades")
        .execute(&mut *tx)
        .await?;
    for t in &snapshot.recent_trades {
        sqlx::query(
            r#"
            insert into recent_trades (
                taker_order_id, maker_order_id, price, quantity, taker_side, taker_user, maker_user, market, timestamp
                ) values (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8
                )
            "#,
        )
            .bind(t.taker_order_id.to_string())
            .bind(t.maker_order_id.to_string())
            .bind(t.price as i64)
            .bind(t.quantity as i64)
            .bind(t.side.to_str())
            .bind(t.taker_user.clone())
            .bind(t.maker_user.clone())
            .bind(t.market.clone())
            .bind(t.timestamp.timestamp_millis())
            .execute(&mut *tx).await?;
    }

    sqlx::query("delete from settlements")
        .execute(&mut *tx)
        .await?;

    for s in &snapshot.settlements {
        let payload = serde_json::to_value(s).unwrap_or_default();
        sqlx::query("insert into settlements (payload) values ($1)")
            .bind(payload)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

async fn capture(
    book: &OrderBook,
    recent: &VecDeque<Trade>,
    settlement: &SettlementService,
) -> Snapshot {
    let (next_settlement_id, settlements) = settlement.snapshot_state().await;

    Snapshot {
        next_order_id: book.next_order_id(),
        next_settlement_id,
        open_orders: book.open_orders(None),
        recent_trades: recent.iter().cloned().collect(),
        settlements,
    }
}

pub async fn persist_now(
    book: &OrderBook,
    recent: &VecDeque<Trade>,
    settlement: &SettlementService,
) {
    let Some(pool) = pool().await else {
        return;
    };

    let snap = capture(book, recent, settlement).await;

    if let Err(e) = save_snapshot(pool, &snap).await {
        eprintln!("save snapshot error: {e}");
    }
}
