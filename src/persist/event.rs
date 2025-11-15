use crate::orderbook::Order;
use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
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
        trade_id: [u8; 16],
        price: u32,
        quantity: u32,
        maker_order_id: u32,
        taker_order_id: u32,
        timestamp: i64,
    },
}
