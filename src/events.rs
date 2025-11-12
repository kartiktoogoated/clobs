use crate::inputs::Side;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchEvent {
    Trade {
        trade_id: Uuid,
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
