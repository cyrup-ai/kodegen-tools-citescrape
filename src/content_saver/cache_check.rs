use anyhow::{Context, Result};
use chromiumoxide::cdp::browser_protocol::network::{
    EventResponseReceived, Headers, ResourceType,
};
use chromiumoxide::listeners::EventStream;
use flate2::read::GzDecoder;
use futures::StreamExt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::timeout;
use url::Url;

use crate::content_saver::CacheMetadata;

/// Timeout for blocking I/O operations (file open, gzip decompression, JSON parsing)
/// 
/// Prevents indefinite hangs on:
/// - Network filesystem failures (NFS/SMB hang)
/// - Corrupted gzip files (infinite decompression loops)
/// - Slow disk I/O (dying HDDs, resource exhaustion)
const BLOCKING_DECOMPRESSION_TIMEOUT: Duration = Duration::from_secs(15);

/// Get the mirror path for a URL synchronously (internal helper for cache checking)
pub fn get_mirror_path_sync(url: &str, output_dir: &Path, filename: &str) -> Result<PathBuf> {
    let url = Url::parse(url).context("Failed to parse URL")?;
    let domain = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid URL: no host"))?;
    let path = if url.path() == "/" {
        PathBuf::new()
    } else {
        PathBuf::from(url.path().trim_start_matches('/'))
    };

    let mirror_path = output_dir.join(domain).join(path).join(filename);
    Ok(mirror_path)
}

/// Read the cached etag from a gzip file's header comment
///
/// Looks for the expected cache file path based on URL and reads
/// the etag stored in the gzip header comment metadata.
pub async fn read_cached_etag(url: &str, output_dir: &Path) -> Result<Option<String>> {
    // Get expected cache path (this is pure computation, can stay sync)
    let cache_path = get_mirror_path_sync(url, output_dir, "index.md")?;
    let gz_path = cache_path.with_extension("md.gz");
    
    // Clone for error logging (needed after closure consumes original)
    let gz_path_for_log = gz_path.clone();

    // Spawn blocking for file I/O + decompression + parsing
    // Moves work to dedicated blocking thread pool (max 512 threads)
    let blocking_task = tokio::task::spawn_blocking(move || -> Result<Option<String>> {
        // Attempt to open file - handle NotFound gracefully
        let file = match File::open(&gz_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist - this is normal, return None
                return Ok(None);
            }
            Err(e) => {
                // Other I/O errors (permissions, network, disk) - propagate
                return Err(anyhow::anyhow!(
                    "Failed to open cached gzip file at {}: {}",
                    gz_path.display(),
                    e
                ));
            }
        };
        let gz = GzDecoder::new(file);

        let header = gz
            .header()
            .ok_or_else(|| anyhow::anyhow!("No gzip header found"))?;

        let comment = std::str::from_utf8(header.comment().unwrap_or(&[]))
            .context("Invalid UTF-8 in gzip comment")?;

        let metadata: CacheMetadata =
            serde_json::from_str(comment).context("Failed to parse cache metadata JSON")?;

        Ok(Some(metadata.etag))
    });

    match tokio::time::timeout(BLOCKING_DECOMPRESSION_TIMEOUT, blocking_task).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => Err(anyhow::anyhow!("Blocking task panicked: {}", e)),
        Err(_) => {
            log::warn!(
                "Blocking I/O timeout reading cache file: {:?} (timeout: {:?})",
                gz_path_for_log,
                BLOCKING_DECOMPRESSION_TIMEOUT
            );
            Err(anyhow::anyhow!(
                "Cache read timed out after {:?} - possible filesystem hang or corrupted file",
                BLOCKING_DECOMPRESSION_TIMEOUT
            ))
        }
    }
}

/// Extract etag from Network Response headers
///
/// The headers come from CDP EventResponseReceived.response.headers
/// which is a JSON object like {"etag": "abc123", "content-type": "text/html"}
#[must_use]
pub fn extract_etag_from_headers(headers: &Headers) -> Option<String> {
    let headers_json = headers.inner();

    headers_json.as_object()?.get("etag")?.as_str().map(|s| {
        // Strip "W/" prefix for weak etags
        s.strip_prefix("W/").unwrap_or(s).to_string()
    })
}

