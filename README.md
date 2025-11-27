Central Limit Order Book (CLOB) Engine
======================================

A high-performance **Central Limit Order Book (CLOB)** backend built with **Rust**, **Actix-Web**, and **ScyllaDB**, featuring **real-time trade and depth broadcasting** over WebSockets.

It simulates how real exchanges (like Binance or Coinbase) match orders, persist data, and stream updates to connected clients.

* * * * *

Performance Benchmarks
----------------------

### **Sustained Load Test (600s, 2500 concurrent connections)**

| Metric | Value |
| --- | --- |
| **Total Requests** | 32,296,069 |
| **Throughput** | **53,822 req/s** |
| **Success Rate** | **99.986%** (4,413 failures) |
| **Client P50 Latency** | **0.11 ms** |
| **Client P99 Latency** | **2.35 ms** |
| **HTTP Latency (avg)** | **4.68 μs** |
| **Matching Engine (avg)** | **3.27 μs** |
| **Order Processing (avg)** | **3.12 μs** |
| **Trades Executed** | 24,082,877 (74.58% match rate) |
| **CPU Usage** | 96.8% avg (client), 100.0% peak |

**Performance Distribution:**
- 81.1% of requests < 5 μs
- 96.8% of requests < 10 μs  
- 99.3% of requests < 25 μs

**Sub-3.3 microsecond matching engine latency** achieved through optimized trade buffer flushing (32-trade threshold), lock-free ring buffers, and binary protocol (MessagePack + Wincode). Compiled with `--release` flag for production optimizations.

### **Standard Load Test (500K requests)**

| Metric | Value |
| --- | --- |
| **Total Requests** | 500,001 |
| **HTTP Latency (avg)** | **3.61 μs** |
| **Matching Engine (avg)** | **2.36 μs** |
| **Order Processing (avg)** | **2.21 μs** |
| **Orders Matched** | 500,000 |
| **Trades Executed** | 373,667 (74.73% match rate) |

### **Architecture Comparison (100K requests, 500 concurrent connections)**

| Architecture | Throughput | Avg Latency | P99 Latency |
| --- | --- | --- | --- |
| MPSC + JSON (baseline) | 10,653 req/s | 40.05ms | 91.85ms |
| Ring Buffer + JSON | 22,356 req/s | 7.10ms | 24.30ms |
| **Ring Buffer + Binary** | **22,264 req/s** | **0.27ms** | **0.50ms** |

**Sub-microsecond matching engine latency (2.36 μs avg)** achieved through lock-free ring buffers and binary protocol (MessagePack). Sustained load testing demonstrates production-grade performance with **16.5K req/s** throughput and **99.999% reliability** under extreme concurrent load.

* * * * *

Features
--------

### Core Engine

-   Full in-memory **limit order book** with custom `PriceLevel` implementation using parallel vectors for cache-friendly access
-   Supports both **buy (bid)** and **sell (ask)** orders with BTreeMap for O(log n) price lookups
-   Implements price-time priority matching with tombstone-based deletion for zero-copy removals
-   Pre-allocated trade buffer (64 entries) with MaybeUninit for zero-overhead broadcasting
-   Depth cache with dirty-flag optimization - rebuilds only when orderbook changes
-   Lazy cleanup architecture - removes empty price levels only after matching completes
-   Wincode binary serialization for minimal trade broadcast overhead

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

### Binary Protocol Support (MessagePack & Wincode)

-   Dual protocol support for requests: JSON and MessagePack
-   Wincode binary serialization for WebSocket trade broadcasts (zero-copy, schema-based)
-   70% smaller payload size (24 bytes vs 67 bytes) compared to JSON
-   97% faster serialization with Wincode's compile-time schema generation
-   Content-type negotiation (`application/json` or `application/msgpack`)
-   Trade buffer pre-allocation with MaybeUninit for minimal allocation overhead

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

```
orderbooks/
├── src/
│   ├── engine/              # Matching engine core
│   │   ├── engine_registry.rs
│   │   └── mod.rs
│   ├── kafka_worker/        # Kafka integration
│   │   ├── consumer.rs
│   │   ├── mod.rs
│   │   └── producer.rs
│   ├── matching_loop/       # Order processing loop
│   │   └── mod.rs
│   ├── middleware/          # HTTP middleware
│   │   ├── latency.rs
│   │   └── mod.rs
│   ├── persist/             # ScyllaDB persistence
│   │   ├── client.rs
│   │   ├── event.rs
│   │   ├── mod.rs
│   │   └── worker.rs
│   ├── worker/              # WebSocket broadcaster
│   │   ├── mod.rs
│   │   └── ws.rs
│   ├── error.rs             # Error types
│   ├── events.rs            # Event definitions
│   ├── inputs.rs            # Request types
│   ├── lib.rs               # Library root
│   ├── main.rs              # Entry point
│   ├── metrics.rs           # Prometheus metrics
│   ├── msgpack.rs           # MessagePack support
│   ├── orderbook.rs         # Core orderbook logic
│   ├── outputs.rs           # Response types
│   └── routes.rs            # HTTP endpoints
├── tests/                   # Integration tests
└── target/                  # Build artifacts

```

**Key Components:**

-   `orderbook.rs` - Core matching engine with custom PriceLevel data structure
-   `matching_loop/` - Asynchronous order processing with configurable batch sizes
-   `engine/` - Multi-symbol engine registry for trading pair management
-   `persist/` - ScyllaDB integration with async worker pattern
-   `worker/` - WebSocket broadcasting with Wincode binary serialization
-   `middleware/` - Prometheus metrics collection at HTTP layer
-   `kafka_worker/` - Optional Kafka integration for event streaming

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
-   **Data Structures:** Custom cache-friendly PriceLevel with parallel vectors, BTreeMap for price indexing
-   **Serialization:** Serde JSON, MessagePack (rmp-serde), Wincode (schema-based binary)
-   **Messaging:** MPSC channels + async workers
-   **WebSocket Layer:** Actix Actors with binary broadcast support
-   **Metrics:** Prometheus with histogram-based latency tracking
-   **Optional:** Kafka integration for event streaming

* * * * *

**Note:** Benchmarks are localhost synthetic tests. Production systems face additional complexity including network latency, geographic distribution, authentication, and regulatory requirements.
