use actix_web::{
    HttpRequest, HttpResponse, Responder, delete, get, post,
    web::{self, Data},
};
use parking_lot::RwLock;
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::{
    ORDER_ID_COUNTER,
    events::OrderEvent,
    inputs::{CreateOrderInput, DeleteOrder},
    metrics::{HTTP_LATENCY_MS, HTTP_REQUESTS_TOTAL},
    msgpack::MsgPackResponse,
    outputs::{CreateOrderResponse, DeleteOrderResponse, Depth},
};

type OrderSender = Arc<mpsc::UnboundedSender<OrderEvent>>;

fn is_msgpack(req: &HttpRequest) -> bool {
    req.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("msgpack"))
        .unwrap_or(false)
}

fn wants_msgpack(req: &HttpRequest) -> bool {
    req.headers()
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("msgpack"))
        .unwrap_or(false)
}

#[post("/order")]
pub async fn create_order(
    req: HttpRequest,
    body: web::Bytes,
    sender: Data<OrderSender>,
) -> impl Responder {
    let start = Instant::now();
    HTTP_REQUESTS_TOTAL.inc();

    let input: CreateOrderInput = if is_msgpack(&req) {
        match rmp_serde::from_slice(&body) {
            Ok(data) => data,
            Err(e) => {
                return HttpResponse::BadRequest().body(format!("Invalid MessagePack: {}", e));
            }
        }
    } else {
        match serde_json::from_slice(&body) {
            Ok(data) => data,
            Err(e) => return HttpResponse::BadRequest().body(format!("Invalid JSON: {}", e)),
        }
    };

    let order_id = ORDER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    let event = OrderEvent::NewOrder {
        order_id,
        user_id: input.user_id,
        price: input.price,
        quantity: input.quantity,
        side: input.side,
    };

    match sender.send(event) {
        Ok(_) => {
            HTTP_LATENCY_MS.observe(start.elapsed().as_secs_f64() * 1000.0);
            let response = CreateOrderResponse {
                order_id: order_id.to_string(),
            };

            if is_msgpack(&req) {
                response.msgpack()
            } else {
                HttpResponse::Ok().json(response)
            }
        }
        Err(_) => HttpResponse::InternalServerError().body("Order processing unavailable"),
    }
}

#[delete("/order")]
pub async fn delete_order(
    req: HttpRequest,
    body: web::Bytes,
    sender: Data<OrderSender>,
) -> impl Responder {
    let start = Instant::now();
    HTTP_REQUESTS_TOTAL.inc();

    let input: DeleteOrder = if is_msgpack(&req) {
        match rmp_serde::from_slice(&body) {
            Ok(data) => data,
            Err(e) => {
                return HttpResponse::BadRequest().body(format!("Invalid MessagePack: {}", e));
            }
        }
    } else {
        match serde_json::from_slice(&body) {
            Ok(data) => data,
            Err(e) => return HttpResponse::BadRequest().body(format!("Invalid JSON: {}", e)),
        }
    };

    let order_id = input.order_id.parse::<u32>().unwrap_or(0);
    let event = OrderEvent::DeleteOrder { order_id };

    match sender.send(event) {
        Ok(_) => {
            HTTP_LATENCY_MS.observe(start.elapsed().as_secs_f64() * 1000.0);
            let response = DeleteOrderResponse {
                filled_qty: 0,
                average_price: 0,
            };

            if is_msgpack(&req) {
                response.msgpack()
            } else {
                HttpResponse::Accepted().json(response)
            }
        }
        Err(_) => HttpResponse::InternalServerError().body("Order processing unavailable"),
    }
}

#[get("/depth")]
pub async fn get_depth(req: HttpRequest, depth: Data<Arc<RwLock<Depth>>>) -> impl Responder {
    let start = Instant::now();
    HTTP_REQUESTS_TOTAL.inc();

    let d = depth.read();
    let response = Depth {
        bids: d.bids.clone(),
        asks: d.asks.clone(),
        lastUpdateId: d.lastUpdateId.clone(),
    };
    drop(d); // Release the lock before serialization

    HTTP_LATENCY_MS.observe(start.elapsed().as_secs_f64() * 1000.0);

    if wants_msgpack(&req) {
        // Serialize to MessagePack and return as binary response
        match rmp_serde::to_vec(&response) {
            Ok(bytes) => HttpResponse::Ok()
                .content_type("application/msgpack")
                .body(bytes),
            Err(e) => {
                eprintln!("Failed to serialize depth to MessagePack: {}", e);
                // Fallback to JSON on serialization error
                HttpResponse::Ok().json(response)
            }
        }
    } else {
        HttpResponse::Ok().json(response)
    }
}

#[get("/metrics")]
pub async fn metrics_endpoint() -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();

    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok()
        .content_type(encoder.format_type())
        .body(String::from_utf8(buffer).unwrap_or_default())
}
