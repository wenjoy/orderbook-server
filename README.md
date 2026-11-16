# orderbook-engine

基于 Rust 的链下订单簿撮合引擎，提供 HTTP / WebSocket 接口完成撮合，并把成交结果提交到 Solana 链上做结算（PDA 金库 + 结算程序）。

- **撮合**：纯内存、价格优先 + 时间优先（FIFO），限价单与市价单
- **接入**：axum 提供 REST 接口与 WebSocket 实时推送
- **结算**：每笔成交生成结算任务，支持 dry-run / 排队 / 热钱包三种模式，通过 Solana RPC 调用结算程序
- **持久化**：PostgreSQL 快照式持久化，启动时自动恢复未成交订单、成交记录与结算任务

---

## 1. 架构

### 1.1 总览

```mermaid
flowchart LR
    subgraph Client["客户端 / 前端"]
        FE["Web UI"]
        BOT["做市 / 测试脚本"]
    end

    subgraph Server["orderbook-engine (axum, :8080)"]
        direction TB
        R["路由层<br/>REST + WS"]
        H["handlers.rs<br/>业务编排"]
        subgraph State["AppState（共享状态）"]
            E["MatchingEngine<br/>Arc&lt;RwLock&gt;"]
            RT["recent_trades<br/>VecDeque (max 200)"]
            CB["broadcast::Sender&lt;ServerEvent&gt;"]
            SS["SettlementService<br/>jobs: HashMap"]
        end
        P["persist.rs<br/>快照 / 恢复"]
        C["chain.rs<br/>Solana 指令构造与提交"]
    end

    subgraph External["外部依赖"]
        PG[("PostgreSQL<br/>engine_meta / open_orders<br/>recent_trades / settlements<br/>markets")]
        RPC["Solana RPC"]
        PROG["结算程序 (on-chain)<br/>market / user_vault PDA"]
        SPL["SPL Token"]
    end

    FE --> R
    BOT --> R
    R --> H
    H --> E
    H --> RT
    H --> SS
    H -. 广播事件 .-> CB
    CB -. 推送 .-> FE
    H --> P
    P --> PG
    SS --> C
    C --> RPC
    RPC --> PROG
    C -. transfer_checked .-> SPL
```

### 1.2 组件职责

| 组件 | 文件 | 职责 |
| --- | --- | --- |
| 路由层 | `src/main.rs` | 注册路由、CORS、启动 HTTP 服务（`0.0.0.0:8080`） |
| 业务编排 | `src/api/handlers.rs` | 下单、撤单、查订单簿、查成交、WebSocket、链上指令提交、空投 |
| 共享状态 | `src/api/state.rs` | `AppState`：引擎锁、最近成交环形队列、事件广播、结算服务、PG 连接池；启动时 `load_persisted()` |
| 撮合引擎 | `src/engine.rs` | 撮合主循环 `place_order` / `cancel_order` / `restore_order` / `get_snapshot` |
| 订单簿 | `src/engine/book.rs` | 价格档位（`BTreeMap<price, VecDeque<Order>>`）、`order_index`、深度快照 |
| 领域模型 | `src/engine/order.rs` | `Order`、`Trade`、`Side`、`OrderStatus`、`DepthLevel`、`OrderBookSnapshot` |
| 事件 | `src/event.rs` | `ServerEvent`：WebSocket 推送的报文枚举 |
| 结算服务 | `src/api/settlement.rs` | 结算任务生命周期、指令数据打包、三种结算模式 |
| 链交互 | `src/chain.rs` | PDA 推导、指令构造、交易签名与提交、SPL 转账 |
| 持久化 | `src/persist.rs` | 建表迁移、加载快照、全量保存快照 |

### 1.3 下单撮合时序

