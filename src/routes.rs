use actix_web::{body, delete, get, post, web::Json, HttpResponse, Responder};

use crate::{inputs::{CreateOrderInput, DeleteOrder}, outputs::{CreateOrderResponse, DeleteOrderResponse, Depth}};

#[post("/order")]
pub async fn create_order(Json(body): Json<CreateOrderInput>) -> impl Responder {
    let price = body.price;
    let quantity = body.quantity;
    let user_id = body.user_id;
    let side = body.side;

    // maintain orderbook logic here

    return HttpResponse::Ok().json(CreateOrderResponse {
        order_id: String::from("ABC")
    });
}

#[delete("/order")]
pub async fn delete_order(Json(body): Json<DeleteOrder>) -> impl Responder {
    let order_id = body.order_id;
    HttpResponse::Ok().json(DeleteOrderResponse {
        filled_qty: 0,
        average_price: 100
    })
}

#[get("/depth")]
pub async fn get_depth() -> impl Responder{
    HttpResponse::Ok().json(Depth {
        bids: vec![],
        asks: vec![],
        lastUpdateId: String::from("ABCD")
    })
}