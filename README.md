Central Limit Order Book (CLOB) Engine
======================================

A high-performance **Central Limit Order Book (CLOB)** backend built with **Rust**, **Actix-Web**, and **ScyllaDB**, featuring **real-time trade and depth broadcasting** over WebSockets.

It simulates how real exchanges (like Binance or Coinbase) match orders, persist data, and stream updates to connected clients.

* * * * *

Performance Benchmarks
----------------------

**Localhost benchmarks (100K requests, 500 concurrent connections):**

| Architecture | Throughput | Avg Latency | P99 Latency |
| --- | --- | --- | --- |
| MPSC + JSON (baseline) | 10,653 req/s | 40.05ms | 91.85ms |
| Ring Buffer + JSON | 22,356 req/s | 7.10ms | 24.30ms |
| **Ring Buffer + Binary** | **22,264 req/s** | **0.27ms** | **0.50ms** |

Sub-millisecond P99 latency achieved through lock-free ring buffers and binary protocol (MessagePack).

* * * * *

Features
--------

### Core Engine

-   Full in-memory **limit order book** using `BTreeMap` and `VecDeque` for O(log n) price-level lookups
-   Supports both **buy (bid)** and **sell (ask)** orders
-   Implements price-time priority matching
-   Lock-free ring buffer architecture for zero contention
-   Batch processing (200 orders per iteration)
-   Automatically removes filled orders and empty levels

### Real-Time WebSocket Broadcasts

-   Emits **`trade`** and **`depth_update`** messages to all connected clients
-   Follows exchange-style streaming updates for live order book visualization
-   Example WebSocket messages:

json

```
{
  "type": "trade",
  "price": 101,
  "quantity": 5,
  "maker_order_id": 1,
  "taker_order_id": 2,
  "timestamp": 1730836400000
}

{
  "type": "depth_update",
  "depth": {
    "bids": [[100, 5]],
    "asks": [[102, 3]],
    "lastUpdateId": "6"
  }
}
```

### Persistent Storage (ScyllaDB)

-   All order and trade data is stored in **ScyllaDB** for durability
-   Schema includes:
    -   `clob.orders` for open orders
    -   `clob.trades` for historical trades
-   Background worker consumes persistence events asynchronously

### REST API (Actix-Web)

| Method | Endpoint | Description |
| --- | --- | --- |
| `POST` | `/order` | Create a new order |
| `DELETE` | `/order` | Cancel an existing order |
| `GET` | `/depth` | Fetch top 10 levels of order book |
| `GET` | `/metrics` | Prometheus metrics endpoint |

Example:

bash

```
curl -X POST http://127.0.0.1:8080/order\
  -H "Content-Type: application/json"\
  -d '{"price":100,"quantity":5,"user_id":1,"side":"Buy"}'
```

### Binary Protocol Support (MessagePack)

- Dual protocol support: JSON and MessagePack
- 70% smaller payload size (24 bytes vs 67 bytes)
- 97% faster serialization compared to JSON
- Content-type negotiation (`application/json` or `application/msgpack`)

### Prometheus Metrics

Detailed observability with separate metrics for each layer:
```
http_requests_total          # Total HTTP requests received
http_request_latency_ms      # End-to-end HTTP latency
orders_matched_total         # Orders processed by matching engine
matching_engine_latency_ms   # Order matching engine latency
trades_executed_total        # Total trades executed
depth_broadcasts_total       # Number of depth updates broadcast
```

Example metrics output:
```
http_request_latency_ms P50: 5.9 microseconds
matching_engine_latency_ms P50: 22 microseconds
orders_matched_total: 99,965
trades_executed_total: 74,822
```

### Scylla Persistence Worker

-   Runs in the background, handling:
    -   `NewOrder`
    -   `OrderFilled`
    -   `TradeExecuted`
    -   `OrderDeleted`
-   Inserts and updates records in ScyllaDB through the async driver

* * * * *

Project Structure
-----------------

<img width="628" height="280" alt="Screenshot 2025-11-05 at 10 48 20 PM" src="https://github.com/user-attachments/assets/0c711164-7b3d-40f8-b2f8-8b738e574cb4" />

* * * * *

Example Flow
------------

1.  A client submits a **buy order** using `/order`
2.  The orderbook matches it against existing **sell orders**
3.  If a trade occurs:
    -   `OrderFilled` and `TradeExecuted` are persisted to ScyllaDB
    -   A live `trade` broadcast is sent via WebSocket
    -   The top-10 depth snapshot is broadcast as `depth_update`
4.  Unfilled portions are added to the orderbook

* * * * *

Running Locally
---------------

### **Start ScyllaDB (Docker)**

bash

```
docker run -d --name scylla -p 9042:9042 scylladb/scylla
```

### **Run the Backend**

bash

```
cargo run --release
```

Server starts on:
```
http://127.0.0.1:8080
```

### **Run Benchmarks**

bash

```
cargo test extreme_stress_test --release -- --nocapture
```

* * * * *

WebSocket Testing
-----------------

Use `wscat` to connect:

bash

```
npx wscat -c ws://127.0.0.1:8080/ws
```

Then send a few orders via `curl` --- you'll see live JSON depth and trade updates appear instantly in your WebSocket terminal.

* * * * *

Tech Stack
----------

-   **Language:** Rust
-   **Framework:** Actix-Web
-   **Database:** ScyllaDB
-   **Async runtime:** Tokio
-   **Concurrency:** Lock-free ring buffers, MPSC channels
-   **Serialization:** Serde JSON, MessagePack (rmp-serde)
-   **Messaging:** MPSC channel + async worker
-   **WebSocket Layer:** Actix Actors
-   **Metrics:** Prometheus

* * * * *

**Note:** Benchmarks are localhost synthetic tests. Production systems face additional complexity including network latency, geographic distribution, authentication, and regulatory requirements.
