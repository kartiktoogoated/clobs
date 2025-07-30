use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct CreateOrderInput {
    pub price: u32,
    pub quantity: u32,
    pub user_id: u32,
    pub side: Side
}

#[derive(Deserialize, Debug)]
pub enum Side {
    Buy,
    Sell
}

#[derive(Serialize, Deserialize)]
pub struct DeleteOrder {
    pub order_id: String
}