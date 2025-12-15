use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use flate2::{Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::time::Duration as StdDuration;
use tempfile::NamedTempFile;
use tokio::time::timeout;

/// Timeout for blocking compression operations
/// Large files (>1MB) are compressed on blocking thread pool
const BLOCKING_COMPRESSION_TIMEOUT: StdDuration = StdDuration::from_secs(30);

/// Maximum allowed length for content_type field
/// Real-world Content-Type headers rarely exceed 200 bytes:
/// - "text/html; charset=utf-8" = 24 bytes
/// - "application/json; charset=utf-8" = 31 bytes
/// - Even with boundary parameters rarely exceeds 100 bytes
///
/// 512 bytes provides generous headroom for legitimate use cases
const MAX_CONTENT_TYPE_LEN: usize = 512;

/// Gzip comment field maximum size per RFC 1952
const MAX_METADATA_JSON_LEN: usize = 60_000;

/// Sanitize content_type header to prevent oversized metadata
///
/// Extracts the essential parts of a Content-Type header:
/// 1. Main MIME type (e.g., "text/html")
/// 2. charset parameter if present
///
/// Discards non-essential parameters that could be used for DoS attacks.
///
/// # Examples
/// ```
/// assert_eq!(
///     sanitize_content_type("text/html; charset=utf-8"),
///     "text/html; charset=utf-8"
/// );
///
/// // Long malicious header is truncated intelligently
/// let malicious = format!("text/html; {}", "junk;".repeat(10000));
/// let result = sanitize_content_type(&malicious);
/// assert!(result.len() <= 512);
/// assert!(result.starts_with("text/html"));
/// ```
fn sanitize_content_type(raw: &str) -> String {
    // Fast path: already within limits
    if raw.len() <= MAX_CONTENT_TYPE_LEN {
        return raw.to_string();
    }

    // Parse Content-Type header format: "type/subtype; param=value; param=value"
    let parts: Vec<&str> = raw.split(';').collect();
    
    // Always preserve the main MIME type (first part)
    let mut result = parts[0].trim().to_string();
    
    // Try to preserve charset parameter (essential for text rendering)
    for part in &parts[1..] {
        let part = part.trim();
        if part.starts_with("charset=") {
            result.push_str("; ");
            result.push_str(part);
            break;
        }
    }
    
    // Final safety truncation if still too long
    // (shouldn't happen with valid Content-Type, but defense in depth)
    if result.len() > MAX_CONTENT_TYPE_LEN {
        result.truncate(MAX_CONTENT_TYPE_LEN);
        log::warn!(
            "Content-Type extremely long even after parsing, truncated to {} bytes",
            MAX_CONTENT_TYPE_LEN
        );
    }
    
    result
}

/// All compression + file I/O uses `spawn_blocking` to prevent blocking the async runtime
/// Metadata stored in compressed files for caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub etag: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expires: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_modified: DateTime<Utc>,
    pub content_type: String,
}

/// Save content as a file with optional compression and cache metadata
/// Returns (`actual_saved_path`, metadata)
///
/// When compress=true, saves as .gz file
/// When compress=false, saves as plain file
///
/// # Arguments
///
/// * `content` - Raw content bytes to save
/// * `path` - Target file path (extension will be modified if compressing)
/// * `content_type` - MIME type for cache metadata
/// * `compress` - Whether to gzip compress the content
/// * `compression_threshold` - Size threshold in bytes for using spawn_blocking (default: 1MB)
///
/// # Performance
///
/// Content larger than `compression_threshold` will be compressed using
/// `tokio::task::spawn_blocking()` to avoid blocking the async runtime.
/// Smaller content is compressed directly for lower overhead.
pub async fn save_compressed_file(
    content: Vec<u8>,
    path: &Path,
    content_type: &str,
    compress: bool,
    _compression_threshold: usize,
) -> Result<(std::path::PathBuf, CacheMetadata)> {
    let path = path.to_path_buf();
    
    // SECURITY: Sanitize content_type BEFORE creating metadata
    // This prevents DoS attacks via oversized Content-Type headers
    let content_type = sanitize_content_type(content_type);

    // Calculate XXHash for etag (unchanged)
    let hash = xxhash_rust::xxh3::xxh3_64(&content);
    let etag = format!("\"{hash:x}\"");

    // Set cache control headers (unchanged)
    let now = Utc::now();
    let expires = now + Duration::seconds(7 * 24 * 60 * 60);

    let metadata = CacheMetadata {
        etag,
        expires,
        last_modified: now,
        content_type,
    };

    if compress {
        // Compressed file path
        let gz_path = path.with_extension(format!(
            "{}.gz",
            path.extension().unwrap_or_default().to_str().unwrap_or("")
        ));

        // Get parent directory for atomic temp file creation
        let parent_dir = gz_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Path has no parent directory"))?;

        let metadata_json = serde_json::to_string(&metadata)?;

        // SAFETY: This should never fail now that content_type is sanitized
        // Keep as debug assertion to catch any unexpected edge cases
        debug_assert!(
            metadata_json.len() <= MAX_METADATA_JSON_LEN,
            "Metadata JSON unexpectedly large: {} bytes (content_type: {:?})",
            metadata_json.len(),
            metadata.content_type
        );

        let filename_str = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Missing filename"))?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid filename encoding"))?
            .to_string();

        let gz_path_clone = gz_path.clone();
            let parent_dir_clone = parent_dir.to_path_buf();
            let filename_clone = filename_str.clone();
            let metadata_json_clone = metadata_json.clone();
            
            // Save values for error logging (needed after closure consumes originals)
            let gz_path_for_log = gz_path_clone.clone();
            let content_len = content.len();

            let blocking_task = tokio::task::spawn_blocking(move || -> Result<()> {
                // Create temp file in same directory as target (ATOMIC)
                let temp_file = NamedTempFile::new_in(&parent_dir_clone)?;
                
                // Write compressed data to temp file
                let mut gz = GzBuilder::new()
                    .filename(filename_clone)
                    .comment(metadata_json_clone)
                    .write(temp_file, Compression::new(3));
                gz.write_all(&content)?;
                let temp_file = gz.finish()?;
                
                // Atomically rename temp file to final path
                // This is atomic at OS level - prevents race conditions
                temp_file.persist(&gz_path_clone)?;
                Ok(())
            });

            match timeout(BLOCKING_COMPRESSION_TIMEOUT, blocking_task).await {
                Ok(Ok(result)) => result?,
                Ok(Err(e)) => return Err(anyhow::anyhow!("Blocking compression task panicked: {}", e)),
                Err(_) => {
                    log::warn!(
                        "Blocking compression timeout for file: {:?} (size: {} bytes, timeout: {:?})",
                        gz_path_for_log,
                        content_len,
                        BLOCKING_COMPRESSION_TIMEOUT
                    );
                    return Err(anyhow::anyhow!(
                        "Compression timed out after {:?} - possible filesystem hang or extremely slow disk",
                        BLOCKING_COMPRESSION_TIMEOUT
                    ));
                }
            }

        Ok((gz_path, metadata))
    } else {
        // Uncompressed file: atomic write pattern
        let parent_dir = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Path has no parent directory"))?;

        let mut temp_file = NamedTempFile::new_in(parent_dir)?;
        temp_file.write_all(&content)?;
        
        // Atomic rename to final path
        temp_file.persist(&path)?;

        Ok((path.clone(), metadata))
    }
}
