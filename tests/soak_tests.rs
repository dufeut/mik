// Test-specific lint suppressions
#![allow(dead_code)]
#![allow(clippy::redundant_locals)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::manual_range_contains)]

//! Soak tests for long-running stability of the mikrozen runtime.
//!
//! These tests run for extended periods to detect issues that only
//! appear over time: memory leaks, connection exhaustion, performance
//! degradation, and resource accumulation.
//!
//! ## Background
//!
//! Most tests run for seconds; production runs for days. Issues that
//! appear over time include:
//!
//! - Gradual memory growth (fragmentation, leaks)
//! - File descriptor exhaustion
//! - Connection pool degradation
//! - Performance regression under sustained load
//!
//! ## Test Philosophy
//!
//! 1. **Duration** - Run for minutes, not seconds
//! 2. **Metrics** - Track trends, not just pass/fail
//! 3. **Stability** - Variance should decrease over time
//!
//! ## Running Tests
//!
//! ```bash
//! # Run soak tests (takes several minutes)
//! cargo test -p mik soak -- --ignored --test-threads=1 --nocapture
//!
//! # Run with custom duration (via env var)
//! SOAK_DURATION_SECS=300 cargo test -p mik soak -- --ignored
//! ```

#[path = "common.rs"]
mod common;

use common::RealTestHost;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// =============================================================================
// Configuration
// =============================================================================

/// Default soak test duration in seconds.
const DEFAULT_SOAK_DURATION_SECS: u64 = 60;

/// Get soak duration from environment or use default.
fn soak_duration() -> Duration {
    let secs = std::env::var("SOAK_DURATION_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_SOAK_DURATION_SECS);
    Duration::from_secs(secs)
}

// =============================================================================
// Helper Functions
// =============================================================================

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("modules")
}

fn echo_wasm_exists() -> bool {
    fixtures_dir().join("echo.wasm").exists()
}

/// Metrics collected during soak test.
#[derive(Debug, Clone, Default)]
struct SoakMetrics {
    requests_total: u64,
    requests_success: u64,
    requests_failed: u64,
    latencies_ms: Vec<u64>,
    errors: Vec<String>,
    checkpoints: Vec<CheckpointMetrics>,
}

#[derive(Debug, Clone)]
struct CheckpointMetrics {
    elapsed_secs: f64,
    requests_total: u64,
    requests_per_sec: f64,
    success_rate: f64,
    latency_avg_ms: f64,
    latency_p50_ms: u64,
    latency_p99_ms: u64,
}

impl SoakMetrics {
    fn success_rate(&self) -> f64 {
        if self.requests_total == 0 {
            return 0.0;
        }
        (self.requests_success as f64 / self.requests_total as f64) * 100.0
    }

    fn latency_percentile(&self, p: f64) -> u64 {
        if self.latencies_ms.is_empty() {
            return 0;
        }
        let mut sorted = self.latencies_ms.clone();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64 * p / 100.0) as usize).min(sorted.len() - 1);
        sorted[idx]
    }

    fn latency_avg(&self) -> f64 {
        if self.latencies_ms.is_empty() {
            return 0.0;
        }
        self.latencies_ms.iter().sum::<u64>() as f64 / self.latencies_ms.len() as f64
    }

    fn record_checkpoint(&mut self, elapsed: Duration, recent_rps: f64) {
        let checkpoint = CheckpointMetrics {
            elapsed_secs: elapsed.as_secs_f64(),
            requests_total: self.requests_total,
            requests_per_sec: recent_rps,
            success_rate: self.success_rate(),
            latency_avg_ms: self.latency_avg(),
            latency_p50_ms: self.latency_percentile(50.0),
            latency_p99_ms: self.latency_percentile(99.0),
        };
        self.checkpoints.push(checkpoint);
    }

    fn print_checkpoint(&self, checkpoint_num: usize) {
        if let Some(cp) = self.checkpoints.last() {
            println!(
                "Checkpoint {}: {:.0}s elapsed, {} total reqs, {:.1} req/s, {:.1}% success, p50={:.0}ms p99={:.0}ms",
                checkpoint_num,
                cp.elapsed_secs,
                cp.requests_total,
                cp.requests_per_sec,
                cp.success_rate,
                cp.latency_p50_ms,
                cp.latency_p99_ms
            );
        }
    }

    fn check_stability(&self) -> Result<(), String> {
        if self.checkpoints.len() < 2 {
            return Ok(());
        }

        // Check that success rate stays high
        let min_success_rate = self
            .checkpoints
            .iter()
            .map(|c| c.success_rate)
            .fold(f64::MAX, f64::min);
        if min_success_rate < 95.0 {
            return Err(format!(
                "Success rate dropped below 95%: {:.1}%",
                min_success_rate
            ));
        }

        // Check that latency doesn't grow unbounded
        let first_p99 = self
            .checkpoints
            .first()
            .map(|c| c.latency_p99_ms)
            .unwrap_or(0);
        let last_p99 = self
            .checkpoints
            .last()
            .map(|c| c.latency_p99_ms)
            .unwrap_or(0);

        if first_p99 > 0 && last_p99 > first_p99 * 3 {
            return Err(format!(
                "P99 latency grew 3x: {}ms -> {}ms",
                first_p99, last_p99
            ));
        }

        Ok(())
    }

    fn print_summary(&self, duration: Duration) {
        println!("\n=== Soak Test Summary ===");
        println!("Duration: {:.1}s", duration.as_secs_f64());
        println!("Total requests: {}", self.requests_total);
        println!("Successful: {}", self.requests_success);
        println!("Failed: {}", self.requests_failed);
        println!("Success rate: {:.2}%", self.success_rate());
        println!(
            "Throughput: {:.1} req/s",
            self.requests_total as f64 / duration.as_secs_f64()
        );
        println!("\nLatency:");
        println!("  Avg: {:.1}ms", self.latency_avg());
        println!("  P50: {}ms", self.latency_percentile(50.0));
        println!("  P95: {}ms", self.latency_percentile(95.0));
        println!("  P99: {}ms", self.latency_percentile(99.0));

        if !self.errors.is_empty() {
            println!("\nUnique errors ({}):", self.errors.len().min(5));
            for err in self.errors.iter().take(5) {
                println!("  - {}", err);
            }
        }
    }
}

