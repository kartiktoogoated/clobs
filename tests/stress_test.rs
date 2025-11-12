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

        println!("================= MessagePack Load Test =================");
        println!("Total Requests: {}", total);
        println!("Failed Requests: {}", self.failed);
        println!("Average Latency: {:.2} ms", avg);
        println!(
            "P50: {:.2} ms | P95: {:.2} ms | P99: {:.2} ms",
            p50, p95, p99
        );
        println!("========================================================");
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
        println!("================= CPU Usage Stats =================");
        println!(
            "Average CPU Utilization: {:.1}% | Peak: {:.1}%",
            avg_cpu, peak
        );
        println!("Samples: {}", samples.len());
        println!("=====================================================");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn msgpack_stress_test_with_cpu() {
    let base_url = "http://127.0.0.1:8080";
    let total_requests = 500_000;
    let concurrency = 2_000;

    let client = Arc::new(Client::new());
    let stats = Arc::new(Mutex::new(Stats::default()));
    let stop = Arc::new(Mutex::new(false));

    println!(
        "MessagePack Stress Test: {} total requests | {} concurrency",
        total_requests, concurrency
    );

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

    println!("=====================================================");
    println!("Total time: {:.2}s", total_time);
    println!("Throughput: {:.2} req/sec", rps);
    println!("=====================================================");

    stats.lock().await.summarize();
}
