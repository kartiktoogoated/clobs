use chrono::Utc;
use std::collections::{BTreeMap, HashMap};
use std::mem::MaybeUninit;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::inputs::Side;
use crate::outputs::Depth;
use crate::persist::PersistEvent;
use crate::worker::Broadcaster;

use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub struct Order {
    pub order_id: u32,
    pub user_id: u32,
    pub price: u32,
    pub quantity: u32,
    pub side: Side,
}

struct OrderLocation {
    side: Side,
    price: u32,
    index: usize,
}

pub struct PriceLevel {
    prices: Vec<u32>,
    users: Vec<u32>,
    quantities: Vec<u32>,
    tombstone: Vec<bool>,
    total_qty: u32,
}

impl PriceLevel {
    #[inline]
    fn new() -> Self {
        Self {
            prices: Vec::with_capacity(16),
            users: Vec::with_capacity(16),
            quantities: Vec::with_capacity(16),
            tombstone: Vec::with_capacity(16),
            total_qty: 0,
        }
    }

    #[inline]
    fn push(&mut self, order: &Order) {
        self.prices.push(order.order_id);
        self.users.push(order.user_id);
        self.quantities.push(order.quantity);
        self.tombstone.push(false);
        self.total_qty += order.quantity;
    }

    #[inline]
    unsafe fn remove_fast(&mut self, idx: usize) -> u32 {
        let qty = self.quantities[idx];
        if !self.tombstone[idx] {
            self.total_qty -= qty;
            self.tombstone[idx] = true;
        }
        qty
    }

    #[inline]
    unsafe fn reduce_qty(&mut self, idx: usize, new_qty: u32) {
        let old = self.quantities[idx];
        self.quantities[idx] = new_qty;
        self.total_qty = self.total_qty - old + new_qty;
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.total_qty == 0
    }
}

#[derive(SchemaWrite, SchemaRead)]
struct TradeMsg {
    msg_type: u8,
    price: u32,
    quantity: u32,
    maker_order_id: u32,
    taker_order_id: u32,
    timestamp: i64,
}

struct DepthCache {
    bids: [[u32; 2]; 20],
    asks: [[u32; 2]; 20],
    bid_count: usize,
    ask_count: usize,
    dirty: bool,
}

pub struct OrderBook {
    pub bids: BTreeMap<u32, PriceLevel>,
    pub asks: BTreeMap<u32, PriceLevel>,

    order_locations: HashMap<u32, OrderLocation>,
    depth_cache: DepthCache,

    trade_buf: [MaybeUninit<TradeMsg>; 64],
    trade_len: usize,

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

            trade_buf: unsafe { MaybeUninit::uninit().assume_init() },
            trade_len: 0,

            tx,
            broadcaster,
        }
    }

    pub fn match_limit_order(&mut self, mut taker: Order) {
        let timestamp = Utc::now().timestamp_millis();

        let book = match taker.side {
            Side::Buy => &mut self.asks,
            Side::Sell => &mut self.bids,
        };

        let mut prices_to_remove = Vec::with_capacity(8);

        let range: Box<dyn Iterator<Item = (&u32, &mut PriceLevel)>> = match taker.side {
            Side::Buy => Box::new(book.range_mut(..=taker.price)),
            Side::Sell => Box::new(book.range_mut(taker.price..).rev()),
        };

        for (&price, level) in range {
            if taker.quantity == 0 {
                break;
            }

            let mut idx = 0;

            while idx < level.prices.len() && taker.quantity > 0 {
                if level.tombstone[idx] {
                    idx += 1;
                    continue;
                }

                let maker_id = level.prices[idx];
                let maker_qty = level.quantities[idx];
                let traded = taker.quantity.min(maker_qty);

                taker.quantity -= traded;

                let new_maker_qty = maker_qty - traded;
                unsafe {
                    level.reduce_qty(idx, new_maker_qty);
                }

                crate::metrics::TRADES_EXECUTED.inc();

                if self.trade_len < 64 {
                    self.trade_buf[self.trade_len].write(TradeMsg {
                        msg_type: 1,
                        price,
                        quantity: traded,
                        maker_order_id: maker_id,
                        taker_order_id: taker.order_id,
                        timestamp,
                    });
                    self.trade_len += 1;
                }

                if new_maker_qty == 0 {
                    unsafe {
                        level.remove_fast(idx);
                    }
                } else {
                    idx += 1;
                }
            }

            if level.is_empty() {
                prices_to_remove.push(price);
            }
        }

        for price in prices_to_remove {
            book.remove(&price);
        }

        if taker.quantity > 0 {
            self.inserting_resting(taker);
        }

        self.flush_trades();
        self.depth_cache.dirty = true;
    }

    #[inline]
    fn inserting_resting(&mut self, order: Order) {
        let book = match order.side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        };

        let level = book.entry(order.price).or_insert_with(PriceLevel::new);
        let index = level.prices.len();

        level.push(&order);

        self.order_locations.insert(
            order.order_id,
            OrderLocation {
                side: order.side,
                price: order.price,
                index,
            },
        );

        let _ = self.tx.send(PersistEvent::NewOrder(order));
    }

    pub fn delete_order(&mut self, order_id: u32) {
        if let Some(loc) = self.order_locations.remove(&order_id) {
            let book = match loc.side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };

            if let Some(level) = book.get_mut(&loc.price) {
                unsafe {
                    level.remove_fast(loc.index);
                }

                if level.is_empty() {
                    book.remove(&loc.price);
                }
            }
        }
        self.depth_cache.dirty = true;
    }

    #[inline]
    fn flush_trades(&mut self) {
        if self.trade_len == 0 {
            return;
        }

        for i in 0..self.trade_len {
            let trade = unsafe { self.trade_buf[i].assume_init_read() };
            if let Ok(encoded) = wincode::serialize(&trade) {
                self.broadcaster.broadcast_bytes(&encoded);
            }

            let _ = self.tx.send(PersistEvent::TradeExecuted {
                trade_id: Uuid::new_v4().into_bytes(),
                price: trade.price,
                quantity: trade.quantity,
                maker_order_id: trade.maker_order_id,
                taker_order_id: trade.taker_order_id,
                timestamp: trade.timestamp,
            });
        }
        self.trade_len = 0;
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
}