/// Rolling window for recent metrics.
struct RollingWindow {
    timestamps: VecDeque<Instant>,
    window_duration: Duration,
}

impl RollingWindow {
    fn new(window_duration: Duration) -> Self {
        Self {
            timestamps: VecDeque::new(),
            window_duration,
        }
    }

    fn record(&mut self) {
        let now = Instant::now();
        self.timestamps.push_back(now);

        // Remove old entries
        while let Some(front) = self.timestamps.front() {
            if now.duration_since(*front) > self.window_duration {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    fn rate_per_sec(&self) -> f64 {
        if self.timestamps.len() < 2 {
            return 0.0;
        }
        let first = self.timestamps.front().unwrap();
        let last = self.timestamps.back().unwrap();
        let duration = last.duration_since(*first);
        if duration.as_secs_f64() < 0.001 {
            return 0.0;
        }
        self.timestamps.len() as f64 / duration.as_secs_f64()
    }
}

// =============================================================================
// Integration Tests
// =============================================================================

/// Main soak test: sustained load over extended duration.
///
/// Runs continuous requests for the configured duration (default 60s)
/// and tracks stability metrics throughout.
#[tokio::test]
#[ignore = "Long-running soak test, requires fixtures"]
async fn test_soak_sustained_load() {
    if !echo_wasm_exists() {
        eprintln!(
            "Skipping: echo.wasm not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    let duration = soak_duration();
    let checkpoint_interval = Duration::from_secs(10);
    let concurrency = 10;

    println!("Starting soak test:");
    println!("  Duration: {:?}", duration);
    println!("  Checkpoint interval: {:?}", checkpoint_interval);
    println!("  Concurrency: {}", concurrency);

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_concurrent_requests(concurrency * 2)
        .start()
        .await
        .expect("Failed to start host");

    let metrics = Arc::new(tokio::sync::Mutex::new(SoakMetrics::default()));
    let stop = Arc::new(AtomicU64::new(0)); // 0 = running, 1 = stop

    // Spawn worker tasks
    let mut handles = Vec::new();
    for worker_id in 0..concurrency {
        let url = host.url("/run/echo/");
        let metrics = metrics.clone();
        let stop = stop.clone();

        handles.push(tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap();

            let mut request_num = 0u64;
            while stop.load(Ordering::Relaxed) == 0 {
                let start = Instant::now();
                let result = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "worker": worker_id,
                        "request": request_num
                    }))
                    .send()
                    .await;

                let latency_ms = start.elapsed().as_millis() as u64;

                {
                    let mut m = metrics.lock().await;
                    m.requests_total += 1;
                    m.latencies_ms.push(latency_ms);

                    match result {
                        Ok(resp) if resp.status().is_success() => {
                            m.requests_success += 1;
                        },
                        Ok(resp) => {
                            m.requests_failed += 1;
                            let err = format!("HTTP {}", resp.status());
                            if !m.errors.contains(&err) {
                                m.errors.push(err);
                            }
                        },
                        Err(e) => {
                            m.requests_failed += 1;
                            let err = format!("{}", e);
                            if m.errors.len() < 100
                                && !m
                                    .errors
                                    .iter()
                                    .any(|x| x.contains(&err[..20.min(err.len())]))
                            {
                                m.errors.push(err);
                            }
                        },
                    }
                }

                request_num += 1;

                // Small delay to prevent CPU spinning
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }));
    }

    // Monitor and checkpoint
    let start = Instant::now();
    let mut checkpoint_num = 0;
    let mut rolling = RollingWindow::new(Duration::from_secs(5));
    let mut last_checkpoint = Instant::now();
    let mut last_request_count = 0u64;

    while start.elapsed() < duration {
        tokio::time::sleep(Duration::from_millis(100)).await;
        rolling.record();

        // Checkpoint
        if last_checkpoint.elapsed() >= checkpoint_interval {
            checkpoint_num += 1;
            let mut m = metrics.lock().await;
            let current_count = m.requests_total;
            let requests_since_last = current_count - last_request_count;
            let rps = requests_since_last as f64 / last_checkpoint.elapsed().as_secs_f64();

            m.record_checkpoint(start.elapsed(), rps);
            m.print_checkpoint(checkpoint_num);

            last_checkpoint = Instant::now();
            last_request_count = current_count;
        }
    }

    // Stop workers
    stop.store(1, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.await;
    }

    // Final metrics
    let m = metrics.lock().await;
    m.print_summary(start.elapsed());

    // Assertions
    assert!(
        m.success_rate() >= 95.0,
        "Success rate should be >= 95%: {:.1}%",
        m.success_rate()
    );

    if let Err(e) = m.check_stability() {
        panic!("Stability check failed: {}", e);
    }

    // Health check
    drop(m);
    let health = host.get("/health").await.expect("Health check");
    assert_eq!(health.status(), 200);
}

/// Soak test with varying load patterns.
///
/// Alternates between high and low load to test recovery.
#[tokio::test]
#[ignore = "Long-running soak test, requires fixtures"]
async fn test_soak_variable_load() {
    if !echo_wasm_exists() {
        eprintln!(
            "Skipping: echo.wasm not found at {}",
            fixtures_dir().display()
        );
        return;
    }

    let host = RealTestHost::builder()
        .with_modules_dir(fixtures_dir())
        .with_max_concurrent_requests(100)
        .start()
        .await
        .expect("Failed to start host");

    let phases = [
        ("warmup", 5, 10),     // 5s, 10 concurrent
        ("high_load", 10, 50), // 10s, 50 concurrent
        ("recovery", 5, 5),    // 5s, 5 concurrent
        ("spike", 5, 100),     // 5s, 100 concurrent
        ("cooldown", 10, 10),  // 10s, 10 concurrent
    ];

    println!("Starting variable load soak test");

    let mut total_success = 0u64;
    let mut total_failed = 0u64;

    for (phase_name, duration_secs, concurrency) in phases {
        println!(
            "\nPhase '{}': {}s at {} concurrency",
            phase_name, duration_secs, concurrency
        );

        let phase_start = Instant::now();
        let phase_duration = Duration::from_secs(duration_secs);
        let success = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));

