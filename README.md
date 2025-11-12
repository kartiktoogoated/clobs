Central Limit Order Book (CLOB) Engine
========================================

A high-performance **Central Limit Order Book (CLOB)** backend built with **Rust**, **Actix-Web**, and **ScyllaDB**, featuring **real-time trade and depth broadcasting** over WebSockets.\
It simulates how real exchanges (like Binance or Coinbase) match orders, persist data, and stream updates to connected clients.

* * * * *

Features
-----------

### Core Engine

-   Full in-memory **limit order book** using `BTreeMap` and `VecDeque` for O(log n) price-level lookups.

-   Supports both **buy (bid)** and **sell (ask)** orders.

-   Implements price--time priority matching.

-   Automatically removes filled orders and empty levels.

### Real-Time WebSocket Broadcasts

-   Emits **`trade`** and **`depth_update`** messages to all connected clients.

-   Follows exchange-style streaming updates for live order book visualization.

-   Example WebSocket messages:

    `{
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
    }`

### Persistent Storage (ScyllaDB)

-   All order and trade data is stored in **ScyllaDB** for durability.

-   Schema includes:

    -   `clob.orders` for open orders

    -   `clob.trades` for historical trades

-   Background worker consumes persistence events asynchronously.

### REST API (Actix-Web)

| Method | Endpoint | Description |
| --- | --- | --- |
| `POST` | `/order` | Create a new order |
| `DELETE` | `/order` | Cancel an existing order |
| `GET` | `/depth` | Fetch top 10 levels of order book |

Example:

`curl -X POST http://127.0.0.1:8080/order\
  -H "Content-Type: application/json"\
  -d '{"price":100,"quantity":5,"user_id":1,"side":"Buy"}'`

### Scylla Persistence Worker

-   Runs in the background, handling:

    -   `NewOrder`

    -   `OrderFilled`

    -   `TradeExecuted`

    -   `OrderDeleted`

-   Inserts and updates records in ScyllaDB through the async driver.

* * * * *

Project Structure
---------------------
<img width="628" height="280" alt="Screenshot 2025-11-05 at 10 48 20 PM" src="https://github.com/user-attachments/assets/0c711164-7b3d-40f8-b2f8-8b738e574cb4" />

* * * * *

Example Flow
---------------

1.  A client submits a **buy order** using `/order`.

2.  The orderbook matches it against existing **sell orders**.

3.  If a trade occurs:

    -   `OrderFilled` and `TradeExecuted` are persisted to ScyllaDB.

    -   A live `trade` broadcast is sent via WebSocket.

    -   The top-10 depth snapshot is broadcast as `depth_update`.

4.  Unfilled portions are added to the orderbook.

* * * * *

Running Locally
-------------------

### **Start ScyllaDB (Docker)**

`docker run -d --name scylla -p 9042:9042 scylladb/scylla`

### **Run the Backend**

`cargo run`

Server starts on:

`http://127.0.0.1:8080`

* * * * *

WebSocket Testing
--------------------

Use `wscat` to connect:

`npx wscat -c ws://127.0.0.1:8080/ws`

Then send a few orders via `curl` --- you'll see live JSON depth and trade updates appear instantly in your WebSocket terminal.

* * * * *

Tech Stack
-------------

-   **Language:** Rust

-   **Framework:** Actix-Web

-   **Database:** ScyllaDB

-   **Async runtime:** Tokio

-   **Serialization:** Serde JSON

-   **Messaging:** MPSC channel + async worker

-   **WebSocket Layer:** Actix Actors

* * * * *
