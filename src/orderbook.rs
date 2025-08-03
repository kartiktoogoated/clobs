use crate::inputs::Side;
use crate::outputs::Depth;
use crate::persist::PersistEvent;
use std::collections::{BTreeMap, HashMap, VecDeque};
use tokio::sync::mpsc::UnboundedSender;

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
}

impl OrderBook {
    pub fn new(tx: UnboundedSender<PersistEvent>) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_id_map: HashMap::new(),
            next_id: 1,
            tx,
        }
    }

    pub fn match_limit_order(&mut self, mut order: Order) {
        let book = match order.side {
            Side::Buy => &mut self.asks,
            Side::Sell => &mut self.bids,
        };

        let matching_prices: Vec<u32> = match order.side {
            Side::Buy => book.keys().cloned().filter(|&p| p <= order.price).collect(), // Buy: match lower or equal asks
            Side::Sell => book
                .keys()
                .cloned()
                .rev()
                .filter(|&p| p >= order.price)
                .collect(), // Sell: match higher or equal bid
        };

        for price in matching_prices {
            let queue = book.get_mut(&price).unwrap();

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
                    "Matched: {} {:?} {} @ {} with {}",
                    traded_qty, order.side, price, order.user_id, resting_order.user_id
                );

                let trade_event = PersistEvent::TradeExecuted {
                    trade_id: uuid::Uuid::new_v4(),
                    price,
                    quantity: traded_qty,
                    maker_order_id: resting_order.order_id,
                    taker_order_id: order.order_id,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };

                let _ = self.tx.send(trade_event);

                // Removing the resting order if fully filled
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

        // If remaining, push to the book
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
                .insert(order.order_id, (order.side.clone(), order.price));
        }
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
                    // Clean up empty levels
                    if queue.is_empty() {
                        book.remove(&price);
                    }
                    return (order.quantity, order.price); // (qty removed, avg price)
                }
            }
        }

        (0, 0) // not found
    }

    pub fn get_depth(&self, depth_limit: usize) -> Depth {
        let mut bids = self
            .bids
            .iter()
            .rev() // highest price first
            .map(|(&price, orders)| {
                let total_qty: u32 = orders.iter().map(|o| o.quantity).sum();
                [price, total_qty]
            })
            .take(depth_limit)
            .collect();

        let mut asks = self
            .asks
            .iter() // lowest price first
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
