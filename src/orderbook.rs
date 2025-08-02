use crate::inputs::Side;
use std::collections::{BTreeMap, HashMap, VecDeque};

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
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_id_map: HashMap::new(),
            next_id: 1,
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
                resting_order.quantity -= traded_qty;

                println!(
                    "Matched: {} {:?} {} @ {} with {}",
                    traded_qty, order.side, price, order.user_id, resting_order.user_id
                );

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

        match side {
            Side::Buy => self.bids.entry(price).or_default().push_back(order.clone()),
            Side::Sell => self.bids.entry(price).or_default().push_back(order.clone()),
        }

        self.order_id_map.insert(order_id, (side, price));
        order_id
    }
}
