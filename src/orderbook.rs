use crate::inputs::Side;
use crate::outputs::Depth;
use crate::persist::PersistEvent;
use crate::worker::Broadcaster;
use chrono::Utc;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Order {
    pub order_id: u32,
    pub user_id: u32,
    pub price: u32,
    pub quantity: u32,
    pub side: Side,
}

pub struct OrderBook {
    pub bids: BTreeMap<u32, VecDeque<Order>>,
    pub asks: BTreeMap<u32, VecDeque<Order>>,
    pub order_id_map: HashMap<u32, (Side, u32)>,
    pub next_id: u32,
    pub tx: UnboundedSender<PersistEvent>,
    pub broadcaster: Arc<Broadcaster>,
}

impl OrderBook {
    pub fn new(tx: UnboundedSender<PersistEvent>, broadcaster: Arc<Broadcaster>) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_id_map: HashMap::new(),
            next_id: 1,
            tx,
            broadcaster,
        }
    }

    pub fn broadcast_depth(&self) {
        let depth = self.get_depth(10);
        let msg = serde_json::to_string(&json!({
            "type": "depth_update",
            "depth": depth
        }))
        .unwrap();
        self.broadcaster.broadcast(&msg);
    }

    pub fn match_limit_order(&mut self, mut order: Order) {
        let book = match order.side {
            Side::Buy => &mut self.asks,
            Side::Sell => &mut self.bids,
        };

        let matching_prices: Vec<u32> = match order.side {
            Side::Buy => book.keys().cloned().filter(|&p| p <= order.price).collect(),
            Side::Sell => book
                .keys()
                .cloned()
                .rev()
                .filter(|&p| p >= order.price)
                .collect(),
        };

        for price in matching_prices {
            let queue = match book.get_mut(&price) {
                Some(q) => q,
                None => continue,
            };

            while let Some(mut resting_order) = queue.front_mut() {
                if order.quantity == 0 {
                    break;
                }

                let traded_qty = order.quantity.min(resting_order.quantity);
                order.quantity -= traded_qty;
                resting_order.quantity -= traded_qty;

                let _ = self.tx.send(PersistEvent::OrderFilled {
                    order_id: resting_order.order_id,
                    traded_qty,
                });

                println!(
                    "Matched: {} {:?} {} @price = {} (maker={}, taker={})",
                    traded_qty,
                    order.side,
                    order.user_id,
                    price,
                    resting_order.order_id,
                    order.user_id,
                );

                let trade_msg = serde_json::to_string(&json!({
                    "type": "trade",
                    "price": price,
                    "quantity": traded_qty,
                    "maker_order_id": resting_order.order_id,
                    "taker_order_id": order.order_id,
                    "timestamp": Utc::now().timestamp_millis(),
                }))
                .unwrap();
                self.broadcaster.broadcast(&trade_msg);

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
                }
            }

            if queue.is_empty() {
                book.remove(&price);
            }

            if order.quantity == 0 {
                break;
            }
        }

        if order.quantity > 0 {
            let side_book = match order.side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };

            side_book
                .entry(order.price)
                .or_default()
                .push_back(order.clone());
            self.order_id_map
                .insert(order.order_id, (order.side, order.price));
            let _ = self.tx.send(PersistEvent::NewOrder(order.clone()));
        }

        self.broadcast_depth();
    }

    pub fn create_order(&mut self, price: u32, quantity: u32, user_id: u32, side: Side) -> u32 {
        let order_id = self.next_id;
        self.next_id += 1;

        let order = Order {
            order_id,
            user_id,
            price,
            quantity,
            side,
        };

        let clone_for_tx = order.clone();
        self.match_limit_order(order);
        let _ = self.tx.send(PersistEvent::NewOrder(clone_for_tx));
        order_id
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
            .map(|(&price, orders)| {
                let total_qty: u32 = orders.iter().map(|o| o.quantity).sum();
                [price, total_qty]
            })
            .take(depth_limit)
            .collect();

        let asks = self
            .asks
            .iter()
            .map(|(&price, orders)| {
                let total_qty: u32 = orders.iter().map(|o| o.quantity).sum();
                [price, total_qty]
            })
            .take(depth_limit)
            .collect();

        Depth {
            bids,
            asks,
            lastUpdateId: self.next_id.to_string(),
        }
    }
}
