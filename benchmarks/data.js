window.BENCHMARK_DATA = {
  "lastUpdate": 1767416314755,
  "repoUrl": "https://github.com/dufeutech/mik",
  "entries": {
    "Rust Benchmarks": [
      {
        "commit": {
          "author": {
            "email": "23062270+hlop3z@users.noreply.github.com",
            "name": "hlop3z",
            "username": "hlop3z"
          },
          "committer": {
            "email": "23062270+hlop3z@users.noreply.github.com",
            "name": "hlop3z",
            "username": "hlop3z"
          },
          "distinct": true,
          "id": "12920661b0ea84f206faedac9c88039f8ee07bc3",
          "message": "fix(ci): explicit benchmark data path to avoid conflicts\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>",
          "timestamp": "2026-01-02T07:00:09-06:00",
          "tree_id": "9883123381149ecbc5e9a8f45fc6b1fcb1200a58",
          "url": "https://github.com/dufeut/mik/commit/12920661b0ea84f206faedac9c88039f8ee07bc3"
        },
        "date": 1767360155912,
        "tool": "cargo",
        "benches": [
          {
            "name": "circuit_breaker/check_request_closed",
            "value": 357,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_and_record_success",
            "value": 1387,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/10",
            "value": 3903,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/100",
            "value": 39302,
            "range": "± 267",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/1000",
            "value": 404925,
            "range": "± 3497",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_hit",
            "value": 107,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_miss",
            "value": 88,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_insert",
            "value": 4455,
            "range": "± 1789",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_with_eviction",
            "value": 2833,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/runtime_create",
            "value": 206348,
            "range": "± 958",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_simple",
            "value": 2121,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_json_transform",
            "value": 8478,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_function_call",
            "value": 4065,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/script_preprocess",
            "value": 515,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent_access/circuit_breaker_concurrent",
            "value": 421057,
            "range": "± 11048",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent_access/module_cache_concurrent",
            "value": 156197,
            "range": "± 1699",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_single",
            "value": 10,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/10",
            "value": 111,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/100",
            "value": 1106,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/1000",
            "value": 11719,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_exhausted",
            "value": 148,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/1024",
            "value": 11,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/8192",
            "value": 11,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/65536",
            "value": 13,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "store_pool/placeholder_acquire_release",
            "value": 10,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_next",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/2",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/4",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/8",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/16",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/32",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/counter_increment",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/request_lifecycle",
            "value": 14,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/counter_concurrent",
            "value": 129300,
            "range": "± 1673",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent/buffer_pool_contention",
            "value": 130637,
            "range": "± 1813",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent/mixed_workload_read_heavy",
            "value": 128075,
            "range": "± 1583",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "23062270+hlop3z@users.noreply.github.com",
            "name": "hlop3z",
            "username": "hlop3z"
          },
          "committer": {
            "email": "23062270+hlop3z@users.noreply.github.com",
            "name": "hlop3z",
            "username": "hlop3z"
          },
          "distinct": true,
          "id": "a14bc0478157d32feee039d166849f8acc968688",
          "message": "ci: fix benchmark git conflict and add caching improvements\n\n- Clean working directory before benchmark action (fixes Cargo.lock conflict)\n- Add apt package caching for faster Linux builds\n- Add shared cache keys for Rust dependencies across jobs\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>",
          "timestamp": "2026-01-02T17:44:35-06:00",
          "tree_id": "6815a6f884b1386c51ec0d863fe7a993f4e3626b",
          "url": "https://github.com/dufeut/mik/commit/a14bc0478157d32feee039d166849f8acc968688"
        },
        "date": 1767398873455,
        "tool": "cargo",
        "benches": [
          {
            "name": "circuit_breaker/check_request_closed",
            "value": 362,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_and_record_success",
            "value": 1427,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/10",
            "value": 3951,
            "range": "± 40",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/100",
            "value": 39951,
            "range": "± 4829",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/1000",
            "value": 409841,
            "range": "± 3455",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_hit",
            "value": 105,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_miss",
            "value": 86,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_insert",
            "value": 4092,
            "range": "± 2963",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_with_eviction",
            "value": 2786,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/runtime_create",
            "value": 207029,
            "range": "± 1005",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_simple",
            "value": 2219,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_json_transform",
            "value": 8449,
            "range": "± 113",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_function_call",
            "value": 4017,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/script_preprocess",
            "value": 505,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent_access/circuit_breaker_concurrent",
            "value": 382800,
            "range": "± 19659",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent_access/module_cache_concurrent",
            "value": 159796,
            "range": "± 2389",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_single",
            "value": 10,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/10",
            "value": 111,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/100",
            "value": 1105,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/1000",
            "value": 11720,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_exhausted",
            "value": 150,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/1024",
            "value": 11,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/8192",
            "value": 11,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/65536",
            "value": 10,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "store_pool/placeholder_acquire_release",
            "value": 10,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_next",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/2",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/4",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/8",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/16",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/32",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/counter_increment",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/request_lifecycle",
            "value": 14,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/counter_concurrent",
            "value": 122224,
            "range": "± 896",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent/buffer_pool_contention",
            "value": 125150,
            "range": "± 1616",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent/mixed_workload_read_heavy",
            "value": 121738,
            "range": "± 833",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "23062270+hlop3z@users.noreply.github.com",
            "name": "hlop3z",
            "username": "hlop3z"
          },
          "committer": {
            "email": "23062270+hlop3z@users.noreply.github.com",
            "name": "hlop3z",
            "username": "hlop3z"
          },
          "distinct": true,
          "id": "8f69c82afa42ac35a7d75f51d91560d277230798",
          "message": "chore: update GitHub org from dufeut to dufeutech\n\nUpdate all references to the new organization URL across:\n- Repository URLs (Cargo.toml, install scripts)\n- Documentation links (README, docs site)\n- OCI registry references (ghcr.io)\n- Example Cargo.toml files (mik-sdk dependency)\n- Related project links\n- systemd service documentation\n\n🤖 Generated with [Claude Code](https://claude.com/claude-code)\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>",
          "timestamp": "2026-01-02T22:38:13-06:00",
          "tree_id": "012d3fef9df8f0b4f8b51fad8de00920f8b1405d",
          "url": "https://github.com/dufeutech/mik/commit/8f69c82afa42ac35a7d75f51d91560d277230798"
        },
        "date": 1767416314329,
        "tool": "cargo",
        "benches": [
          {
            "name": "circuit_breaker/check_request_closed",
            "value": 361,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_and_record_success",
            "value": 1384,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/10",
            "value": 3928,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/100",
            "value": 40018,
            "range": "± 357",
            "unit": "ns/iter"
          },
          {
            "name": "circuit_breaker/check_multiple_keys/1000",
            "value": 410368,
            "range": "± 3596",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_hit",
            "value": 105,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_miss",
            "value": 86,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_insert",
            "value": 4153,
            "range": "± 1215",
            "unit": "ns/iter"
          },
          {
            "name": "module_cache/cache_with_eviction",
            "value": 3109,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/runtime_create",
            "value": 205368,
            "range": "± 659",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_simple",
            "value": 2264,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_json_transform",
            "value": 8421,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/eval_function_call",
            "value": 4049,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "script_execution/script_preprocess",
            "value": 504,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent_access/circuit_breaker_concurrent",
            "value": 380841,
            "range": "± 5387",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent_access/module_cache_concurrent",
            "value": 155379,
            "range": "± 1769",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_single",
            "value": 10,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/10",
            "value": 111,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/100",
            "value": 1107,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_batch/1000",
            "value": 11564,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_exhausted",
            "value": 151,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/1024",
            "value": 11,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/8192",
            "value": 11,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "buffer_pool/acquire_release_size/65536",
            "value": 11,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "store_pool/placeholder_acquire_release",
            "value": 10,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_next",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/2",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/4",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/8",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/16",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "scheduling/round_robin_workers/32",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/counter_increment",
            "value": 2,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/request_lifecycle",
            "value": 14,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "metrics/counter_concurrent",
            "value": 121975,
            "range": "± 1477",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent/buffer_pool_contention",
            "value": 124673,
            "range": "± 1000",
            "unit": "ns/iter"
          },
          {
            "name": "concurrent/mixed_workload_read_heavy",
            "value": 121925,
            "range": "± 2001",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}