```mermaid
sequenceDiagram
    autonumber
    participant C as 客户端
    participant A as axum handler
    participant E as MatchingEngine
    participant S as AppState
    participant D as PostgreSQL
    participant W as SettlementService
    participant X as WebSocket 订阅者

    C->>A: POST /orders {side, price, quantity, user, market}
    A->>E: write().place_order(order)
    E->>E: 与对手方最优价档撮合（FIFO）
    E-->>A: (order, trades)
    A->>S: record_trades(trades)
    S->>S: 追加 recent_trades（上限 200）
    S->>W: enqueue_trades -> SettlementJob
    W-->>S: job(status, signature)
    S-->>X: ServerEvent::Trade / SettlementUpdate
    A->>S: broadcast_orderbook()
    S-->>X: ServerEvent::OrderbookUpdate（20 档）
    A->>S: persist()
    S->>D: 全量快照（open_orders / recent_trades / settlements）
    A-->>C: {order_id, remaining, trades[]}
```

### 1.4 目录结构

```
orderbook-engine/
├── src/
│   ├── main.rs            # 入口：路由 / CORS / 监听 8080
│   ├── lib.rs             # 模块导出
│   ├── api.rs             # api 模块声明
│   ├── api/
│   │   ├── handlers.rs    # HTTP + WS 处理器
│   │   ├── state.rs       # AppState 与共享状态
│   │   └── settlement.rs  # 结算服务与指令打包
│   ├── engine.rs          # 撮合引擎
│   ├── engine/
│   │   ├── book.rs        # 订单簿 / 价格档位 / 索引
│   │   └── order.rs       # 订单、成交、快照类型
│   ├── chain.rs           # Solana 链上交互
│   ├── persist.rs         # PostgreSQL 持久化
│   ├── event.rs           # WebSocket 事件
│   └── error.rs           # ApiError / ApiResult
├── tests/
│   ├── matching_tests.rs  # 撮合行为测试
│   └── perf_test.rs       # 吞吐性能测试
├── docker-compose.yaml    # PostgreSQL 16（宿主端口 6432）
├── init.sql               # markets 表初始化
├── env.example            # 环境变量模板
└── Cargo.toml
```

---

## 2. 撮合模型

### 2.1 订单簿结构

```mermaid
flowchart TB
    subgraph OB["OrderBook"]
        BIDS["bids: BTreeMap&lt;price, PriceLevel&gt;<br/>best_bid = 最大价格"]
        ASKS["asks: BTreeMap&lt;price, PriceLevel&gt;<br/>best_ask = 最小价格"]
        IDX["order_index: HashMap&lt;Uuid, (Side, price)&gt;"]
        SEQ["sequence: u64"]
    end
    BIDS --> L1["PriceLevel.orders: VecDeque&lt;Order&gt;<br/>FIFO → 时间优先"]
    ASKS --> L2["PriceLevel.orders: VecDeque&lt;Order&gt;<br/>FIFO → 时间优先"]
    IDX -. 撤单 O(1) 定位 .-> BIDS
    IDX -. 撤单 O(1) 定位 .-> ASKS
```

- 买盘：`BTreeMap` 升序遍历后取 `next_back()` → 价格最高者为最优
- 卖盘：`BTreeMap` 升序遍历取 `next()` → 价格最低者为最优
- 同价档位内部 `VecDeque`，新单 `push_back`，撮合从队首（最早）开始 → **价格优先 + 时间优先**
- 成交价使用 **maker 的挂单价**（买盘吃卖单取 `best_bid`，卖单吃买单取 `best_ask`）
- `price = None` 表示市价单；限价单必须带价格

### 2.2 订单状态机

```mermaid
stateDiagram-v2
    [*] --> Open: 下单（未成交部分入簿）
    Open --> PartiallyFilled: 部分成交且有剩余
    PartiallyFilled --> Filled: 剩余全部成交
    Open --> Filled: 一次全部成交
    Open --> Cancelled: 调用撤单接口
    PartiallyFilled --> Cancelled: 调用撤单接口
    Open --> Rejected: 非法订单（quantity = 0）
    Filled --> [*]
    Cancelled --> [*]
    Rejected --> [*]
```

> 只有 `Open` / `PartiallyFilled` 会进入订单簿（`Order::is_active`），也只有这两个状态会被持久化。

### 2.3 结算状态机

```mermaid
stateDiagram-v2
    [*] --> Pending: 成交产生 SettlementJob
    Pending --> Simulated: mode = dry_run（默认）
    Pending --> Submitted: mode = queue（仅记录队列）
    Pending --> Confirmed: mode = hot_wallet 提交成功
    Pending --> Failed: 提交失败 / confirm 报告失败
    Confirmed --> [*]
    Failed --> [*]
    Simulated --> [*]
```

