//! Registry operations for WASM components.
//!
//! Downloads WASM components from HTTP(S) URLs with GitHub authentication support.
//! Includes automatic retry with exponential backoff for transient failures.

use crate::reliability::retry::{RetryConfig, retry_sync};
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// HTTP timeout for receiving response body
const HTTP_RECV_BODY_TIMEOUT_SECS: u64 = 60;
/// HTTP connect timeout
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;

/// `GitHub` domain patterns for authentication.
const GITHUB_DOMAINS: [&str; 2] = ["github.com", "githubusercontent.com"];

/// Download a file from any HTTP(S) URL with automatic retry.
pub fn download(url: &str, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let body = download_from_url(url)?;
    let mut file = File::create(output_path).context("Failed to create output file")?;
    io::copy(&mut body.into_reader(), &mut file)?;

    Ok(())
}

/// Download and extract a .tar.gz archive with automatic retry.
#[allow(dead_code)]
pub fn download_and_extract(url: &str, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;

    let body = download_from_url(url)?;
    let gz = flate2::read::GzDecoder::new(body.into_reader());
    tar::Archive::new(gz)
        .unpack(output_dir)
        .context("Failed to extract archive")?;

    Ok(())
}

/// Download from URL with retry, handling `GitHub` authentication.
fn download_from_url(url: &str) -> Result<ureq::Body> {
    let url_owned = url.to_string();
    let token = if is_github_url(url) {
        get_github_token()
    } else {
        None
    };

    retry_sync(&RetryConfig::network(), "http_download", move || {
        let config = ureq::Agent::config_builder()
            .timeout_recv_body(Some(Duration::from_secs(HTTP_RECV_BODY_TIMEOUT_SECS)))
            .timeout_connect(Some(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS)))
            .build();
        let agent: ureq::Agent = config.into();
        let mut request = agent
            .get(&url_owned)
            .header("Accept", "application/octet-stream");

        if let Some(ref t) = token {
            request = request.header("Authorization", &format!("Bearer {t}"));
        }

        request
            .call()
            .map(|resp| resp.into_body())
            .context("HTTP request failed")
    })
}

/// Check if URL is a `GitHub` domain.
fn is_github_url(url: &str) -> bool {
    GITHUB_DOMAINS.iter().any(|domain| url.contains(domain))
}

/// Get `GitHub` token from gh CLI or `GITHUB_TOKEN` env var.
fn get_github_token() -> Option<String> {
    Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let token = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (!token.is_empty()).then_some(token)
        })
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
}
