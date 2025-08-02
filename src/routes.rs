use std::sync::{Arc, Mutex};

use actix_web::{
    HttpResponse, Responder, delete, get, post,
    web::{self, Data, Json},
};

use crate::{
    inputs::{CreateOrderInput, DeleteOrder, Side},
    orderbook::{self, OrderBook},
    outputs::{CreateOrderResponse, DeleteOrderResponse, Depth},
};

#[post("/order")]
pub async fn create_order(
    Json(body): Json<CreateOrderInput>,
    orderbook: Data<Arc<Mutex<OrderBook>>>,
) -> impl Responder {
    let price = body.price;
    let quantity = body.quantity;
    let user_id = body.user_id;
    let side = body.side.clone();

    let mut ob = orderbook.lock().unwrap();
    let order_id = ob.create_order(price, quantity, user_id, side);

    HttpResponse::Ok().json(CreateOrderResponse {
        order_id: order_id.to_string(),
    })
}

#[delete("/order")]
pub async fn delete_order(
    Json(body): Json<DeleteOrder>,
    orderbook: Data<Arc<Mutex<OrderBook>>>,
) -> impl Responder {
    let order_id = body.order_id.parse::<u32>().unwrap_or(0);
    let mut ob = orderbook.lock().unwrap();

    let (filled_qty, avg_price) = ob.delete_order(order_id);
    HttpResponse::Ok().json(DeleteOrderResponse {
        filled_qty,
        average_price: avg_price,
    })
}

#[get("/depth")]
pub async fn get_depth(orderbook: Data<Arc<Mutex<OrderBook>>>) -> impl Responder {
    let ob = orderbook.lock().unwrap();
    let depth = ob.get_depth(10); // Top 10 levels
    HttpResponse::Ok().json(depth)
}

