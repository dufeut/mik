//! mik-bench: High-performance HTTP benchmark tool for mikrozen
//!
//! Features:
//! - Pre-allocated static requests (no dynamic content generation)
//! - Connection pooling with keep-alive
//! - Async concurrent requests using tokio
//! - HDR histogram for accurate latency percentiles
//! - Warmup phase to stabilize connections

use clap::Parser;
use hdrhistogram::Histogram;
use reqwest::Client;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

#[derive(Parser, Debug)]
#[command(name = "mik-bench")]
#[command(about = "High-performance HTTP benchmark for mikrozen")]
struct Args {
    /// Target URL to benchmark
    #[arg(default_value = "http://127.0.0.1:3000/health")]
    url: String,

    /// Number of concurrent connections
    #[arg(short, long, default_value = "50")]
    connections: usize,

    /// Duration in seconds
    #[arg(short, long, default_value = "10")]
    duration: u64,

    /// HTTP method (GET or POST)
    #[arg(short, long, default_value = "GET")]
    method: String,

    /// Request body for POST (static, pre-allocated)
    #[arg(short, long)]
    body: Option<String>,

    /// Warmup duration in seconds
    #[arg(short, long, default_value = "2")]
    warmup: u64,

    /// Disable keep-alive (new connection per request)
    #[arg(long)]
    no_keepalive: bool,
}

#[derive(Default)]
struct Stats {
    success: AtomicU64,
    errors: AtomicU64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║              mik-bench v0.1.0                            ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ URL:         {:<44} ║", truncate(&args.url, 44));
    println!("║ Method:      {:<44} ║", &args.method);
    println!("║ Connections: {:<44} ║", args.connections);
    println!("║ Duration:    {:<44} ║", format!("{}s", args.duration));
    println!("║ Warmup:      {:<44} ║", format!("{}s", args.warmup));
    println!("║ Keep-Alive:  {:<44} ║", if args.no_keepalive { "disabled" } else { "enabled" });
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // Build HTTP client with connection pooling
    let client = Client::builder()
        .pool_max_idle_per_host(args.connections)
        .pool_idle_timeout(Duration::from_secs(30))
        .tcp_keepalive(if args.no_keepalive { None } else { Some(Duration::from_secs(60)) })
        .timeout(Duration::from_secs(30))
        .build()?;

    // Pre-allocate static request data
    let url: Arc<str> = args.url.clone().into();
    let method = args.method.to_uppercase();
    let body: Option<Arc<str>> = args.body.map(|b| b.into());

    let stats = Arc::new(Stats::default());
    let running = Arc::new(AtomicBool::new(true));
    let latencies = Arc::new(parking_lot::Mutex::new(
        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap()
    ));

    // Warmup phase
    if args.warmup > 0 {
        println!("Warming up for {}s...", args.warmup);
        let warmup_barrier = Arc::new(Barrier::new(args.connections + 1));

        for _ in 0..args.connections {
            let client = client.clone();
            let url = url.clone();
            let method = method.clone();
            let body = body.clone();
            let barrier = warmup_barrier.clone();

            tokio::spawn(async move {
                barrier.wait().await;
                for _ in 0..10 {
                    let _ = send_request(&client, &url, &method, body.as_deref()).await;
                }
            });
        }

        warmup_barrier.wait().await;
        tokio::time::sleep(Duration::from_secs(args.warmup)).await;
        println!("Warmup complete.\n");
    }

    // Main benchmark
    println!("Running benchmark for {}s with {} connections...", args.duration, args.connections);

    let start_barrier = Arc::new(Barrier::new(args.connections + 1));
    let mut handles = Vec::with_capacity(args.connections);

    for _ in 0..args.connections {
        let client = client.clone();
        let url = url.clone();
        let method = method.clone();
        let body = body.clone();
        let stats = stats.clone();
        let running = running.clone();
        let latencies = latencies.clone();
        let barrier = start_barrier.clone();

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            while running.load(Ordering::Relaxed) {
                let start = Instant::now();
                let result = send_request(&client, &url, &method, body.as_deref()).await;
                let elapsed = start.elapsed();

                if result.is_ok() {
                    stats.success.fetch_add(1, Ordering::Relaxed);
                    let micros = elapsed.as_micros() as u64;
                    if let Some(mut hist) = latencies.try_lock() {
                        let _ = hist.record(micros);
                    }
                } else {
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    // Wait for all workers to be ready
    start_barrier.wait().await;
    let bench_start = Instant::now();

    // Run for specified duration
    tokio::time::sleep(Duration::from_secs(args.duration)).await;
    running.store(false, Ordering::Relaxed);

    // Wait for all workers to finish
    for handle in handles {
        let _ = handle.await;
    }

    let total_time = bench_start.elapsed();
    let success = stats.success.load(Ordering::Relaxed);
    let errors = stats.errors.load(Ordering::Relaxed);
    let total = success + errors;
    let rps = success as f64 / total_time.as_secs_f64();

    // Calculate latency stats
    let hist = latencies.lock();

    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║                      RESULTS                             ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Total Requests:    {:<38} ║", format_number(total));
    println!("║ Successful:        {:<38} ║", format_number(success));
    println!("║ Failed:            {:<38} ║", format_number(errors));
    println!("║ Duration:          {:<38} ║", format!("{:.2}s", total_time.as_secs_f64()));
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Requests/sec:      {:<38} ║", format!("{:.2}", rps));
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Latency (μs):                                            ║");
    println!("║   Min:             {:<38} ║", format_number(hist.min()));
    println!("║   Avg:             {:<38} ║", format!("{:.0}", hist.mean()));
    println!("║   Max:             {:<38} ║", format_number(hist.max()));
    println!("║   P50:             {:<38} ║", format_number(hist.value_at_percentile(50.0)));
    println!("║   P90:             {:<38} ║", format_number(hist.value_at_percentile(90.0)));
    println!("║   P99:             {:<38} ║", format_number(hist.value_at_percentile(99.0)));
    println!("║   P99.9:           {:<38} ║", format_number(hist.value_at_percentile(99.9)));
    println!("╚══════════════════════════════════════════════════════════╝");

    // Summary line for easy parsing
    println!();
    println!("Summary: {:.2} req/s, {:.2}ms avg, {:.2}ms p99",
        rps,
        hist.mean() / 1000.0,
        hist.value_at_percentile(99.0) as f64 / 1000.0
    );

    Ok(())
}

async fn send_request(
    client: &Client,
    url: &str,
    method: &str,
    body: Option<&str>,
) -> Result<(), reqwest::Error> {
    let request = match method {
        "POST" => {
            let mut req = client.post(url);
            if let Some(b) = body {
                req = req.body(b.to_string()).header("Content-Type", "application/json");
            }
            req
        }
        "PUT" => {
            let mut req = client.put(url);
            if let Some(b) = body {
                req = req.body(b.to_string()).header("Content-Type", "application/json");
            }
            req
        }
        "DELETE" => client.delete(url),
        _ => client.get(url), // Default to GET
    };

    request.send().await?.error_for_status()?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max-3])
    }
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
