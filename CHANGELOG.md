# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-12-27

### Added

- WASI HTTP runtime powered by wasmtime
- JS orchestration via rquickjs with `host.call()` API
- Circuit breaker pattern for per-module failure tracking with half-open recovery
- Rate limiting with global and per-module semaphores
- LRU cache with byte-aware module caching and configurable limits
- Static file serving with MIME type detection
- OpenTelemetry OTLP support for distributed tracing (optional feature)
- Graceful shutdown with SIGTERM/SIGINT handling and connection draining
- Path traversal prevention and input sanitization
- Configurable body size limits and execution timeouts
- Script orchestration endpoint (`/script/<name>`) for chaining WASM handlers
- Horizontal scaling with `--workers` flag for multi-process deployment
- Auto-detect CPU cores with `--workers 0` option
- Centralized `constants.rs` with security limits and documented defaults
- Typed error modules (`runtime/error.rs`, `daemon/error.rs`) for structured error handling
- Comprehensive test suites for security-critical modules:
  - `security.rs`: 46 tests (path traversal, null bytes, Unicode, Windows paths)
  - `circuit_breaker.rs`: 39 tests (state transitions, timeouts, concurrency)
  - `script.rs`: 58 tests (JS execution, sandbox security)
- CI enhancements: MSRV check (1.85), security audit, code coverage, fuzz corpus validation

### Changed

- Pooling allocator and parallel compilation for better performance
- Migrated config defaults to use centralized constants
- Circuit breaker defaults now use constants (threshold=5, recovery=60s)
- Added `#![deny(unsafe_code)]` with documented exceptions for wasmtime AOT cache

### Fixed

- Circuit breaker threshold=1 edge case (first failure now opens circuit immediately)
- Platform-specific path parsing in security tests for Windows compatibility

### Removed

- Blanket `#![allow(dead_code)]` from 8 modules (cron, logging, watch, metrics, kv, sql, queue, circuit_breaker)