/// Normalize a URL for cache matching comparison.
///
/// Handles common URL variations that represent the same resource:
/// - Removes fragments (#section)
/// - Removes query parameters (?page=2)
/// - Strips trailing slashes
/// - Preserves scheme (http and https are treated as distinct URLs)
/// - Case-insensitive host comparison
///
/// # Examples
/// ```ignore
/// // Same URL with variations - these ARE equal
/// normalize_url("https://example.com/page/") == normalize_url("https://example.com/page")
/// normalize_url("https://example.com/page#top") == normalize_url("https://example.com/page")
/// normalize_url("https://example.com/page?utm=x") == normalize_url("https://example.com/page")
///
/// // Different schemes - these are NOT equal
/// normalize_url("http://example.com/page") != normalize_url("https://example.com/page")
/// ```
fn normalize_url_for_cache_matching(url_str: &str) -> Option<String> {
    let parsed = Url::parse(url_str).ok()?;
    
    let scheme = parsed.scheme();
    let host = parsed.host_str()?.to_lowercase();
    let path = parsed.path().trim_end_matches('/');
    
    // Normalize empty path to "/"
    let normalized_path = if path.is_empty() { "/" } else { path };
    
    // Build normalized URL: scheme://host/path (no query, no fragment)
    Some(format!("{}://{}{}", scheme, host, normalized_path))
}

/// Async helper: Check if received etag matches expected etag
///
/// Listens to responseReceived events and matches the Document response for the
/// specified URL (not the first Document encountered). Returns true if etag matches
/// (cache hit), false otherwise.
///
/// Handles multiple Document resources correctly (iframes, embedded frames) by matching
/// the response URL against the target URL after normalization.
///
/// # Arguments
/// * `events` - Event stream of network responses from CDP
/// * `url` - The URL to match against response events (used for matching, not ignored)
/// * `expected_etag` - The etag from cached file to compare
/// * `timeout_duration` - How long to wait for response (configurable per crawl)
///
/// # Returns
/// * `true` - ETag matches (cache hit)
/// * `false` - ETag mismatch, no matching document found, or timeout
pub async fn check_etag_from_events(
    events: &mut EventStream<EventResponseReceived>,
    url: &str,  // Now actively used!
    expected_etag: &str,
    timeout_duration: Duration,
) -> bool {
    // Normalize target URL once (efficient)
    let target_url_normalized = match normalize_url_for_cache_matching(url) {
        Some(normalized) => normalized,
        None => {
            log::warn!("Failed to normalize target URL for cache check: {}", url);
            return false;
        }
    };
    
    let result = timeout(timeout_duration, async {
        // Track all Document resources for debugging
        let mut document_count = 0;
        let mut checked_urls: Vec<String> = Vec::new();
        
        while let Some(event) = events.next().await {
            // Only process Document type resources
            if event.r#type != ResourceType::Document {
                continue;
            }
            
            document_count += 1;
            let response_url = event.response.url.as_str();
            
            // Normalize response URL for comparison
            let response_url_normalized = match normalize_url_for_cache_matching(response_url) {
                Some(normalized) => normalized,
                None => {
                    log::debug!("Skipping Document with unparseable URL: {}", response_url);
                    continue;
                }
            };
            
            checked_urls.push(response_url.to_string());
            
            // Match by normalized URL (not by "first Document")
            if response_url_normalized == target_url_normalized {
                if let Some(received_etag) = extract_etag_from_headers(&event.response.headers) {
                    log::debug!(
                        "Cache check: URL match found (document #{}) - etag comparison: {} vs {}",
                        document_count,
                        received_etag,
                        expected_etag
                    );
                    return received_etag == expected_etag;
                }
                
                // Matched URL but no etag header
                log::debug!(
                    "Cache check: URL match found but no etag header for {}",
                    response_url
                );
                return false;
            }
        }
        
        // No match found - log diagnostic information
        if document_count > 1 {
            log::debug!(
                "Cache check: URL '{}' not found in {} Document resources: {:?}",
                url,
                document_count,
                checked_urls
            );
        }
        
        false
    })
    .await;

    result.unwrap_or(false)
}
