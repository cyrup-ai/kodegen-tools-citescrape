//! Path utilities for converting URLs to filesystem-safe output directories
//!
//! Provides URL parsing and sanitization for creating safe directory structures
//! from web URLs while maintaining consistency across crawl operations.

use kodegen_config::KodegenConfig;
use kodegen_mcp_schema::McpError;
use std::path::PathBuf;
use url::Url;

/// Convert URL to filesystem-safe output directory path
///
/// Extracts domain from URL and sanitizes for filesystem use.
/// Default base directory is `${git_root}/.kodegen/citescrape` (if in git repo),
/// otherwise falls back to `~/.local/share/kodegen/citescrape`.
///
/// # Examples
/// 
/// Basic usage with default output directory:
/// ```
/// use kodegen_tools_citescrape::mcp::manager::url_to_output_dir;
/// 
/// let output_dir = url_to_output_dir(
///     "https://ratatui.rs/concepts/layout",
///     None,
///     None
/// ).unwrap();
/// 
/// assert!(output_dir.to_string_lossy().contains("ratatui.rs"));
/// ```
/// 
/// With custom base directory:
/// ```
/// use kodegen_tools_citescrape::mcp::manager::url_to_output_dir;
/// use std::path::PathBuf;
/// 
/// let output_dir = url_to_output_dir(
///     "https://example.com:8080/path",
///     Some("/tmp/custom"),
///     None
/// ).unwrap();
/// 
/// assert_eq!(output_dir, PathBuf::from("/tmp/custom/example.com"));
/// ```
pub fn url_to_output_dir(
    url: &str,
    base_dir: Option<&str>,
    client_pwd: Option<&std::path::Path>,
) -> Result<PathBuf, McpError> {
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

    // Determine base directory with proper precedence:
    // 1. Explicit base_dir parameter (highest priority)
    // 2. ${git_root}/.kodegen/citescrape (if in git repo)
    // 3. ${data_dir}/citescrape (fallback)
    let base = if let Some(dir) = base_dir {
        PathBuf::from(dir)
    } else if let Ok(local_config) = KodegenConfig::local_config_dir() {
        local_config.join("citescrape")
    } else {
        KodegenConfig::data_dir()
            .map(|data| data.join("citescrape"))
            .unwrap_or_else(|_| PathBuf::from(".kodegen/citescrape"))
    };

    let output_dir = base.join(safe_domain);

    // Convert to absolute path using client's PWD (if available)
    let output_dir = if output_dir.is_absolute() {
        output_dir
    } else {
        // Use client's pwd if available (HTTP MCP case), fallback to server's pwd (library/test case)
        let base_path = if let Some(pwd) = client_pwd {
            pwd.to_path_buf()
        } else {
            // Fallback for non-HTTP clients (direct library usage, tests, legacy)
            std::env::current_dir()
                .map_err(|e| McpError::InvalidUrl(format!("Failed to get current directory: {e}")))?
        };
        base_path.join(&output_dir)
    };

    Ok(output_dir)
}
