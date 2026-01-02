//! HTTP request latency benchmarks measuring P50, P95, P99.
//!
//! Benchmarks:
//! - Path parsing overhead (routing hot path)
//! - Module name extraction from request paths
//!
//! Run with:
//! ```bash
//! cargo bench -p mik --bench http_latency
//! ```
//!
//! For HTML reports:
//! ```bash
//! cargo bench -p mik --bench http_latency -- --verbose
//! open target/criterion/report/index.html
//! ```

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

fn http_request_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("http_latency");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    // Benchmark cold start vs warm request would require runtime setup
    // For now, benchmark the path parsing overhead
    group.bench_function("path_parsing", |b| {
        b.iter(|| {
            let path = black_box("/run/my-module/api/v1/users");
            path.strip_prefix("/run/").and_then(|p| p.split('/').next())
        })
    });

    group.bench_function("module_name_extraction", |b| {
        b.iter(|| {
            let paths = [
                "/run/auth/login",
                "/run/api-gateway/v2/resource",
                "/run/simple/",
            ];
            for path in paths {
                black_box(path.strip_prefix("/run/").and_then(|p| p.split('/').next()));
            }
        })
    });

    // Benchmark path matching with different path depths
    for depth in [1, 3, 5, 10].iter() {
        let path = format!(
            "/run/module{}",
            (0..*depth)
                .map(|i| format!("/segment{}", i))
                .collect::<String>()
        );
        group.bench_with_input(BenchmarkId::new("path_depth", depth), &path, |b, path| {
            b.iter(|| black_box(path.strip_prefix("/run/").and_then(|p| p.split('/').next())))
        });
    }

    // Benchmark header parsing simulation
    group.bench_function("header_key_lookup", |b| {
        use std::collections::HashMap;
        let headers: HashMap<&str, &str> = [
            ("content-type", "application/json"),
            ("authorization", "Bearer token123"),
            ("x-request-id", "abc-123-def"),
            ("accept", "application/json"),
            ("user-agent", "mik-client/1.0"),
        ]
        .into_iter()
        .collect();

        b.iter(|| {
            black_box(headers.get("content-type"));
            black_box(headers.get("authorization"));
            black_box(headers.get("x-request-id"));
        })
    });

    // Benchmark URL query string parsing
    group.bench_function("query_string_parse", |b| {
        let query = "page=1&limit=50&sort=created_at&order=desc&filter=active";
        b.iter(|| {
            let params: Vec<(&str, &str)> = query
                .split('&')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    Some((parts.next()?, parts.next()?))
                })
                .collect();
            black_box(params)
        })
    });

    group.finish();
}

criterion_group!(benches, http_request_latency);
criterion_main!(benches);
