use reqwest::Client;
use rmp_serde;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{CpuRefreshKind, RefreshKind, System};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize)]
struct CreateOrderInput {
    price: u32,
    quantity: u32,
    user_id: u32,
    side: String,
}

#[derive(Default)]
struct Stats {
    latencies: Vec<f64>,
    total: usize,
    failed: usize,
}

impl Stats {
    fn record(&mut self, duration_ms: f64, ok: bool) {
        self.latencies.push(duration_ms);
        self.total += 1;
        if !ok {
            self.failed += 1;
        }
    }

    fn summarize(&mut self) {
        self.latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let total = self.latencies.len();
        if total == 0 {
            println!("No requests recorded.");
            return;
        }

        let avg: f64 = self.latencies.iter().sum::<f64>() / total as f64;
        let p50 = self.latencies[(0.5 * total as f64) as usize];
        let p95 = self.latencies[(0.95 * total as f64) as usize];
        let p99 = self.latencies[(0.99 * total as f64) as usize];

        println!("\n========== CLIENT-SIDE MEASUREMENTS ==========");
        println!("Total Requests:   {}", total);
        println!("Failed Requests:  {}", self.failed);
        println!("Average Latency:  {:.2} ms", avg);
        println!("P50:              {:.2} ms", p50);
        println!("P95:              {:.2} ms", p95);
        println!("P99:              {:.2} ms", p99);
        println!("==============================================\n");
    }
}

async fn monitor_cpu(stop: Arc<Mutex<bool>>) {
    let mut sys =
        System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()));
    let mut samples = vec![];

    loop {
        if *stop.lock().await {
            break;
        }

        sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        let cpus = sys.cpus();
        if cpus.is_empty() {
            continue;
        }

        let total_usage: f32 = cpus.iter().map(|c| c.cpu_usage()).sum();
        let avg = total_usage / cpus.len() as f32;
        samples.push(avg);

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    if !samples.is_empty() {
        let avg_cpu: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
        let peak = samples.iter().cloned().fold(0.0, f32::max);
        println!("\n========== CPU USAGE (Test Client) ==========");
        println!("Average CPU:  {:.1}%", avg_cpu);
        println!("Peak CPU:     {:.1}%", peak);
        println!("Samples:      {}", samples.len());
        println!("=============================================\n");
    }
}

async fn fetch_server_metrics(client: &Client) {
    match client.get("http://127.0.0.1:8080/metrics").send().await {
        Ok(resp) => {
            if let Ok(text) = resp.text().await {
                println!("\n========== SERVER-SIDE METRICS ==========");

                for line in text.lines() {
                    if line.starts_with("http_requests_total ") {
                        if let Some(val) = line.split_whitespace().nth(1) {
                            println!("HTTP Requests:        {}", val);
                        }
                    }
                    if line.starts_with("orders_matched_total ") {
                        if let Some(val) = line.split_whitespace().nth(1) {
                            println!("Orders Matched:       {}", val);
                        }
                    }
                    if line.starts_with("trades_executed_total ") {
                        if let Some(val) = line.split_whitespace().nth(1) {
                            println!("Trades Executed:      {}", val);
                        }
                    }
                    if line.contains("http_request_latency_ms_sum") {
                        if let Some(val) = line.split_whitespace().nth(1) {
                            if let Ok(num) = val.parse::<f64>() {
                                println!("HTTP Latency Sum:     {:.2} ms", num);
                            }
                        }
                    }
                    if line.contains("http_request_latency_ms_count") {
                        if let Some(val) = line.split_whitespace().nth(1) {
                            println!("HTTP Latency Count:   {}", val);
                        }
                    }
                    if line.contains("matching_engine_latency_ms_sum") {
                        if let Some(val) = line.split_whitespace().nth(1) {
                            if let Ok(num) = val.parse::<f64>() {
                                println!("Matching Latency Sum: {:.2} ms", num);
                            }
                        }
                    }
                }

                println!("=========================================\n");
            }
        }
        Err(_) => {
            println!("\nCould not fetch server metrics (is the server running?)\n");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn msgpack_stress_test_with_cpu() {
    let base_url = "http://127.0.0.1:8080";
    let total_requests = 500_000;
    let concurrency = 2_000;

    let client = Arc::new(
        Client::builder()
            .pool_max_idle_per_host(200)
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .unwrap(),
    );
    let stats = Arc::new(Mutex::new(Stats::default()));
    let stop = Arc::new(Mutex::new(false));

    println!("\n========== MessagePack Stress Test ==========");
    println!("Total Requests:  {}", total_requests);
    println!("Concurrency:     {}", concurrency);
    println!("=============================================\n");

    let stop_monitor = stop.clone();
    tokio::spawn(async move {
        monitor_cpu(stop_monitor).await;
    });

    let start_time = Instant::now();
    let mut handles = vec![];

    for i in 0..concurrency {
        let client = client.clone();
        let stats = stats.clone();

        handles.push(tokio::spawn(async move {
            for j in 0..(total_requests / concurrency) {
                let start_op = Instant::now();
                let side = if (i + j) % 2 == 0 { "Buy" } else { "Sell" };
                let price = 10000 + ((i * j) % 2000);
                let qty = 1 + ((i + j) % 20);
                let user_id = 1000 + (i % 1000);
                let input = CreateOrderInput {
                    price,
                    quantity: qty,
                    user_id,
                    side: side.to_string(),
                };
                let body = rmp_serde::to_vec(&input).unwrap();

                let ok = client
                    .post(format!("{}/order", base_url))
                    .header("Content-Type", "application/msgpack")
                    .header("Accept", "application/msgpack")
                    .body(body)
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false);

                let elapsed = start_op.elapsed().as_secs_f64() * 1000.0;
                stats.lock().await.record(elapsed, ok);
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    *stop.lock().await = true;

    let total_time = start_time.elapsed().as_secs_f64();
    let total_done = stats.lock().await.total;
    let rps = total_done as f64 / total_time;

    println!("\n========== TEST SUMMARY ==========");
    println!("Total Time:   {:.2}s", total_time);
    println!("Throughput:   {:.2} req/sec", rps);
    println!("==================================");

    stats.lock().await.summarize();
    fetch_server_metrics(&client).await;
}
