use crate::metrics::ORDER_PROCESSING_LATENCY_MS;
use chrono::Utc;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;
use tracing::error;
use uuid::Uuid;

use crate::error::*;
use crate::inputs::Side;
use crate::outputs::Depth;
use crate::persist::PersistEvent;
use crate::worker::Broadcaster;

use wincode_derive::{SchemaRead, SchemaWrite};

pub const PRICE_SCALE: u64 = 1_000_000;
pub const QTY_SCALE: u64 = 1_000_000;

#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub struct Order {
    pub order_id: u32,
    pub user_id: u32,
    pub price: u64,
    pub quantity: u64,
    pub side: Side,
}

impl Order {
    pub fn validate(&self) -> Result<()> {
        if self.price == 0 {
            return Err(OrderBookError::InvalidOrder("Price cannot be zero".into()));
        }
        if self.quantity == 0 {
            return Err(OrderBookError::InvalidOrder(
                "Quantity cannot be zero".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct OrderLocation {
    side: Side,
    price: u64,
    index: usize,
}

pub struct PriceLevel {
    prices: Vec<u32>,
    users: Vec<u32>,
    quantities: Vec<u64>,
    tombstone: Vec<bool>,
    head: usize,
    total_qty: u64,
}

impl PriceLevel {
    #[inline]
    fn new() -> Self {
        Self {
            prices: Vec::with_capacity(16),
            users: Vec::with_capacity(16),
            quantities: Vec::with_capacity(16),
            tombstone: Vec::with_capacity(16),
            head: 0,
            total_qty: 0,
        }
    }

    #[inline]
    fn push(&mut self, order: &Order) -> Result<()> {
        let total_qty = self
            .total_qty
            .checked_add(order.quantity)
            .ok_or_else(|| OrderBookError::InvalidOrder("Price-level quantity overflow".into()))?;

        self.prices.push(order.order_id);
        self.users.push(order.user_id);
        self.quantities.push(order.quantity);
        self.tombstone.push(false);
        self.total_qty = total_qty;
        Ok(())
    }

    #[inline]
    fn remove_fast(&mut self, idx: usize) -> Result<u64> {
        if idx >= self.tombstone.len() {
            return Err(OrderBookError::InvalidOrder(format!(
                "Index {} out of bounds",
                idx
            )));
        }

        let qty = self.quantities[idx];
        if !self.tombstone[idx] {
            self.total_qty = self.total_qty.saturating_sub(qty);
            self.tombstone[idx] = true;
        }
        Ok(qty)
    }

    #[inline]
    fn advance_head(&mut self) {
        while self.head < self.tombstone.len() && self.tombstone[self.head] {
            self.head += 1;
        }
    }

    #[inline]
    fn should_compact(&self) -> bool {
        self.head >= 1024 && self.head >= self.prices.len() / 2
    }

    fn compact(&mut self, order_locations: &mut HashMap<u32, OrderLocation>) {
        if !self.should_compact() {
            return;
        }

        let removed = self.head;
        self.prices.drain(..removed);
        self.users.drain(..removed);
        self.quantities.drain(..removed);
        self.tombstone.drain(..removed);
        self.head = 0;

        for (index, &order_id) in self.prices.iter().enumerate() {
            if !self.tombstone[index]
                && let Some(location) = order_locations.get_mut(&order_id)
            {
                location.index = index;
            }
        }
    }

    #[inline]
    fn reduce_qty(&mut self, idx: usize, new_qty: u64) -> Result<()> {
        if idx >= self.quantities.len() {
            return Err(OrderBookError::InvalidOrder(format!(
                "Index {} out of bounds",
                idx
            )));
        }

        let old = self.quantities[idx];
        if new_qty > old {
            return Err(OrderBookError::InvalidOrder(format!(
                "New quantity {} exceeds old {}",
                new_qty, old
            )));
        }

        self.quantities[idx] = new_qty;
        self.total_qty = self.total_qty.saturating_sub(old).saturating_add(new_qty);
        Ok(())
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.total_qty == 0
    }
}

#[derive(SchemaWrite, SchemaRead)]
struct TradeMsg {
    msg_type: u8,
    price: u64,
    quantity: u64,
    maker_order_id: u32,
    taker_order_id: u32,
    timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct ExecutedTrade {
    pub price: u64,
    pub quantity: u64,
    pub maker_order_id: u32,
    pub taker_order_id: u32,
    pub timestamp: i64,
}

struct DepthCache {
    bids: [[u64; 2]; 20],
    asks: [[u64; 2]; 20],
    bid_count: usize,
    ask_count: usize,
    dirty: bool,
}

pub struct OrderBook {
    pub bids: BTreeMap<u64, PriceLevel>,
    pub asks: BTreeMap<u64, PriceLevel>,

    order_locations: HashMap<u32, OrderLocation>,
    depth_cache: DepthCache,

    trade_buf: Vec<TradeMsg>,

    pub tx: UnboundedSender<PersistEvent>,
    pub broadcaster: Arc<Broadcaster>,
}

impl OrderBook {
    pub fn new(tx: UnboundedSender<PersistEvent>, broadcaster: Arc<Broadcaster>) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_locations: HashMap::with_capacity(10000),

            depth_cache: DepthCache {
                bids: [[0; 2]; 20],
                asks: [[0; 2]; 20],
                bid_count: 0,
                ask_count: 0,
                dirty: true,
            },

            trade_buf: Vec::with_capacity(64),

            tx,
            broadcaster,
        }
    }

    pub fn match_limit_order(&mut self, mut taker: Order) -> Result<Vec<ExecutedTrade>> {
        self.match_limit_order_inner(&mut taker, true)
    }

    pub fn process_limit_order(&mut self, mut taker: Order) -> Result<()> {
        self.match_limit_order_inner(&mut taker, false).map(drop)
    }

    fn match_limit_order_inner(
        &mut self,
        taker: &mut Order,
        collect_executed_trades: bool,
    ) -> Result<Vec<ExecutedTrade>> {
        let process_start = Instant::now();

        taker.validate()?;

        if self.order_locations.contains_key(&taker.order_id) {
            return Err(OrderBookError::InvalidOrder(format!(
                "Order ID {} is already active",
                taker.order_id
            )));
        }

        let timestamp = Utc::now().timestamp_millis();
        let mut executed_trades = Vec::new();

        let matching_asks = matches!(taker.side, Side::Buy);

        {
            let (book, order_locations) = if matching_asks {
                (&mut self.asks, &mut self.order_locations)
            } else {
                (&mut self.bids, &mut self.order_locations)
            };

            loop {
                if taker.quantity == 0 {
                    break;
                }

                let Some(price) = (if matching_asks {
                    book.first_key_value().map(|(&price, _)| price)
                } else {
                    book.last_key_value().map(|(&price, _)| price)
                }) else {
                    break;
                };

                let crosses = if matching_asks {
                    price <= taker.price
                } else {
                    price >= taker.price
                };
                if !crosses {
                    break;
                }

                let level = book
                    .get_mut(&price)
                    .expect("best price must remain present while matching");

                while taker.quantity > 0 {
                    level.advance_head();
                    if level.head >= level.prices.len() {
                        break;
                    }

                    let idx = level.head;
                    let maker_id = level.prices[idx];
                    let maker_qty = level.quantities[idx];
                    let traded = taker.quantity.min(maker_qty);

                    taker.quantity -= traded;

                    let new_maker_qty = maker_qty - traded;
                    level.reduce_qty(idx, new_maker_qty)?;

                    crate::metrics::TRADES_EXECUTED.inc();

                    self.trade_buf.push(TradeMsg {
                        msg_type: 1,
                        price,
                        quantity: traded,
                        maker_order_id: maker_id,
                        taker_order_id: taker.order_id,
                        timestamp,
                    });

                    if collect_executed_trades {
                        executed_trades.push(ExecutedTrade {
                            price,
                            quantity: traded,
                            maker_order_id: maker_id,
                            taker_order_id: taker.order_id,
                            timestamp,
                        });
                    }

                    if new_maker_qty == 0 {
                        level.remove_fast(idx)?;
                        order_locations.remove(&maker_id);
                    } else {
                        // A partially filled maker means the taker is exhausted.
                        break;
                    }
                }

                if level.is_empty() {
                    book.remove(&price);
                } else {
                    level.advance_head();
                    level.compact(order_locations);
                }
            }
        }

        if taker.quantity > 0 {
            self.insert_resting_order(taker.clone())?;
        }

        self.flush_trades()?;
        self.depth_cache.dirty = true;
        ORDER_PROCESSING_LATENCY_MS.observe(process_start.elapsed().as_secs_f64() * 1000.0);

        Ok(executed_trades)
    }

    #[inline]
    fn insert_resting_order(&mut self, order: Order) -> Result<()> {
        order.validate()?;

        let book = match order.side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        let level = book.entry(order.price).or_insert_with(PriceLevel::new);
        let index = level.prices.len();

        level.push(&order)?;

        self.order_locations.insert(
            order.order_id,
            OrderLocation {
                side: order.side,
                price: order.price,
                index,
            },
        );

        self.tx.send(PersistEvent::NewOrder(order)).map_err(|e| {
            error!("Failed to persist order: {}", e);
            crate::metrics::PERSISTENCE_FAILURES.inc();
            OrderBookError::PersistenceFailed(format!("Channel send failed: {}", e))
        })?;

        Ok(())
    }

    pub fn delete_order(&mut self, order_id: u32) -> Result<()> {
        let loc = *self
            .order_locations
            .get(&order_id)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;

        let book = match loc.side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        let level = book
            .get_mut(&loc.price)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;
        level.remove_fast(loc.index)?;
        self.order_locations.remove(&order_id);

        if level.is_empty() {
            book.remove(&loc.price);
        } else {
            level.advance_head();
            level.compact(&mut self.order_locations);
        }

        self.depth_cache.dirty = true;
        Ok(())
    }

    #[inline]
    fn flush_trades(&mut self) -> Result<()> {
        if self.trade_buf.is_empty() {
            return Ok(());
        }

        let mut last_error: Option<OrderBookError> = None;

        for trade in self.trade_buf.drain(..) {
            match wincode::serialize(&trade) {
                Ok(encoded) => {
                    self.broadcaster.broadcast_bytes(&encoded);
                }
                Err(e) => {
                    error!("Serialization failed: {}", e);
                    crate::metrics::SERIALIZATION_FAILURES.inc();
                    last_error = Some(OrderBookError::SerializationFailed(e.to_string()));
                }
            }

            if let Err(e) = self.tx.send(PersistEvent::TradeExecuted {
                trade_id: Uuid::new_v4().into_bytes(),
                price: trade.price,
                quantity: trade.quantity,
                maker_order_id: trade.maker_order_id,
                taker_order_id: trade.taker_order_id,
                timestamp: trade.timestamp,
            }) {
                error!("Persistence failed: {}", e);
                crate::metrics::PERSISTENCE_FAILURES.inc();
                last_error = Some(OrderBookError::PersistenceFailed(e.to_string()));
            }
        }

        if let Some(err) = last_error {
            return Err(err);
        }

        Ok(())
    }

    pub fn get_depth(&mut self, limit: usize) -> Depth {
        if self.depth_cache.dirty {
            self.rebuild_depth_cache();
        }

        let bids = self.depth_cache.bids[..self.depth_cache.bid_count.min(limit)].to_vec();
        let asks = self.depth_cache.asks[..self.depth_cache.ask_count.min(limit)].to_vec();

        Depth {
            bids,
            asks,
            last_update_id: "0".to_string(),
        }
    }

    #[inline]
    fn rebuild_depth_cache(&mut self) {
        self.depth_cache.bid_count = 0;
        for (&price, level) in self.bids.iter().rev().take(20) {
            self.depth_cache.bids[self.depth_cache.bid_count] = [price, level.total_qty];
            self.depth_cache.bid_count += 1;
        }

        self.depth_cache.ask_count = 0;
        for (&price, level) in self.asks.iter().take(20) {
            self.depth_cache.asks[self.depth_cache.ask_count] = [price, level.total_qty];
            self.depth_cache.ask_count += 1;
        }

        self.depth_cache.dirty = false;
    }

    pub fn stats(&self) -> OrderBookStats {
        OrderBookStats {
            total_bids: self.bids.len(),
            total_asks: self.asks.len(),
            total_orders: self.order_locations.len(),
            best_bid: self.bids.keys().next_back().copied(),
            best_ask: self.asks.keys().next().copied(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderBookStats {
    pub total_bids: usize,
    pub total_asks: usize,
    pub total_orders: usize,
    pub best_bid: Option<u64>,
    pub best_ask: Option<u64>,
}

impl Drop for OrderBook {
    fn drop(&mut self) {
        if !self.trade_buf.is_empty()
            && let Err(e) = self.flush_trades()
        {
            error!("Failed to flush trades during drop: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn test_book() -> (OrderBook, UnboundedReceiver<PersistEvent>) {
        let (tx, rx) = unbounded_channel();
        (OrderBook::new(tx, Arc::new(Broadcaster::new())), rx)
    }

    fn order(order_id: u32, price: u64, quantity: u64, side: Side) -> Order {
        Order {
            order_id,
            user_id: order_id,
            price,
            quantity,
            side,
        }
    }

    #[test]
    fn matches_more_than_sixty_four_makers_without_losing_a_trade() {
        let (mut book, _rx) = test_book();

        for order_id in 1..=65 {
            book.match_limit_order(order(order_id, 100, 1, Side::Sell))
                .unwrap();
        }

        let trades = book
            .match_limit_order(order(1_000, 100, 65, Side::Buy))
            .unwrap();

        assert_eq!(trades.len(), 65);
        assert_eq!(trades.iter().map(|trade| trade.quantity).sum::<u64>(), 65);
        assert!(book.asks.is_empty());
        assert!(book.bids.is_empty());
        assert_eq!(book.stats().total_orders, 0);
    }

    #[test]
    fn removes_filled_makers_from_the_location_index() {
        let (mut book, _rx) = test_book();

        book.match_limit_order(order(1, 100, 5, Side::Sell))
            .unwrap();
        book.match_limit_order(order(2, 100, 5, Side::Buy)).unwrap();

        assert_eq!(book.stats().total_orders, 0);
        assert!(matches!(
            book.delete_order(1),
            Err(OrderBookError::OrderNotFound(1))
        ));
    }

    #[test]
    fn compaction_updates_cancellation_indices_and_preserves_fifo() {
        let (mut book, _rx) = test_book();

        for order_id in 1..=1_100 {
            book.match_limit_order(order(order_id, 100, 1, Side::Sell))
                .unwrap();
        }
        for order_id in 1..=1_024 {
            book.delete_order(order_id).unwrap();
        }

        let level = book.asks.get(&100).unwrap();
        assert_eq!(level.prices.len(), 76);
        assert_eq!(level.head, 0);

        // This location moved during compaction; cancellation must still hit it.
        book.delete_order(1_100).unwrap();

        let trades = book
            .match_limit_order(order(2_000, 100, 75, Side::Buy))
            .unwrap();
        let maker_ids: Vec<_> = trades.iter().map(|trade| trade.maker_order_id).collect();

        assert_eq!(maker_ids, (1_025..1_100).collect::<Vec<_>>());
        assert!(book.asks.is_empty());
        assert_eq!(book.stats().total_orders, 0);
    }

    #[test]
    fn rejects_an_active_duplicate_order_id_before_mutating_the_book() {
        let (mut book, _rx) = test_book();

        book.match_limit_order(order(7, 100, 10, Side::Buy))
            .unwrap();
        let result = book.match_limit_order(order(7, 200, 10, Side::Sell));

        assert!(matches!(result, Err(OrderBookError::InvalidOrder(_))));
        assert_eq!(book.stats().total_orders, 1);
        assert_eq!(book.stats().best_bid, Some(100));
        assert_eq!(book.stats().best_ask, None);
    }
}
