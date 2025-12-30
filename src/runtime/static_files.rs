//! Static file serving for the runtime.
//!
//! This module handles serving static files from a configured directory:
//! - Path sanitization and security
//! - MIME type detection
//! - Cache-Control headers

use crate::runtime::STATIC_PREFIX;
use crate::runtime::security;
use anyhow::Result;
use http_body_util::Full;
use hyper::Response;
use hyper::body::Bytes;
use std::borrow::Cow;
use std::path::Path;
use tracing::{error, warn};

/// Cache-Control header value for static files (1 hour).
pub(crate) const STATIC_CACHE_CONTROL: &str = "public, max-age=3600";

/// Serve a static file from the static directory.
/// Path format: /static/<project>/<file>
pub(crate) async fn serve_static_file(
    static_dir: &Path,
    path: &str,
) -> Result<Response<Full<Bytes>>> {
    // Strip /static/ prefix
    let file_path = path.strip_prefix(STATIC_PREFIX).unwrap_or(path);

    // Security: sanitize path to prevent directory traversal
    let sanitized_path = match security::sanitize_file_path(file_path) {
        Ok(p) => p,
        Err(e) => {
            warn!("Path traversal attempt blocked: {} - {}", file_path, e);
            return Ok(Response::builder()
                .status(400)
                .body(Full::new(Bytes::from("Invalid path")))?);
        },
    };

    // Check if it's a directory - try index.html (async to avoid blocking)
    let check_path = static_dir.join(&sanitized_path);
    let target_path = match tokio::fs::metadata(&check_path).await {
        Ok(meta) if meta.is_dir() => sanitized_path.join("index.html"),
        _ => sanitized_path,
    };

    // Security: validate path stays within static_dir after resolving symlinks (TOCTOU protection)
    let full_path = match security::validate_path_within_base(static_dir, &target_path) {
        Ok(p) => p,
        Err(e) => {
            warn!(
                "Symlink escape attempt blocked: {} - {}",
                target_path.display(),
                e
            );
            return Ok(Response::builder()
                .status(400)
                .body(Full::new(Bytes::from("Invalid path")))?);
        },
    };

    // Read file
    match tokio::fs::read(&full_path).await {
        Ok(contents) => {
            let content_type = guess_content_type(&full_path);
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", content_type.as_ref())
                .header("Cache-Control", STATIC_CACHE_CONTROL)
                .body(Full::new(Bytes::from(contents)))?)
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => not_found("File not found"),
        Err(e) => {
            error!("Failed to read static file {}: {}", full_path.display(), e);
            Ok(Response::builder()
                .status(500)
                .body(Full::new(Bytes::from("Internal server error")))?)
        },
    }
}

/// Create a 404 Not Found response.
fn not_found(message: &str) -> Result<Response<Full<Bytes>>> {
    Ok(Response::builder()
        .status(404)
        .body(Full::new(Bytes::from(message.to_string())))?)
}

/// Guess content type from file extension using the `mime_guess` crate.
///
/// Uses a comprehensive MIME type database instead of custom extension matching.
/// Returns `Cow<'static, str>` to avoid allocations for common MIME types.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use mik::runtime::guess_content_type;
///
/// assert_eq!(guess_content_type(Path::new("style.css")), "text/css; charset=utf-8");
/// assert_eq!(guess_content_type(Path::new("image.png")), "image/png");
/// assert_eq!(guess_content_type(Path::new("data.bin")), "application/octet-stream");
/// ```
pub fn guess_content_type(path: &Path) -> Cow<'static, str> {
    mime_guess::from_path(path)
        .first()
        .map_or(Cow::Borrowed("application/octet-stream"), |mime| {
            // Use static strings for common types to avoid allocations
            let mime_str = mime.essence_str();
            match mime_str {
                "text/html" => Cow::Borrowed("text/html; charset=utf-8"),
                "text/css" => Cow::Borrowed("text/css; charset=utf-8"),
                "text/javascript" => Cow::Borrowed("text/javascript; charset=utf-8"),
                "application/javascript" => Cow::Borrowed("application/javascript; charset=utf-8"),
                "application/json" => Cow::Borrowed("application/json; charset=utf-8"),
                "text/plain" => Cow::Borrowed("text/plain; charset=utf-8"),
                "text/xml" => Cow::Borrowed("text/xml; charset=utf-8"),
                "application/xml" => Cow::Borrowed("application/xml; charset=utf-8"),
                // Common binary types (no charset needed)
                "image/png" => Cow::Borrowed("image/png"),
                "image/jpeg" => Cow::Borrowed("image/jpeg"),
                "image/gif" => Cow::Borrowed("image/gif"),
                "image/svg+xml" => Cow::Borrowed("image/svg+xml"),
                "image/webp" => Cow::Borrowed("image/webp"),
                "image/x-icon" => Cow::Borrowed("image/x-icon"),
                "application/pdf" => Cow::Borrowed("application/pdf"),
                "application/wasm" => Cow::Borrowed("application/wasm"),
                // Uncommon types: allocate only when needed
                _ => {
                    if mime_str.starts_with("text/")
                        || mime_str.contains("json")
                        || mime_str.contains("xml")
                    {
                        Cow::Owned(format!("{mime_str}; charset=utf-8"))
                    } else {
                        Cow::Owned(mime_str.to_string())
                    }
                },
            }
        })
}
