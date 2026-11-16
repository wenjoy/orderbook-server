# orderbook-engine

A Rust order-book matching engine. Orders are matched in memory over HTTP/WebSocket, and every fill is settled on Solana through a settlement program.

## Architecture

```mermaid
flowchart LR
    subgraph Client["Clients"]
        FE["Web UI / Bot"]
    end

    subgraph Server["orderbook-engine (axum :8080)"]
        direction TB
        R["Router<br/>REST + WS"]
        H["handlers.rs<br/>business orchestration"]
        subgraph State["AppState"]
            E["MatchingEngine<br/>Arc RwLock"]
            RT["recent_trades<br/>VecDeque, max 200"]
            CB["broadcast Sender"]
            SS["SettlementService<br/>jobs map"]
        end
        P["persist.rs<br/>snapshot / restore"]
        C["chain.rs<br/>Solana instructions"]
    end

    subgraph Ext["External"]
        PG[("PostgreSQL")]
        RPC["Solana RPC"]
        PROG["Settlement program<br/>market + user_vault PDA"]
        SPL["SPL Token"]
    end

    FE --> R
    R --> H
    H --> E
    H --> RT
    H --> SS
    H -. events .-> CB
    CB -. push .-> FE
    H --> P
    P --> PG
    SS --> C
    C --> RPC --> PROG
    C -. transfer_checked .-> SPL
```

Components:

| Layer | Files | Role |
| --- | --- | --- |
| HTTP/WS | `src/main.rs`, `src/api/handlers.rs` | Routes, order placement, cancels, reads, WebSocket feed |
| State | `src/api/state.rs` | Shared engine, trade cache, event bus, settlement service, PG pool |
| Matching | `src/engine.rs`, `src/engine/book.rs` | Price-time priority (`BTreeMap` price levels + FIFO `VecDeque`) |
| Domain | `src/engine/order.rs` | `Order`, `Trade`, `Side`, `OrderStatus`, `OrderBookSnapshot` |
| Settlement | `src/api/settlement.rs`, `src/chain.rs` | Job lifecycle, 18-byte instruction packing, RPC submission |
| Persistence | `src/persist.rs` | Startup restore and full-snapshot writes on every order/cancel |

## API

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/orders` | Place order: `{side, price, quantity, user, market}` |
| `GET` | `/orders?user=` | Open orders |
| `DELETE` | `/orders/{id}` | Cancel by UUID |
| `GET` | `/orderbook` | 10-level snapshot |
| `GET` | `/trades` | Recent trades |
| `GET` | `/settlements?limit=` | Settlement jobs |
| `GET` | `/markets` | Rows from `markets` |
| `GET` | `/ws` | Snapshot on connect, then live events |
| `POST` | `/onchain/request_token_airdrop` | Devnet token airdrop |
| `POST` | `/onchain/initialize-market` | Submit market init instruction |

## Quick start

```bash
cp env.example .env        # set DATABASE_URL, SOLANA_RPC_URL, SETTLEMENT_MODE, SETTLEMENT_PROGRAM_ID
docker compose up -d       # PostgreSQL 16 on host port 6432
psql "postgres://orderbook:123@127.0.0.1:6432/orderbook_markets" -f init.sql
cargo run                  # http://127.0.0.1:8080
cargo test                 # matching + throughput tests
```
