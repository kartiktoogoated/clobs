use crate::orderbook::Order;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum PersistEvent {
    NewOrder(Order),
    OrderFilled {
        order_id: u32,
        traded_qty: u32,
    },
    OrderDeleted {
        order_id: u32,
    },
    TradeExecuted {
        trade_id: Uuid,
        price: u32,
        quantity: u32,
        maker_order_id: u32,
        taker_order_id: u32,
        timestamp: i64,
    },
}
