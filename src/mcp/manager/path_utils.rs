//! Path utilities for converting URLs to filesystem-safe output directories
//!
//! Provides URL parsing and sanitization for creating safe directory structures
//! from web URLs while maintaining consistency across crawl operations.

use kodegen_mcp_tool::error::McpError;
use std::path::PathBuf;
use url::Url;

/// Convert URL to filesystem-safe output directory path
///
/// Extracts domain from URL and sanitizes for filesystem use.
/// Default base directory is "docs".
///
/// # Examples
/// ```
/// url_to_output_dir("https://ratatui.rs/concepts/layout", None)
/// // => Ok(PathBuf::from("docs/ratatui.rs"))
///
/// url_to_output_dir("https://example.com:8080/path", Some("output"))
/// // => Ok(PathBuf::from("output/example.com_8080"))
/// ```
pub fn url_to_output_dir(url: &str, base_dir: Option<&str>) -> Result<PathBuf, McpError> {
    let parsed_url =
        Url::parse(url).map_err(|e| McpError::InvalidUrl(format!("Invalid URL '{url}': {e}")))?;

    let domain = parsed_url
        .host_str()
        .ok_or_else(|| McpError::InvalidUrl(format!("URL '{url}' has no host")))?;

    // Sanitize domain for filesystem
    // Replace characters that are problematic in file paths
    let safe_domain = domain
        .replace([':', '/', '\\'], "_") // Windows path separator
        .replace("..", "_"); // Directory traversal protection

    let base = base_dir.unwrap_or("docs");
    let output_dir = PathBuf::from(base).join(safe_domain);

    // Convert to absolute path to avoid CWD issues in indexing
    let output_dir = if output_dir.is_absolute() {
        output_dir
    } else {
        std::env::current_dir()
            .map_err(|e| McpError::InvalidUrl(format!("Failed to get current directory: {e}")))?
            .join(&output_dir)
    };

    Ok(output_dir)
}
