//! Retry utilities with exponential backoff.
//!
//! Provides retry logic for transient failures using the `backon` crate.
//!
//! # Example
//!
//! ```rust,ignore
//! use mik::reliability::retry::{retry_async, RetryConfig};
//!
//! let result = retry_async(
//!     RetryConfig::default(),
//!     || async { fetch_from_registry().await },
//!     |e| e.is_transient(),
//! ).await;
//! ```

use backon::{ExponentialBuilder, Retryable};
use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including the initial attempt).
    pub max_retries: u32,
    /// Initial delay before first retry.
    pub initial_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles delay each retry).
    pub factor: f32,
    /// Optional jitter factor (0.0 to 1.0) to add randomness to delays.
    #[allow(dead_code)]
    pub jitter: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            factor: 2.0,
            jitter: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create a config for quick operations (fewer retries, shorter delays).
    #[must_use]
    #[allow(dead_code)]
    pub fn quick() -> Self {
        Self {
            max_retries: 2,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
            factor: 2.0,
            jitter: 0.1,
        }
    }

    /// Create a config for network operations (more retries, longer delays).
    ///
    /// Aligned with AWS SDK standard retry configuration.
    #[must_use]
    pub fn network() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(20),
            factor: 2.0,
            jitter: 0.2,
        }
    }

    /// Create a config for critical operations (many retries, long delays).
    #[must_use]
    #[allow(dead_code)]
    pub fn critical() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            factor: 2.0,
            jitter: 0.1,
        }
    }

    /// Set maximum number of retries.
    #[must_use]
    #[allow(dead_code)]
    pub const fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set initial delay.
    #[must_use]
    #[allow(dead_code)]
    pub const fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Build the exponential backoff strategy.
    fn build_backoff(&self) -> ExponentialBuilder {
        ExponentialBuilder::default()
            .with_min_delay(self.initial_delay)
            .with_max_delay(self.max_delay)
            .with_max_times(self.max_retries as usize)
            .with_factor(self.factor)
            .with_jitter()
    }
}

