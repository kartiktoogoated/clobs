use actix_web::{App, HttpServer, web::Data};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::unbounded_channel;

use crate::orderbook::OrderBook;
use crate::persist::{client::ScyllaClient, event::PersistEvent, worker::start_persistence_worker};
use crate::routes::{create_order, delete_order, get_depth};
use crate::worker::{Broadcaster, ws_index};

pub mod inputs;
pub mod kafka_worker;
pub mod orderbook;
pub mod outputs;
pub mod persist;
pub mod routes;
pub mod worker;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let scylla = ScyllaClient::new("127.0.0.1:9042").await;
    let (tx, rx) = unbounded_channel::<PersistEvent>();
    start_persistence_worker(rx, scylla).await;

    let broadcaster = Arc::new(Broadcaster::new());
    let orderbook = Arc::new(Mutex::new(OrderBook::new(tx, broadcaster.clone())));

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(orderbook.clone()))
            .app_data(Data::new(broadcaster.clone()))
            .service(create_order)
            .service(delete_order)
            .service(get_depth)
            .route("/ws", actix_web::web::get().to(ws_index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
