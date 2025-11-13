use crate::{
    events::OrderEvent,
    metrics::{CHANNEL_BUFFER_SIZE, MATCHING_LATENCY_MS, ORDERS_MATCHED_TOTAL},
    orderbook::OrderBook,
    outputs::Depth,
    persist::event::PersistEvent,
    worker::Broadcaster,
};
use ringbuf::traits::Observer;

use parking_lot::RwLock;
use ringbuf::{HeapCons, traits::Consumer};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

pub async fn start_matching_loop(
    mut order_rx: HeapCons<OrderEvent>,
    tx_persist: UnboundedSender<PersistEvent>,
    broadcaster: Arc<Broadcaster>,
    depth_snapshot: Arc<RwLock<Depth>>,
    market: String,
) {
    let mut orderbook = OrderBook::new(tx_persist.clone(), broadcaster.clone());
    let mut events_processed = 0u64;
    let mut idle_iterations = 0u32;

    loop {
        match order_rx.try_pop() {
            Some(event) => {
                idle_iterations = 0;
                let start = Instant::now();

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
                        ORDERS_MATCHED_TOTAL.inc();
                    }
                    OrderEvent::DeleteOrder { order_id } => {
                        let _ = orderbook.delete_order(order_id);
                    }
                }
                events_processed += 1;

                if events_processed % 100 == 0 {
                    update_depth_snapshot(&orderbook, &depth_snapshot);
                }
                MATCHING_LATENCY_MS.observe(start.elapsed().as_secs_f64() * 1000.0);
                CHANNEL_BUFFER_SIZE.set(order_rx.occupied_len() as i64);
            }
            None => {
                idle_iterations += 1;

                if idle_iterations == 1 {
                    update_depth_snapshot(&orderbook, &depth_snapshot);
                }

                if idle_iterations < 1000 {
                    std::hint::spin_loop();
                } else {
                    tokio::task::yield_now().await;
                    idle_iterations = 0;
                }
            }
        }
    }
}

#[inline]
fn update_depth_snapshot(orderbook: &OrderBook, depth_snapshot: &Arc<RwLock<Depth>>) {
    let depth = orderbook.get_depth(10);
    let mut snapshot = depth_snapshot.write();
    snapshot.bids = depth.bids;
    snapshot.asks = depth.asks;
    snapshot.lastUpdateId = depth.lastUpdateId;
}
