use crate::orderbook::Order;
use crate::persist::event::PersistEvent;
use scylla::{Session, SessionBuilder, prepared_statement::PreparedStatement};
use std::sync::Arc;
use uuid::Uuid;

pub struct ScyllaClient {
    session: Session,
    insert_order_stmt: Arc<PreparedStatement>,
    delete_order_stmt: Arc<PreparedStatement>,
    select_order_stmt: Arc<PreparedStatement>,
    update_order_stmt: Arc<PreparedStatement>,
    insert_trade_stmt: Arc<PreparedStatement>,
}

impl ScyllaClient {
    pub async fn new(uri: &str) -> Self {
        let session = loop {
            match SessionBuilder::new().known_node(uri).build().await {
                Ok(s) => break s,
                Err(e) => {
                    eprintln!("[Scylla] Retry in 3s: {:?}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        };

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

        let insert_order_stmt = Arc::new(
            session
                .prepare("INSERT INTO clob.orders (order_id, user_id, price, quantity, side) VALUES (?, ?, ?, ?, ?);")
                .await
                .unwrap(),
        );

        let delete_order_stmt = Arc::new(
            session
                .prepare("DELETE FROM clob.orders WHERE order_id = ?;")
                .await
                .unwrap(),
        );

        let select_order_stmt = Arc::new(
            session
                .prepare("SELECT quantity FROM clob.orders WHERE order_id = ?;")
                .await
                .unwrap(),
        );

        let update_order_stmt = Arc::new(
            session
                .prepare("UPDATE clob.orders SET quantity = ? WHERE order_id = ?;")
                .await
                .unwrap(),
        );

        let insert_trade_stmt = Arc::new(
            session
                .prepare("INSERT INTO clob.trades (trade_id, price, quantity, maker_order_id, taker_order_id, timestamp) VALUES (?, ?, ?, ?, ?, ?);")
                .await
                .unwrap(),
        );

        println!("[Scylla] Connected and schema initialized with prepared statements.");

        Self {
            session,
            insert_order_stmt,
            delete_order_stmt,
            select_order_stmt,
            update_order_stmt,
            insert_trade_stmt,
        }
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
            .execute(
                &self.insert_order_stmt,
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
            .execute(&self.delete_order_stmt, (order_id as i32,))
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
            .execute(&self.select_order_stmt, (order_id as i32,))
            .await?;

        if let Some(row) = result.rows.and_then(|mut r| r.pop()) {
            let current_qty: i32 = row.columns[0].as_ref().unwrap().as_int().unwrap();
            let new_qty = std::cmp::max(0, current_qty - traded_qty as i32);

            self.session
                .execute(&self.update_order_stmt, (new_qty, order_id as i32))
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
            .execute(
                &self.insert_trade_stmt,
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
                if let Err(e) = self.insert_order(order.clone()).await {
                    eprintln!(
                        "[Scylla] Failed to insert order {}: {:?}",
                        order.order_id, e
                    );
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

    pub async fn handle_event_batch(&self, events: Vec<PersistEvent>) {
        for event in events {
            self.handle_event(event).await;
        }
    }
}