/// Retry an async operation with exponential backoff.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - The async operation to retry
/// * `is_retryable` - Predicate to determine if an error is transient
///
/// # Returns
///
/// The result of the operation, or the last error if all retries failed.
///
/// # Example
///
/// ```rust,ignore
/// let result = retry_async(
///     RetryConfig::network(),
///     || async { client.fetch(url).await },
///     |e| matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::ConnectionRefused),
/// ).await;
/// ```
#[allow(dead_code)]
pub async fn retry_async<F, Fut, T, E, R>(
    config: RetryConfig,
    operation: F,
    is_retryable: R,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
    R: Fn(&E) -> bool,
{
    let backoff = config.build_backoff();
    let max_retries = config.max_retries;

    let mut attempt = 0u32;
    let notify = |err: &E, dur: Duration| {
        attempt += 1;
        warn!(
            attempt = attempt,
            max_retries = max_retries,
            next_delay_ms = dur.as_millis() as u64,
            error = %err,
            "Retry attempt failed, will retry"
        );
    };

    operation
        .retry(backoff)
        .when(move |e| is_retryable(e))
        .notify(notify)
        .await
}

/// Retry an async operation that returns anyhow::Result.
///
/// Uses a default predicate that retries on common transient errors.
pub async fn retry_anyhow<F, Fut, T>(
    config: RetryConfig,
    operation_name: &str,
    operation: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let name = operation_name.to_string();
    let backoff = config.build_backoff();
    let max_retries = config.max_retries;

    let mut attempt = 0u32;
    let notify = |err: &anyhow::Error, dur: Duration| {
        attempt += 1;
        warn!(
            operation = %name,
            attempt = attempt,
            max_retries = max_retries,
            next_delay_ms = dur.as_millis() as u64,
            error = %err,
            "Operation failed, will retry"
        );
    };

    operation
        .retry(backoff)
        .when(is_transient_error)
        .notify(notify)
        .await
}

/// Determine if an error is transient and worth retrying.
///
/// Returns `true` for:
/// - Connection refused/reset
/// - Timeouts
/// - Temporary I/O errors
/// - HTTP 5xx, 408, 429
pub fn is_transient_error(error: &anyhow::Error) -> bool {
    let msg = error.to_string().to_lowercase();

    // Network errors
    if msg.contains("connection refused")
        || msg.contains("connection reset")
        || msg.contains("connection closed")
        || msg.contains("broken pipe")
        || msg.contains("network unreachable")
        || msg.contains("host unreachable")
    {
        debug!("Transient error detected: connection issue");
        return true;
    }

    // Timeout errors
    if msg.contains("timed out") || msg.contains("timeout") || msg.contains("deadline exceeded") {
        debug!("Transient error detected: timeout");
        return true;
    }

    // DNS errors (can be transient)
    if msg.contains("dns") && (msg.contains("temporary") || msg.contains("again")) {
        debug!("Transient error detected: DNS issue");
        return true;
    }

    // HTTP status codes (5xx, 408, 429)
    if msg.contains("status: 5")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("408")
        || msg.contains("429")
        || msg.contains("too many requests")
        || msg.contains("service unavailable")
        || msg.contains("gateway timeout")
        || msg.contains("bad gateway")
    {
        debug!("Transient error detected: HTTP server error");
        return true;
    }

    // I/O errors that can be transient
    if msg.contains("resource temporarily unavailable")
        || msg.contains("try again")
        || msg.contains("interrupted")
        || msg.contains("would block")
    {
        debug!("Transient error detected: temporary I/O issue");
        return true;
    }

    // Database lock contention
    if msg.contains("database is locked") || msg.contains("busy") {
        debug!("Transient error detected: database contention");
        return true;
    }

    false
}

/// Check if an HTTP status code is retryable.
#[must_use]
#[allow(dead_code)]
pub const fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

/// Retry a synchronous operation with exponential backoff.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation_name` - Name for logging
/// * `operation` - The sync operation to retry
///
/// # Returns
///
/// The result of the operation, or the last error if all retries failed.
pub fn retry_sync<F, T>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, anyhow::Error>
where
    F: FnMut() -> Result<T, anyhow::Error>,
{
    let mut attempt = 0u32;
    let max_retries = config.max_retries;
    let mut delay = config.initial_delay;

    loop {
        match operation() {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        operation = %operation_name,
                        attempts = attempt + 1,
                        "Operation succeeded after retries"
                    );
                }
                return Ok(result);
            },
            Err(e) => {
                if !is_transient_error(&e) {
                    debug!(
                        operation = %operation_name,
                        error = %e,
                        "Permanent error, not retrying"
                    );
                    return Err(e);
                }

                if attempt >= max_retries {
                    warn!(
                        operation = %operation_name,
                        attempts = attempt + 1,
                        error = %e,
                        "Max retries exhausted"
                    );
                    return Err(e);
                }

                warn!(
                    operation = %operation_name,
                    attempt = attempt + 1,
                    max_retries = max_retries,
                    next_delay_ms = delay.as_millis() as u64,
                    error = %e,
                    "Transient error, retrying"
                );

                std::thread::sleep(delay);
                attempt += 1;

                // Exponential backoff with cap
                delay = std::cmp::min(
                    Duration::from_secs_f32(delay.as_secs_f32() * config.factor),
                    config.max_delay,
                );
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_retry_config_quick() {
        let config = RetryConfig::quick();
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
    }

    #[test]
    fn test_retry_config_network() {
        let config = RetryConfig::network();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(20));
    }

    #[test]
    fn test_is_transient_error_timeout() {
        let err = anyhow::anyhow!("operation timed out");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_is_transient_error_connection_refused() {
        let err = anyhow::anyhow!("connection refused");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_is_transient_error_503() {
        let err = anyhow::anyhow!("HTTP status: 503 Service Unavailable");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_is_transient_error_429() {
        let err = anyhow::anyhow!("too many requests");
        assert!(is_transient_error(&err));
    }

    #[test]
    fn test_is_transient_error_permanent() {
        let err = anyhow::anyhow!("file not found");
        assert!(!is_transient_error(&err));
    }

    #[test]
    fn test_is_transient_error_404() {
        let err = anyhow::anyhow!("HTTP status: 404 Not Found");
        assert!(!is_transient_error(&err));
    }

    #[test]
    fn test_is_retryable_status() {
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(504));
        assert!(is_retryable_status(408));
        assert!(is_retryable_status(429));

        assert!(!is_retryable_status(200));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(404));
    }

    #[tokio::test]
    async fn test_retry_async_succeeds_first_try() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, &str> = retry_async(
            RetryConfig::quick(),
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                }
            },
            |_| true,
        )
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_async_succeeds_after_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, String> = retry_async(
            RetryConfig::quick().with_initial_delay(Duration::from_millis(10)),
            || {
                let c = counter_clone.clone();
                async move {
                    let attempt = c.fetch_add(1, Ordering::SeqCst);
                    if attempt < 2 {
                        Err(format!("attempt {attempt} failed"))
                    } else {
                        Ok(42)
                    }
                }
            },
            |_| true,
        )
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_async_exhausts_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, String> = retry_async(
            RetryConfig::quick()
                .with_max_retries(2)
                .with_initial_delay(Duration::from_millis(10)),
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("always fails".to_string())
                }
            },
            |_| true,
        )
        .await;

        assert!(result.is_err());
        // Initial attempt + 2 retries = 3 total
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_async_no_retry_on_permanent_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, String> = retry_async(
            RetryConfig::quick().with_initial_delay(Duration::from_millis(10)),
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("permanent error".to_string())
                }
            },
            |e| e.contains("transient"), // Only retry if error contains "transient"
        )
        .await;

        assert!(result.is_err());
        // Should not retry since error doesn't match predicate
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
