use crate::orderbook::Order;
use crate::persist::event::PersistEvent;
use scylla::{Session, SessionBuilder};
use uuid::Uuid;

pub struct ScyllaClient {
    session: Session,
}

impl ScyllaClient {
    pub async fn new(uri: &str) -> Self {
        let session = SessionBuilder::new()
            .known_node(uri)
            .build()
            .await
            .expect("Failed to connect to ScyllaDB");

        // Ensure keyspace
        session
            .query(
                "CREATE KEYSPACE IF NOT EXISTS clob \
                 WITH REPLICATION = { 'class': 'SimpleStrategy', 'replication_factor': 1 };",
                &[],
            )
            .await
            .unwrap();

        // Orders table
        session
            .query(
                "CREATE TABLE IF NOT EXISTS clob.orders (
                    order_id int PRIMARY KEY,
                    user_id int,
                    price int,
                    quantity int,
                    side text
                );",
                &[],
            )
            .await
            .unwrap();

        // Trades table
        session
            .query(
                "CREATE TABLE IF NOT EXISTS clob.trades (
                    trade_id uuid PRIMARY KEY,
                    price int,
                    quantity int,
                    maker_order_id int,
                    taker_order_id int,
                    timestamp bigint
                );",
                &[],
            )
            .await
            .unwrap();

        println!("[Scylla] Connected and schema initialized.");
        Self { session }
    }

    pub async fn insert_order(
        &self,
        order: Order,
    ) -> Result<(), scylla::transport::errors::QueryError> {
        let side_str = match order.side {
            crate::inputs::Side::Buy => "buy",
            crate::inputs::Side::Sell => "sell",
        };

        self.session
            .query(
                "INSERT INTO clob.orders (order_id, user_id, price, quantity, side) \
                 VALUES (?, ?, ?, ?, ?);",
                (
                    order.order_id as i32,
                    order.user_id as i32,
                    order.price as i32,
                    order.quantity as i32,
                    side_str,
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn delete_order(
        &self,
        order_id: u32,
    ) -> Result<(), scylla::transport::errors::QueryError> {
        self.session
            .query(
                "DELETE FROM clob.orders WHERE order_id = ?;",
                (order_id as i32,),
            )
            .await?;
        Ok(())
    }

    pub async fn mark_filled(
        &self,
        order_id: u32,
        traded_qty: u32,
    ) -> Result<(), scylla::transport::errors::QueryError> {
        let result = self
            .session
            .query(
                "SELECT quantity FROM clob.orders WHERE order_id = ?;",
                (order_id as i32,),
            )
            .await?;

        if let Some(row) = result.rows.and_then(|mut r| r.pop()) {
            let current_qty: i32 = row.columns[0].as_ref().unwrap().as_int().unwrap();
            let new_qty = std::cmp::max(0, current_qty - traded_qty as i32);

            self.session
                .query(
                    "UPDATE clob.orders SET quantity = ? WHERE order_id = ?;",
                    (new_qty, order_id as i32),
                )
                .await?;
        }

        Ok(())
    }

    pub async fn insert_trade(
        &self,
        trade_id: [u8; 16],
        price: u32,
        quantity: u32,
        maker_order_id: u32,
        taker_order_id: u32,
        timestamp: i64,
    ) -> Result<(), scylla::transport::errors::QueryError> {
        let trade_id = Uuid::from_bytes(trade_id);

        self.session
            .query(
                "INSERT INTO clob.trades (trade_id, price, quantity, maker_order_id, taker_order_id, timestamp) \
                 VALUES (?, ?, ?, ?, ?, ?);",
                (
                    trade_id,
                    price as i32,
                    quantity as i32,
                    maker_order_id as i32,
                    taker_order_id as i32,
                    timestamp,
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn handle_event(&self, event: PersistEvent) {
        match event {
            PersistEvent::NewOrder(order) => {
                if let Err(e) = self.insert_order(order).await {
                    eprintln!("[Scylla] Failed to insert order: {:?}", e);
                }
            }
            PersistEvent::OrderDeleted { order_id } => {
                if let Err(e) = self.delete_order(order_id).await {
                    eprintln!("[Scylla] Failed to delete order {}: {:?}", order_id, e);
                }
            }
            PersistEvent::OrderFilled {
                order_id,
                traded_qty,
            } => {
                if let Err(e) = self.mark_filled(order_id, traded_qty).await {
                    eprintln!(
                        "[Scylla] Failed to mark order {} filled (qty={}): {:?}",
                        order_id, traded_qty, e
                    );
                }
            }
            PersistEvent::TradeExecuted {
                trade_id,
                price,
                quantity,
                maker_order_id,
                taker_order_id,
                timestamp,
            } => {
                if let Err(e) = self
                    .insert_trade(
                        trade_id,
                        price,
                        quantity,
                        maker_order_id,
                        taker_order_id,
                        timestamp,
                    )
                    .await
                {
                    eprintln!("[Scylla] Failed to insert trade {:?}: {:?}", trade_id, e);
                }
            }
        }
    }
}
