use crate::inputs::Side;
use crate::outputs::Depth;
use crate::persist::PersistEvent;
use crate::worker::Broadcaster;

use chrono::Utc;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use serde::{Deserialize, Serialize};
use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct Order {
    pub order_id: u32,
    pub user_id: u32,
    pub price: u32,
    pub quantity: u32,
    pub side: Side,
}

#[derive(SchemaWrite, SchemaRead)]
struct DepthUpdate {
    msg_type: &'static str,
    depth: WireDepth,
}

#[derive(SchemaWrite, SchemaRead)]
struct TradeMsg {
    msg_type: &'static str,
    price: u32,
    quantity: u32,
    maker_order_id: u32,
    taker_order_id: u32,
    timestamp: i64,
}

#[derive(SchemaWrite, SchemaRead)]
struct WireDepth {
    bids: Vec<[u32; 2]>,
    asks: Vec<[u32; 2]>,
    last_update_id: String,
}

pub struct OrderBook {
    pub bids: BTreeMap<u32, VecDeque<Order>>,
    pub asks: BTreeMap<u32, VecDeque<Order>>,
    pub order_id_map: HashMap<u32, (Side, u32)>,
    pub tx: UnboundedSender<PersistEvent>,
    pub broadcaster: Arc<Broadcaster>,
}

impl OrderBook {
    pub fn new(tx: UnboundedSender<PersistEvent>, broadcaster: Arc<Broadcaster>) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_id_map: HashMap::new(),
            tx,
            broadcaster,
        }
    }

    pub fn broadcast_depth(&self) {
        let depth = self.get_depth(10);
        let packet = DepthUpdate {
            msg_type: "depth_update",
            depth: WireDepth {
                bids: depth.bids.clone(),
                asks: depth.asks.clone(),
                last_update_id: depth.lastUpdateId.clone(),
            },
        };

        let encoded = wincode::serialize(&packet).expect("serialize depth");
        self.broadcaster.broadcast_bytes(&encoded);
    }

    pub fn match_limit_order(&mut self, mut order: Order) {
        let book = match order.side {
            Side::Buy => &mut self.asks,
            Side::Sell => &mut self.bids,
        };

        let price_iter: Vec<u32> = match order.side {
            Side::Buy => book.range(..=order.price).map(|(&p, _)| p).collect(),
            Side::Sell => book.range(order.price..).rev().map(|(&p, _)| p).collect(),
        };

        for price in price_iter {
            if order.quantity == 0 {
                break;
            }

            let queue = match book.get_mut(&price) {
                Some(q) => q,
                None => continue,
            };

            while let Some(resting_order) = queue.front_mut() {
                if order.quantity == 0 {
                    break;
                }

                let traded_qty = order.quantity.min(resting_order.quantity);
                order.quantity -= traded_qty;
                resting_order.quantity -= traded_qty;

                crate::metrics::TRADES_EXECUTED.inc();

                let _ = self.tx.send(PersistEvent::OrderFilled {
                    order_id: resting_order.order_id,
                    traded_qty,
                });

                let trade_msg = TradeMsg {
                    msg_type: "trade",
                    price,
                    quantity: traded_qty,
                    maker_order_id: resting_order.order_id,
                    taker_order_id: order.order_id,
                    timestamp: Utc::now().timestamp_millis(),
                };

                let encoded = wincode::serialize(&trade_msg).expect("serialize trade");
                self.broadcaster.broadcast_bytes(&encoded);

                let _ = self.tx.send(PersistEvent::TradeExecuted {
                    trade_id: Uuid::new_v4(),
                    price,
                    quantity: traded_qty,
                    maker_order_id: resting_order.order_id,
                    taker_order_id: order.order_id,
                    timestamp: Utc::now().timestamp_millis(),
                });

                if resting_order.quantity == 0 {
                    queue.pop_front();
                } else {
                    break;
                }
            }

            if queue.is_empty() {
                book.remove(&price);
            }
        }

        if order.quantity > 0 {
            let side_book = match order.side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };

            self.order_id_map
                .insert(order.order_id, (order.side, order.price));

            let _ = self.tx.send(PersistEvent::NewOrder(order.clone()));
            side_book.entry(order.price).or_default().push_back(order);
        }
    }

    pub fn delete_order(&mut self, order_id: u32) -> (u32, u32) {
        if let Some((side, price)) = self.order_id_map.remove(&order_id) {
            let book = match side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };

            if let Some(queue) = book.get_mut(&price) {
                if let Some(pos) = queue.iter().position(|o| o.order_id == order_id) {
                    let order = queue.remove(pos).unwrap();
                    let _ = self.tx.send(PersistEvent::OrderDeleted { order_id });
                    if queue.is_empty() {
                        book.remove(&price);
                    }
                    return (order.quantity, order.price);
                }
            }
        }

        (0, 0)
    }

    pub fn get_depth(&self, depth_limit: usize) -> Depth {
        let bids = self
            .bids
            .iter()
            .rev()
            .take(depth_limit)
            .map(|(&price, orders)| {
                let total_qty: u32 = orders.iter().map(|o| o.quantity).sum();
                [price, total_qty]
            })
            .collect();

        let asks = self
            .asks
            .iter()
            .take(depth_limit)
            .map(|(&price, orders)| {
                let total_qty: u32 = orders.iter().map(|o| o.quantity).sum();
                [price, total_qty]
            })
            .collect();

        Depth {
            bids,
            asks,
            lastUpdateId: "0".to_string(),
        }
    }
}
