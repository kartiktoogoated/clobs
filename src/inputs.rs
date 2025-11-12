use serde::{Deserialize, Serialize};
use wincode_derive::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateOrderInput {
    pub price: u32,
    pub quantity: u32,
    pub user_id: u32,
    pub side: Side,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteOrder {
    pub order_id: String,
}
