use serde::{Deserialize, Serialize};
use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct CreateOrderResponse {
    pub order_id: String,
}

#[derive(Debug, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct DeleteOrderResponse {
    pub filled_qty: u32,
    pub average_price: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct Depth {
    pub bids: Vec<[u32; 2]>,
    pub asks: Vec<[u32; 2]>,
    pub last_update_id: String,
}