> `confirm` 接口可以把任务改为 `Confirmed` / `Failed`，但当前路由未注册（见 [已知限制](#8-已知限制与-todo)）。

---

## 3. HTTP API

服务默认监听 `0.0.0.0:8080`，所有响应为 JSON，CORS 全开（`CorsLayer::permissive`）。

| 方法 | 路径 | 说明 | 参数 |
| --- | --- | --- | --- |
| `POST` | `/orders` | 下单并撮合 | body：`PlaceOrderRequst` |
| `GET` | `/orders` | 查询在簿订单 | query：`user`（可选，按用户过滤） |
| `DELETE` | `/orders/{id}` | 撤单（UUID） | path：订单 id |
| `GET` | `/orderbook` | 订单簿快照（10 档） | — |
| `GET` | `/trades` | 最近成交（内存，最多 200 条） | — |
| `GET` | `/settlements` | 结算任务列表 | query：`limit`（1–200） |
| `GET` | `/markets` | 市场列表（来自 `markets` 表） | — |
| `GET` | `/ws` | WebSocket 实时推送 | — |
| `POST` | `/onchain/request_token_airdrop` | 热钱包向目标 ATA 空投代币 | body：`{destination, amount, mint}` |
| `POST` | `/onchain/initialize-market` | 构造并提交初始化市场指令 | body：`{base_mint, quote_mint, authority}` |

### 3.1 下单

```http
POST /orders
Content-Type: application/json

{
  "side": "Bid",          // Bid | Ask
  "price": 100,           // 市价单传 null / 省略
  "quantity": 10,
  "user": "<wallet pubkey 或任意用户标识>",
  "market": "<market pubkey 或市场标识>"
}
```

响应：

```json
{
  "order_id": "0f1c...-uuid",
  "remaining": 0,
  "trades": [
    {
      "id": "3f2a...-uuid",
      "maker_order_id": "...",
      "taker_order_id": "...",
      "price": 100,
      "quantity": 10,
      "timestamp": "2026-11-16T05:00:00Z",
      "side": "Bid",
      "taker_user": "...",
      "maker_user": "...",
      "market": "..."
    }
  ]
}
```

> 撮合失败（如 `quantity = 0`）返回 `404 NOT FOUND`（当前实现，见已知限制）。

### 3.2 订单簿 / 成交

```http
GET /orderbook
```

```json
{
  "bids": [{ "price": 100, "quantity": 25 }],
  "asks": [{ "price": 101, "quantity": 10 }],
  "sequence": 16
}
```

```http
GET /trades
```

```json
{ "trades": [ /* Trade[]，按时间先后 */ ] }
```

> 注意：`price` / `quantity` 均为 `u64` 整数刻度，不含小数位，前端需自行乘以精度换算。

---

## 4. WebSocket 推送

连接 `/ws` 后服务端**立即下发一次 20 档快照**（`type = "Snapshot"`），随后广播撮合与结算事件。客户端任何文本帧（或 Close）都会断开连接；服务端每 15 秒的空闲心跳目前被注释掉，不会实际发送。

事件采用 `{ "type": <标签>, "data": <负载> }` 的 serde 内部标签格式：

| type | data | 触发时机 |
| --- | --- | --- |
| `Snapshot` | `OrderBookSnapshot` | WebSocket 连接建立时 |
| `Trade` | `Trade` | 每笔成交 |
| `OrderbookUpdate` | `{ bids, asks }`（各 20 档） | 下单 / 撤单后 |
| `OrderCancelled` | `{ order_id }` | 撤单成功 |
| `SettlementUpdate` | `{ settlement_id, status, signature, trade_price, trade_quantity }` | 结算任务入队或状态变更 |
| `Heartbeat` | `{ timestamp }` | 已定义但当前未发送 |
| `DepthUpdate` | `{ side, price, quantity, sequence }` | 已定义但当前未发送 |

示例：

```json
{ "type": "Trade", "data": { "id": "...", "price": 100, "quantity": 5, "side": "Ask", "...": "..." } }
```

---

## 5. 链上结算

### 5.1 结算模式

由 `SETTLEMENT_MODE` 控制（`SettlementMode::from_env`）：

| 值 | 行为 |
| --- | --- |
| `dry_run`（默认，任何未知值） | 直接标记 `Simulated`，签名形如 `dry_run_sig_{id}`，不发起链上交易 |
| `queue` | 保持 `Pending`，只打印日志等待外部确认 |
| `hot_wallet` | 用 `SETTLEMENT_KEYPAIR` 签名，同步调用 RPC 提交，成功即 `Confirmed` |

### 5.2 指令数据格式

撮合产生的每笔成交被打包为结算指令数据（`pack_settle_trade`，共 18 字节）：

| 偏移 | 长度 | 含义 |
| --- | --- | --- |
| 0 | 1 | 指令判别符 `4`（settle_trade） |
| 1 | 8 | `price`（u64，小端） |
| 9 | 8 | `quantity`（u64，小端） |
| 17 | 1 | `taker_is_buy`（taker 为 Bid 时 1） |

账户列表（`OrderBook` 之外的链上侧）：

| # | 账户 | 权限 |
| --- | --- | --- |
| 0 | authority（热钱包公钥） | signer, readonly |
| 1 | market PDA | writable |
| 2 | maker_user_vault PDA | writable |
| 3 | taker_user_vault PDA | writable |

### 5.3 PDA 推导

```text
market     = PDA(["market",     base_mint, quote_mint], program_id)
user_vault = PDA(["user_vault", market,    owner],       program_id)
```

初始化市场 / 初始化金库的指令判别符分别为 `0` / `1`；初始化金库接口已标记为 Deprecated（路由已注释）。

### 5.4 空投（demo 用）

`POST /onchain/request_token_airdrop` 由热钱包作为 authority，通过 SPL `transfer_checked`（精度固定为 9）向目标钱包的 ATA 转账。若目标 ATA 未创建，链上转账会失败——当前不会自动创建 ATA。

---

## 6. 持久化与数据表

启动时 `AppState::load_persisted()`：

1. 读取 `DATABASE_URL`，连接 PostgreSQL
2. `persist::load()` 读取快照
3. `engine.restore_order()` 把 `Open` / `PartiallyFilled` 订单重新挂回订单簿
4. 恢复最近成交（最多 200 条）与结算任务

每次下单 / 撤单成功后调用 `persist()`，执行**全量覆盖**写入（`delete` + 逐条 `insert`，单事务）。未配置数据库时持久化静默跳过，引擎仍可运行（纯内存模式）。

自动建表（`persist::migrate`）：

| 表 | 主要字段 | 说明 |
| --- | --- | --- |
| `engine_meta` | `key`, `value` | `next_order_id` / `next_settlement_id` 等元信息 |
| `open_orders` | `side, price, quantity, filled_quantity, user_wallet, status, market, timestamp` | 在簿订单快照（`user` 是 PG 保留字，故列名 `user_wallet`） |
| `recent_trades` | `seq, taker_order_id, maker_order_id, price, quantity, taker_side, taker_user, maker_user, timestamp, market` | 成交记录 |
| `settlements` | `id`, `payload` (jsonb) | `SettlementJob` 的 JSON 序列化 |

手工初始化（`init.sql`，需单独执行）：

```sql
create table markets (
  id BIGSERIAL PRIMARY KEY,
  symbol text not null unique,
  base_symbol text not null,
  quote_symbol text not null,
  base_mint text not null,
  quote_mint text not null
)
```

---

## 7. 运行

### 7.1 环境变量

| 变量 | 必填 | 说明 |
| --- | --- | --- |
| `DATABASE_URL` | 是 | PostgreSQL 连接串；缺失时持久化功能自动关闭 |
| `SOLANA_RPC_URL` | 是 | Solana RPC 地址（devnet / localnet） |
| `SETTLEMENT_MODE` | 否 | `dry_run`（默认）/ `queue` / `hot_wallet` |
| `SETTLEMENT_PROGRAM_ID` | 是 | 结算程序 program id |
| `SETTLEMENT_KEYPAIR` | 条件 | 密钥对 JSON **文件路径**；`hot_wallet` 模式必需 |
| `SETTLEMENT_KEYPAIR_JSON` | 条件 | 密钥对 JSON **内容**，与上者二选一 |

```bash
cp env.example .env
# 编辑 .env 填入上述配置
```

> `.env` 已在 `.gitignore` 中，请勿提交真实密钥。

### 7.2 启动

```bash
# 1. 启动 PostgreSQL（宿主机端口 6432）
docker compose up -d

# 2. 初始化 markets 表
psql "postgres://orderbook:123@127.0.0.1:6432/orderbook_markets" -f init.sql

# 3. 启动引擎（需先配置 .env）
cargo run

# 服务地址：http://127.0.0.1:8080
```

若使用离线编译（`sqlx::query_as!` 需要数据库元信息），可先执行 `cargo sqlx prepare` 生成 `.sqlx` 目录并以 `SQLX_OFFLINE=true` 构建。

### 7.3 快速验证

```bash
# 挂卖单
curl -s -X POST localhost:8080/orders -H 'content-type: application/json' \
  -d '{"side":"Ask","price":100,"quantity":10,"user":"alice","market":"m1"}'

# 吃单
curl -s -X POST localhost:8080/orders -H 'content-type: application/json' \
  -d '{"side":"Bid","price":100,"quantity":6,"user":"bob","market":"m1"}'

curl -s localhost:8080/orderbook
curl -s localhost:8080/trades
curl -s 'localhost:8080/settlements?limit=10'
```

---

## 8. 测试

```bash
cargo test
```

| 文件 | 覆盖内容 |
| --- | --- |
| `tests/matching_tests.rs` | 完全成交、部分成交、跨档撮合、撤单、价格-时间优先、市价单流动性不足 |
| `tests/perf_test.rs` | 1000 档挂单后 5000 笔吃单的吞吐打印（纯内存匹配） |

当前状态：`cargo test` 中 6 个撮合用例 **5 个通过，1 个失败**（`test_market_order_insufficient_liquidity`：市价单未成交部分被尝试挂回订单簿，触发 `book.rs:65` 的 `expect("limit order must have price")` panic）。

---

## 9. 已知限制与 TODO

**撮合**
- 单一全局订单簿：`Order.market` 只作为字段透传，**不支持多市场隔离**，不同 `market` 的订单会互相撮合
- 市价单未成交部分会被尝试入簿并 panic（见测试）；市价单缺少剩余量处理策略
- 自成交（maker 与 taker 为同一用户）检查被注释，暂未拦截
- 成交后 maker 全量成交时，`order_index` 未清理，后续撤单可能在 `book.rs:90` 的 `unwrap()` panic
- `match_bid` 未更新 maker 的 `OrderStatus`；订单 id 使用 `Uuid::new_v4()`，无全局递增序号
- `bid_depth` / `ask_depth` 累加的是 `quantity` 而非 `remaining`，与 `snapshot()` 的口径不一致，部分成交订单会造成深度虚高
- `add_order_to_book` 对无价格订单直接 `expect` 崩溃

**接口**
- `place_order` 的引擎错误统一映射为 `404`，错误语义不准确
- `get_settlement` / `confirm_settlement` 已实现但未注册路由
- `get_orderbook` 使用写锁且深度固定 10
- WebSocket 心跳逻辑被注释；客户端发送任意文本即断连

**结算 / 链**
- `hot_wallet` 模式在 async handler 内同步阻塞调用 RPC
- `DryRun` 仍存在于正式流程中
- `next_order_id` / `next_settlement_id` 恢复逻辑未完成（`next_order_id` 固定返回 `666`，`next_settlement_id` 固定返回 `1`）
- 空投不自动创建目标 ATA；精度硬编码 9

**持久化**
- 每次下单 / 撤单都做全量覆盖写入，规模增大后写放大明显
- 快照写入与撮合不在同一事务边界内，进程崩溃存在状态不一致窗口
- `markets` 表不参与自动迁移，且没有创建市场的接口

---

## 10. 技术栈

Rust 2024 edition · axum 0.8 · tokio · sqlx 0.9 (PostgreSQL) · solana-sdk / solana-client · spl-token-interface · serde · uuid · chrono · tracing-subscriber