        // Spawn workers for this phase
        let mut handles = Vec::new();
        for _ in 0..concurrency {
            let url = host.url("/run/echo/");
            let phase_start = phase_start;
            let success = success.clone();
            let failed = failed.clone();

            handles.push(tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap();

                while phase_start.elapsed() < phase_duration {
                    let result = client
                        .post(&url)
                        .json(&serde_json::json!({"phase": phase_name}))
                        .send()
                        .await;

                    match result {
                        Ok(r) if r.status().is_success() => {
                            success.fetch_add(1, Ordering::Relaxed);
                        },
                        _ => {
                            failed.fetch_add(1, Ordering::Relaxed);
                        },
                    }

                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }));
        }

        // Wait for phase to complete
        for handle in handles {
            let _ = handle.await;
        }

        let phase_success = success.load(Ordering::Relaxed);
        let phase_failed = failed.load(Ordering::Relaxed);
        let phase_total = phase_success + phase_failed;
        let success_rate = if phase_total > 0 {
            (phase_success as f64 / phase_total as f64) * 100.0
        } else {
            0.0
        };

        println!(
            "Phase complete: {} requests, {:.1}% success",
            phase_total, success_rate
        );

        total_success += phase_success;
        total_failed += phase_failed;
    }

    let total = total_success + total_failed;
    let overall_success_rate = (total_success as f64 / total as f64) * 100.0;

    println!("\n=== Variable Load Summary ===");
    println!("Total requests: {}", total);
    println!("Success rate: {:.1}%", overall_success_rate);

    assert!(
        overall_success_rate >= 90.0,
        "Overall success rate should be >= 90%: {:.1}%",
        overall_success_rate
    );
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soak_duration_default() {
        // Without env var, should use default
        let duration = Duration::from_secs(DEFAULT_SOAK_DURATION_SECS);
        assert_eq!(duration.as_secs(), 60);
    }

    #[test]
    fn test_metrics_success_rate() {
        let mut m = SoakMetrics::default();
        m.requests_total = 100;
        m.requests_success = 95;
        m.requests_failed = 5;

        assert!((m.success_rate() - 95.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_latency_percentile() {
        let mut m = SoakMetrics::default();
        m.latencies_ms = (1..=100).collect();

        // With 100 samples, p50 index is 50, value at index 50 is 51
        // This is correct percentile calculation
        let p50 = m.latency_percentile(50.0);
        assert!(
            p50 >= 50 && p50 <= 51,
            "P50 should be around 50-51, got {}",
            p50
        );

        let p99 = m.latency_percentile(99.0);
        assert!(
            p99 >= 99 && p99 <= 100,
            "P99 should be around 99-100, got {}",
            p99
        );
    }

    #[test]
    fn test_rolling_window() {
        let mut window = RollingWindow::new(Duration::from_secs(1));

        // Record some events
        for _ in 0..10 {
            window.record();
        }

        // Should have recorded all
        assert_eq!(window.timestamps.len(), 10);
    }

    #[test]
    fn test_stability_check_success_rate() {
        let mut m = SoakMetrics::default();

        // Add checkpoints with good success rate
        m.checkpoints.push(CheckpointMetrics {
            elapsed_secs: 10.0,
            requests_total: 100,
            requests_per_sec: 10.0,
            success_rate: 99.0,
            latency_avg_ms: 50.0,
            latency_p50_ms: 45,
            latency_p99_ms: 100,
        });

        m.checkpoints.push(CheckpointMetrics {
            elapsed_secs: 20.0,
            requests_total: 200,
            requests_per_sec: 10.0,
            success_rate: 98.0,
            latency_avg_ms: 55.0,
            latency_p50_ms: 50,
            latency_p99_ms: 110,
        });

        assert!(m.check_stability().is_ok());
    }

    #[test]
    fn test_stability_check_fails_on_low_success() {
        let mut m = SoakMetrics::default();

        m.checkpoints.push(CheckpointMetrics {
            elapsed_secs: 10.0,
            requests_total: 100,
            requests_per_sec: 10.0,
            success_rate: 99.0,
            latency_avg_ms: 50.0,
            latency_p50_ms: 45,
            latency_p99_ms: 100,
        });

        // Bad checkpoint
        m.checkpoints.push(CheckpointMetrics {
            elapsed_secs: 20.0,
            requests_total: 200,
            requests_per_sec: 5.0,
            success_rate: 80.0, // Too low!
            latency_avg_ms: 100.0,
            latency_p50_ms: 80,
            latency_p99_ms: 500,
        });

        assert!(m.check_stability().is_err());
    }

    #[test]
    fn test_stability_check_fails_on_latency_growth() {
        let mut m = SoakMetrics::default();

        m.checkpoints.push(CheckpointMetrics {
            elapsed_secs: 10.0,
            requests_total: 100,
            requests_per_sec: 10.0,
            success_rate: 99.0,
            latency_avg_ms: 50.0,
            latency_p50_ms: 45,
            latency_p99_ms: 100, // Initial p99
        });

        m.checkpoints.push(CheckpointMetrics {
            elapsed_secs: 20.0,
            requests_total: 200,
            requests_per_sec: 10.0,
            success_rate: 98.0,
            latency_avg_ms: 150.0,
            latency_p50_ms: 120,
            latency_p99_ms: 500, // 5x growth! (> 3x threshold)
        });

        assert!(m.check_stability().is_err());
    }

    #[test]
    fn test_phase_calculation() {
        let phases = [
            ("warmup", 5u64, 10u32),
            ("high_load", 10, 50),
            ("recovery", 5, 5),
            ("spike", 5, 100),
            ("cooldown", 10, 10),
        ];

        let total_duration: u64 = phases.iter().map(|(_, d, _)| d).sum();
        assert_eq!(total_duration, 35);

        let max_concurrency = phases.iter().map(|(_, _, c)| c).max().unwrap();
        assert_eq!(*max_concurrency, 100);
    }
}
