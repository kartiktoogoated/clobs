use ringbuf::traits::{Consumer, Producer};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    events::{MatchEvent, OrderEvent},
    metrics::{DEPTH_UPDATES, MATCHING_LATENCY_MS, ORDERS_MATCHED_TOTAL},
    orderbook::OrderBook,
    persist::event::PersistEvent,
    worker::Broadcaster,
};

pub async fn start_matching_loop(
    mut order_rx: ringbuf::HeapCons<OrderEvent>,
    mut match_tx: ringbuf::HeapProd<MatchEvent>,
    tx_persist: UnboundedSender<PersistEvent>,
    broadcaster: Arc<Broadcaster>,
) {
    let mut orderbook = OrderBook::new(tx_persist.clone(), broadcaster.clone());

    tokio::spawn(async move {
        let mut depth_update_counter = 0;

        loop {
            let mut processed = 0;
            let batch_start = std::time::Instant::now();

            while processed < 200 && batch_start.elapsed().as_micros() < 2000 {
                if let Some(event) = order_rx.try_pop() {
                    let start = std::time::Instant::now();

                    match event {
                        OrderEvent::NewOrder {
                            order_id,
                            user_id,
                            price,
                            quantity,
                            side,
                        } => {
                            orderbook.match_limit_order(crate::orderbook::Order {
                                order_id,
                                user_id,
                                price,
                                quantity,
                                side,
                            });
                        }
                        OrderEvent::DeleteOrder { order_id } => {
                            orderbook.delete_order(order_id);
                        }
                    }

                    ORDERS_MATCHED_TOTAL.inc();
                    MATCHING_LATENCY_MS.observe(start.elapsed().as_secs_f64() * 1000.0);
                    processed += 1;
                    depth_update_counter += 1;
                } else {
                    break;
                }
            }

            if depth_update_counter >= 200 {
                orderbook.broadcast_depth();
                DEPTH_UPDATES.inc();
                depth_update_counter = 0;
            }

            if processed == 0 {
                tokio::task::yield_now().await;
            }
        }
    });
}
