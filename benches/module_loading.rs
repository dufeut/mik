//! Module loading and AOT cache benchmarks.
//!
//! Benchmarks:
//! - AOT path computation
//! - Mtime comparison (cache validation)
//! - Module path resolution
//! - Composed module name extraction
//!
//! Run with:
//! ```bash
//! cargo bench -p mik --bench module_loading
//! ```
//!
//! For HTML reports:
//! ```bash
//! cargo bench -p mik --bench module_loading -- --verbose
//! open target/criterion/report/index.html
//! ```

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

fn module_loading_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("module_loading");
    group.measurement_time(Duration::from_secs(5));

    // Benchmark AOT path computation
    group.bench_function("aot_path_computation", |b| {
        b.iter(|| {
            let wasm_path = black_box(PathBuf::from("modules/my-service.wasm"));
            wasm_path.with_extension("wasm.aot")
        })
    });

    // Benchmark mtime comparison (cache validation)
    group.bench_function("mtime_check", |b| {
        b.iter(|| {
            // Simulate mtime comparison
            let source_time = black_box(std::time::SystemTime::UNIX_EPOCH);
            let cache_time = black_box(std::time::SystemTime::UNIX_EPOCH);
            source_time <= cache_time
        })
    });

    // Benchmark module directory scanning (path operations)
    group.bench_function("module_path_resolution", |b| {
        let base_dir = PathBuf::from("/app/modules");
        let module_name = "my-service";

        b.iter(|| {
            let module_path = base_dir.join(format!("{}.wasm", module_name));
            let aot_path = module_path.with_extension("wasm.aot");
            black_box((module_path, aot_path))
        })
    });

    // Benchmark composed module name extraction (strips -composed suffix)
    group.bench_function("composed_name_extraction", |b| {
        let filenames = [
            "my-service-composed.wasm",
            "auth-handler-composed.wasm",
            "simple-module.wasm",
            "api-gateway-v2-composed.wasm",
        ];

        b.iter(|| {
            for filename in &filenames {
                let name = filename
                    .strip_suffix(".wasm")
                    .unwrap_or(filename)
                    .strip_suffix("-composed")
                    .unwrap_or(filename.strip_suffix(".wasm").unwrap_or(filename));
                black_box(name);
            }
        })
    });

    // Benchmark path canonicalization simulation
    group.bench_function("path_normalization", |b| {
        let paths = [
            "/app/modules/../modules/service.wasm",
            "/app/./modules/service.wasm",
            "/app/modules/subdir/../service.wasm",
        ];

        b.iter(|| {
            for path in &paths {
                // Simulate path normalization without actual filesystem access
                let normalized: Vec<&str> = path
                    .split('/')
                    .filter(|s| !s.is_empty() && *s != ".")
                    .fold(Vec::new(), |mut acc, segment| {
                        if segment == ".." {
                            acc.pop();
                        } else {
                            acc.push(segment);
                        }
                        acc
                    });
                black_box(normalized);
            }
        })
    });

    // Benchmark module cache key generation
    for path_length in [1, 3, 5].iter() {
        let path = format!(
            "modules{}service.wasm",
            (0..*path_length)
                .map(|i| format!("/subdir{}", i))
                .collect::<String>()
        );
        group.bench_with_input(
            BenchmarkId::new("cache_key_generation", path_length),
            &path,
            |b, path| {
                b.iter(|| {
                    // Generate cache key from path
                    let key = path.replace(['/', '\\', '.'], "_");
                    black_box(key)
                })
            },
        );
    }

    // Benchmark file extension checks
    group.bench_function("extension_check", |b| {
        let files = [
            "module.wasm",
            "module.wasm.aot",
            "script.js",
            "config.toml",
            "module.wat",
        ];

        b.iter(|| {
            for file in &files {
                let is_wasm = file.ends_with(".wasm");
                let is_aot = file.ends_with(".wasm.aot");
                let is_script = file.ends_with(".js");
                black_box((is_wasm, is_aot, is_script));
            }
        })
    });

    // Benchmark metadata comparison (size + mtime)
    group.bench_function("metadata_comparison", |b| {
        #[derive(Clone, Copy)]
        #[allow(dead_code)]
        struct MockMetadata {
            size: u64,
            mtime: u64,
        }

        let source = MockMetadata {
            size: 1024 * 1024,
            mtime: 1700000000,
        };
        let cached = MockMetadata {
            size: 2048 * 1024,
            mtime: 1700000001,
        };

        b.iter(|| {
            let is_valid = cached.mtime >= source.mtime;
            let needs_recompile = source.mtime > cached.mtime;
            black_box((is_valid, needs_recompile))
        })
    });

    group.finish();
}

criterion_group!(benches, module_loading_benchmarks);
criterion_main!(benches);
