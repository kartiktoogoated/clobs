use actix_web::{HttpResponse, Responder, get};
use lazy_static::lazy_static;
use prometheus::{
    Encoder, Histogram, IntCounter, TextEncoder, register_histogram, register_int_counter,
};
use prometheus::{IntGauge, register_int_gauge};
use std::thread;
use std::time::Duration;

lazy_static! {
    pub static ref HTTP_REQUESTS_TOTAL: IntCounter =
        register_int_counter!("http_requests_total", "Total HTTP requests received")
            .expect("failed to register HTTP_REQUESTS_TOTAL");
    pub static ref HTTP_LATENCY_MS: Histogram = register_histogram!(
        "http_request_latency_ms",
        "HTTP request end-to-end latency (ms)"
    )
    .expect("failed to register HTTP_LATENCY_MS");
    pub static ref ORDERS_MATCHED_TOTAL: IntCounter = register_int_counter!(
        "orders_matched_total",
        "Orders processed by matching engine"
    )
    .expect("failed to register ORDERS_MATCHED_TOTAL");
    pub static ref MATCHING_LATENCY_MS: Histogram = register_histogram!(
        "matching_engine_latency_ms",
        "Order matching engine latency (ms)"
    )
    .expect("failed to register MATCHING_LATENCY_MS");
    pub static ref DEPTH_UPDATES: IntCounter =
        register_int_counter!("depth_broadcasts_total", "Number of depth broadcasts")
            .expect("failed to register DEPTH_UPDATES");
    pub static ref TRADES_EXECUTED: IntCounter =
        register_int_counter!("trades_executed_total", "Total trades executed")
            .expect("failed to register TRADES_EXECUTED");
    pub static ref CHANNEL_BUFFER_SIZE: IntGauge = register_int_gauge!(
        "order_channel_buffer_size",
        "Current orders in channel buffer"
    )
    .expect("failed to register CHANNEL_BUFFER_SIZE");
}

#[get("/metrics")]
pub async fn metrics_endpoint() -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::with_capacity(8192);

    if encoder.encode(&metric_families, &mut buffer).is_err() {
        return HttpResponse::InternalServerError().body("Failed to encode Prometheus metrics");
    }

    HttpResponse::Ok()
        .content_type(encoder.format_type())
        .body(buffer)
}

pub fn start_console_metrics_printer() {
    let http_count = HTTP_REQUESTS_TOTAL.clone();
    let orders_matched = ORDERS_MATCHED_TOTAL.clone();
    let trades = TRADES_EXECUTED.clone();
    let depth = DEPTH_UPDATES.clone();
    let http_latency = HTTP_LATENCY_MS.clone();
    let matching_latency = MATCHING_LATENCY_MS.clone();

    thread::spawn(move || {
        loop {
            let http_samples = http_latency.get_sample_count();
            let avg_http_latency = if http_samples > 0 {
                http_latency.get_sample_sum() / http_samples as f64
            } else {
                0.0
            };

            let matching_samples = matching_latency.get_sample_count();
            let avg_matching_latency = if matching_samples > 0 {
                matching_latency.get_sample_sum() / matching_samples as f64
            } else {
                0.0
            };

            println!(
                "[Metrics] http_reqs={} orders_matched={} trades={} depth_updates={} | http_lat={:.3}ms matching_lat={:.3}ms",
                http_count.get(),
                orders_matched.get(),
                trades.get(),
                depth.get(),
                avg_http_latency,
                avg_matching_latency * 1000.0,
            );

            thread::sleep(Duration::from_secs(5));
        }
    });
}
