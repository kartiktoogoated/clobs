use crate::orderbook::Order;
use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum PersistEvent {
    NewOrder(Order),
    OrderFilled {
        order_id: u32,
        traded_qty: u64,
    },
    OrderDeleted {
        order_id: u32,
    },
    TradeExecuted {
        trade_id: [u8; 16],
        price: u64,
        quantity: u64,
        maker_order_id: u32,
        taker_order_id: u32,
        timestamp: i64,
    },
}
