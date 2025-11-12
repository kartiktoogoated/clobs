use actix_web::{App, HttpServer, web::Data};
use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Split};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::{RwLock, mpsc};

use crate::events::{MatchEvent, OrderEvent};
use crate::matching_loop::start_matching_loop;
use crate::metrics::start_console_metrics_printer;
use crate::outputs::Depth;
use crate::persist::{client::ScyllaClient, event::PersistEvent, worker::start_persistence_worker};
use crate::routes::{create_order, delete_order, get_depth, metrics_endpoint};
use crate::worker::{Broadcaster, ws_index};

pub mod events;
pub mod inputs;
pub mod kafka_worker;
pub mod matching_loop;
pub mod metrics;
pub mod msgpack;
pub mod orderbook;
pub mod outputs;
pub mod persist;
pub mod routes;
pub mod worker;

pub static ORDER_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    start_console_metrics_printer();

    let scylla = ScyllaClient::new("127.0.0.1:9042").await;
    let (tx_persist, rx_persist) = mpsc::unbounded_channel::<PersistEvent>();
    start_persistence_worker(rx_persist, scylla).await;

    let broadcaster = Broadcaster::new();
    let broadcaster_arc = Arc::new(broadcaster.clone());

    let depth_snapshot = Arc::new(RwLock::new(Depth {
        bids: vec![],
        asks: vec![],
        lastUpdateId: "0".to_string(),
    }));

    let (order_tx, mut order_rx) = mpsc::unbounded_channel::<OrderEvent>();

    let order_rb = HeapRb::<OrderEvent>::new(65536);
    let match_rb = HeapRb::<MatchEvent>::new(32768);

    let (mut order_prod, order_cons) = order_rb.split();
    let (match_prod, match_cons) = match_rb.split();

    tokio::spawn(async move {
        while let Some(event) = order_rx.recv().await {
            loop {
                match order_prod.try_push(event) {
                    Ok(_) => break,
                    Err(returned_event) => {
                        tokio::task::yield_now().await;
                    }
                }
            }
        }
    });

    let order_sender = Arc::new(order_tx);

    start_matching_loop(order_cons, match_prod, tx_persist, broadcaster_arc.clone()).await;

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(order_sender.clone()))
            .app_data(Data::new(broadcaster.clone()))
            .app_data(Data::new(depth_snapshot.clone()))
            .service(create_order)
            .service(delete_order)
            .service(get_depth)
            .service(metrics_endpoint)
            .route("/ws", actix_web::web::get().to(ws_index))
    })
    .bind("127.0.0.1:8080")?
    .workers(16)
    .run()
    .await
}
