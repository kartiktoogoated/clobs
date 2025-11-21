use actix_web::{HttpResponse, Responder, get};
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use prometheus::{
    Encoder, Histogram, IntCounter, IntGauge, TextEncoder, register_histogram,
    register_int_counter, register_int_gauge,
};
use std::sync::atomic::Ordering;
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
    pub static ref HTTP_RTT_MS: Histogram =
        register_histogram!("http_rtt_ms", "Full HTTP round trip latency (ms)")
            .expect("failed to register HTTP_RTT_MS");
    pub static ref ORDER_PROCESSING_LATENCY_MS: Histogram = register_histogram!(
        "order_processing_latency_ms",
        "Full end-to-end order processing latency (ms)",
        vec![
            0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        ]
    )
    .expect("failed to register ORDER_PROCESSING_LATENCY_MS");
    pub static ref PERSISTENCE_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
        register_int_counter!("persistence_failures_total", "Total persistence failures").unwrap()
    });
    pub static ref BROADCAST_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
        register_int_counter!("broadcast_failures_total", "Total broadcast failures").unwrap()
    });
    pub static ref SERIALIZATION_FAILURES: Lazy<IntCounter> = Lazy::new(|| {
        register_int_counter!(
            "serialization_failures_total",
            "Total serialization failures"
        )
        .unwrap()
    });
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
    let buffer = CHANNEL_BUFFER_SIZE.clone();

    thread::spawn(move || {
        let mut prev_http_count = 0u64;
        let mut prev_orders = 0u64;
        let mut prev_lat_sum = 0u64;
        let mut prev_lat_count = 0u64;

        loop {
            thread::sleep(Duration::from_secs(5));

            let current_http = http_count.get();
            let current_orders = orders_matched.get();
            let current_trades = trades.get();
            let current_depth = depth.get();
            let current_buffer = buffer.get();

            let delta_http = current_http.saturating_sub(prev_http_count);
            let delta_orders = current_orders.saturating_sub(prev_orders);
            let throughput_per_sec = delta_http as f64 / 5.0;

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

            use crate::middleware::latency::{REAL_LAT_COUNT, REAL_LAT_SUM_US};

            let current_lat_count = REAL_LAT_COUNT.load(Ordering::Relaxed);
            let current_lat_sum = REAL_LAT_SUM_US.load(Ordering::Relaxed);

            let delta_lat_count = current_lat_count.saturating_sub(prev_lat_count);
            let delta_lat_sum = current_lat_sum.saturating_sub(prev_lat_sum);

            let realtime_latency_ms = if delta_lat_count > 0 {
                (delta_lat_sum as f64 / delta_lat_count as f64) / 1000.0
            } else {
                0.0
            };

            println!("\n[Metrics - Last 5s]");
            println!(
                "HTTP Requests: {} | Throughput: {:.0} req/s | Delta Orders: {}",
                current_http, throughput_per_sec, delta_orders
            );
            println!(
                "Trades: {} | Buffer: {} | Depth Updates: {}",
                current_trades, current_buffer, current_depth
            );
            println!(
                "Latency (5s window): HTTP {:.3}ms ({:.1}μs)",
                realtime_latency_ms,
                realtime_latency_ms * 1000.0
            );
            println!(
                "Latency (cumulative): HTTP {:.3}ms | Matching {:.3}ms",
                avg_http_latency, avg_matching_latency
            );

            prev_http_count = current_http;
            prev_orders = current_orders;
            prev_lat_sum = current_lat_sum;
            prev_lat_count = current_lat_count;
        }
    });
}
