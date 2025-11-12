use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderResponse {
    pub order_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteOrderResponse {
    pub filled_qty: u32,
    pub average_price: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Depth {
    pub bids: Vec<[u32; 2]>,
    pub asks: Vec<[u32; 2]>,
    pub lastUpdateId: String,
}
