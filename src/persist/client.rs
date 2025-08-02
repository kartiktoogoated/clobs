use crate::orderbook::Order;
use scylla::{Session, SessionBuilder};

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

        // Optional: create keyspace/table if not exists
        session
            .query(
                "CREATE KEYSPACE IF NOT EXISTS clob WITH REPLICATION = { 'class': 'SimpleStrategy', 'replication_factor': 1 };",
                &[],
            )
            .await
            .unwrap();

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
                "INSERT INTO clob.orders (order_id, user_id, price, quantity, side) VALUES (?, ?, ?, ?, ?);",
                (order.order_id as i32, order.user_id as i32, order.price as i32, order.quantity as i32, side_str),
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
        qty: u32,
    ) -> Result<(), scylla::transport::errors::QueryError> {
        self.session
            .query(
                "UPDATE clob.orders SET quantity = quantity - ? WHERE order_id = ?;",
                (qty as i32, order_id as i32),
            )
            .await?;
        Ok(())
    }
}

