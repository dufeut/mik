//! High-performance runtime test module.
//!
//! This module contains tests for the multi-worker runtime implementation
//! as specified in `.notes/high-performance-runtime.md`.
//!
//! # Test Categories
//!
//! - **Property Tests** (`property_tests.rs`): Proptest-based invariant testing
//!   for configuration validation, buffer pool bounds, and store pool behavior.
//!
//! - **Integration Tests** (`integration_tests.rs`): Full runtime integration
//!   tests that verify server startup, connection handling, and graceful shutdown.
//!
//! # Running Tests
//!
//! ```bash
//! # Run all runtime tests
//! cargo test --test runtime
//!
//! # Run only property tests
//! cargo test --test runtime property
//!
//! # Run only integration tests
//! cargo test --test runtime integration
//! ```
//!
//! # Test Naming Convention
//!
//! Tests follow the pattern: `test_<unit>_<scenario>_<expected>`
//!
//! Examples:
//! - `test_config_validation_invalid_pool_size_returns_error`
//! - `test_buffer_pool_acquire_release_returns_to_pool`
//! - `test_server_shutdown_drains_connections`

mod property_tests;
mod integration_tests;
