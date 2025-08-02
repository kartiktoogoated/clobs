use crate::orderbook::Order;

#[derive(Debug, Clone)]
pub enum PersistEvent {
    NewOrder(Order),
    OrderFilled { order_id: u32, traded_qty: u32 },
    OrderDeleted { order_id: u32 },
}

