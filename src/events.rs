use crate::inputs::Side;
use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone, Copy, SchemaWrite, SchemaRead)]
pub enum OrderEvent {
    NewOrder {
        order_id: u32,
        user_id: u32,
        price: u32,
        quantity: u32,
        side: Side,
    },
    DeleteOrder {
        order_id: u32,
    },
}

#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum MatchEvent {
    Trade {
        trade_id: [u8; 16],
        price: u32,
        quantity: u32,
        maker_order_id: u32,
        taker_order_id: u32,
        timestamp: i64,
    },
    DepthUpdate {
        bids: Vec<[u32; 2]>,
        asks: Vec<[u32; 2]>,
    },
}
