//! Throughput benchmarks for concurrent request handling.
//!
//! Benchmarks:
//! - Concurrent hashmap operations (simulating module cache)
//! - Channel throughput (simulating request queuing)
//! - Concurrent counter operations (simulating metrics)
//!
//! Run with:
//! ```bash
//! cargo bench -p mik --bench throughput
//! ```
//!
//! For HTML reports:
//! ```bash
//! cargo bench -p mik --bench throughput -- --verbose
//! open target/criterion/report/index.html
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

fn throughput_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark concurrent hashmap operations (simulating module cache)
    for concurrency in [1, 4, 8, 16].iter() {
        group.throughput(Throughput::Elements(*concurrency as u64));
        group.bench_with_input(
            BenchmarkId::new("cache_operations", concurrency),
            concurrency,
            |b, &conc| {
                use std::collections::HashMap;
                let mut cache: HashMap<String, Vec<u8>> = HashMap::new();
                b.iter(|| {
                    for i in 0..conc {
                        cache.insert(format!("module_{}", i), vec![0u8; 100]);
                    }
                    for i in 0..conc {
                        let _ = cache.get(&format!("module_{}", i));
                    }
                })
            },
        );
    }

    // Benchmark concurrent DashMap operations (thread-safe cache simulation)
    group.sample_size(50);
    for num_threads in [2, 4, 8].iter() {
        group.throughput(Throughput::Elements((*num_threads * 100) as u64));
        group.bench_with_input(
            BenchmarkId::new("concurrent_cache", num_threads),
            num_threads,
            |b, &num_threads| {
                use std::collections::HashMap;
                use std::sync::RwLock;

                let cache: Arc<RwLock<HashMap<String, Vec<u8>>>> =
                    Arc::new(RwLock::new(HashMap::new()));

                // Pre-populate cache
                {
                    let mut c = cache.write().unwrap();
                    for i in 0..50 {
                        c.insert(format!("module_{}", i), vec![0u8; 100]);
                    }
                }

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|t| {
                            let cache = Arc::clone(&cache);
                            std::thread::spawn(move || {
                                for i in 0..100 {
                                    let key = format!("module_{}", (t * 100 + i) % 50);
                                    // Read-heavy workload (90% reads)
                                    if i % 10 == 0 {
                                        let mut c = cache.write().unwrap();
                                        c.insert(key.clone(), vec![0u8; 100]);
                                    } else {
                                        let c = cache.read().unwrap();
                                        black_box(c.get(&key));
                                    }
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    // Benchmark channel throughput (simulating request queuing)
    group.sample_size(100);
    for queue_size in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*queue_size as u64));
        group.bench_with_input(
            BenchmarkId::new("channel_throughput", queue_size),
            queue_size,
            |b, &queue_size| {
                use std::sync::mpsc;

                b.iter(|| {
                    let (tx, rx) = mpsc::channel::<u64>();

                    let producer = std::thread::spawn(move || {
                        for i in 0..queue_size as u64 {
                            tx.send(i).unwrap();
                        }
                    });

                    let consumer = std::thread::spawn(move || {
                        let mut count = 0u64;
                        for _ in 0..queue_size {
                            count += rx.recv().unwrap();
                        }
                        black_box(count)
                    });

                    producer.join().unwrap();
                    consumer.join().unwrap();
                });
            },
        );
    }

    // Benchmark atomic counter throughput
    group.sample_size(50);
    for num_threads in [1, 2, 4, 8].iter() {
        let ops_per_thread = 1000;
        group.throughput(Throughput::Elements((*num_threads * ops_per_thread) as u64));
        group.bench_with_input(
            BenchmarkId::new("atomic_counter", num_threads),
            num_threads,
            |b, &num_threads| {
                use std::sync::atomic::{AtomicU64, Ordering};

                let counter = Arc::new(AtomicU64::new(0));

                b.iter(|| {
                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let counter = Arc::clone(&counter);
                            std::thread::spawn(move || {
                                for _ in 0..ops_per_thread {
                                    counter.fetch_add(1, Ordering::Relaxed);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }

                    counter.store(0, Ordering::Relaxed);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, throughput_benchmarks);
criterion_main!(benches);
