use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;

use super::crawl_types::{CrawlError, Crawler};
use crate::config::CrawlConfig;
use crate::content_saver::{self};
use crate::imurl::ImUrl;
use crate::link_index::LinkIndex;
use crate::link_rewriter::LinkRewriter;
use crate::runtime::CrawlRequest;

// CacheMetadata is now imported from content_saver module
pub use content_saver::CacheMetadata;

// content_saver::save_compressed_file is now in content_saver module
// save_html_content is now in content_saver module

// save_html_content_with_resources is now in content_saver module
// save_markdown_content is now in content_saver module
// save_screenshot is now in PageProcessor module

pub struct ChromiumoxideCrawler {
    config: CrawlConfig,
    chrome_data_dir: Option<PathBuf>,
}

impl ChromiumoxideCrawler {
    #[must_use]
    pub fn new(config: CrawlConfig) -> Self {
        let chrome_data_dir = config.chrome_data_dir().cloned();
        Self {
            config,
            chrome_data_dir,
        }
    }

    async fn crawl_impl(&mut self) -> Result<()> {
        let config = self.config.clone();
        let chrome_data_dir = self.chrome_data_dir.clone();

        // Initialize LinkIndex (opens or creates SQLite database)
        let link_index = Arc::new(
            LinkIndex::open(config.storage_dir())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to open link index: {}", e))?
        );

        // Create LinkRewriter with the index
        let link_rewriter = LinkRewriter::new(link_index, config.storage_dir().to_path_buf());

        let chrome_data_dir_path =
            super::crawl_impl(config, link_rewriter, chrome_data_dir).await?;

        self.chrome_data_dir = chrome_data_dir_path;
        Ok(())
    }
}

impl Crawler for ChromiumoxideCrawler {
    fn new(config: CrawlConfig) -> Self {
        let chrome_data_dir = config.chrome_data_dir().cloned();
        Self {
            config,
            chrome_data_dir,
        }
    }

    fn crawl(&self) -> CrawlRequest {
        // Create a channel for the result
        let (tx, rx) = oneshot::channel();

        // Clone what we need for the async task
        let config = self.config.clone();

        // Spawn a task to do the async work
        tokio::spawn(async move {
            // Create a new crawler within the spawned task
            let mut crawler = ChromiumoxideCrawler::new(config);

            // Execute crawl implementation
            let result = crawler.crawl_impl().await;

            // Convert from anyhow::Result to CrawlResult
            let converted_result = match result {
                Ok(()) => Ok(()),
                Err(e) => Err(CrawlError::from(e)),
            };

            // Send the result through the channel
            let _ = tx.send(converted_result);
        });

        // Return concrete type wrapping the channel
        CrawlRequest::new(rx)
    }
}
impl Drop for ChromiumoxideCrawler {
    fn drop(&mut self) {
        if let Some(chrome_data_dir) = &self.chrome_data_dir {
            let _ = std::fs::remove_dir_all(chrome_data_dir);
        }
    }
}

// The process_page function is now incorporated directly in crawl_impl
// and has been replaced by a more complete data extraction approach

// save_page_data is now in content_saver module

/// Extracts URLs from links that pass crawl configuration filters.
///
/// This function takes a list of rich `CrawlLink` objects (containing anchor text,
/// title attributes, rel attributes, external flags, etc.) and returns only the
/// URLs that pass the crawl configuration rules.
///
/// # Metadata Discarded
///
/// **Important**: This function discards link metadata and returns only URLs.
/// The following information from `CrawlLink` is lost:
/// - `text`: Anchor text (useful for priority/relevance scoring)
/// - `title`: Title attribute (may indicate page importance)
/// - `rel`: Rel attributes (e.g., "nofollow" for link quality assessment)
/// - `is_external`: External domain flag (recomputable but discarded)
/// - Other contextual metadata about the link
///
/// # Why URLs Only?
///
/// The current `CrawlQueue` structure only requires URLs for crawling.
/// If you need link metadata for features like:
/// - Link-based priority crawling (e.g., prioritize "documentation" links)
/// - Link text analytics (e.g., most common anchor texts)
/// - Respecting "nofollow" hints
/// - External vs internal link statistics
///
/// Consider enhancing `CrawlQueue` to preserve `CrawlLink` objects or
/// creating a separate metadata-preserving filter function.
///
/// # Parameters
///
/// - `links`: Rich link objects extracted from the page
/// - `config`: Crawl configuration with filtering rules
///
/// # Returns
///
/// Vector of URL strings that passed all filters (external domain rules,
/// allowed domains, excluded patterns, etc.)
#[must_use]
pub fn extract_valid_urls(
    links: &[crate::page_extractor::schema::CrawlLink],
    config: &CrawlConfig,
) -> Vec<String> {
    links
        .iter()
        .filter(|link| should_visit_url(&link.url, config))
        .map(|link| link.url.clone())
        .collect()
}

#[must_use]
pub fn should_visit_url(url: &str, config: &CrawlConfig) -> bool {
    // Use ImUrl for URL parsing (existing infrastructure)
    let parsed_url = match ImUrl::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let start_url = match ImUrl::parse(config.start_url()) {
        Ok(u) => u,
        Err(_) => return false,
    };

    // Scheme must match
    if parsed_url.scheme() != start_url.scheme() {
        return false;
    }

    // Host must match exactly (no subdomains, no external domains in real usage)
    let url_host = parsed_url.host().unwrap_or_default();
    let start_host = start_url.host().unwrap_or_default();

    if url_host != start_host {
        return false;  // Reject different hosts immediately
    }

    // Check allowed_domains list if configured (rare, but keep for compatibility)
    if let Some(allowed_domains) = config.allowed_domains()
        && !allowed_domains.is_empty()
    {
        let domain_matches = allowed_domains
            .iter()
            .any(|domain| url_host == domain || url_host.ends_with(&format!(".{domain}")));
        if !domain_matches {
            return false;
        }
    }

    // PATH VALIDATION - MANDATORY for same-host URLs
    // Query params and fragments are automatically ignored by ImUrl.path()
    let url_path = parsed_url.path();
    let start_path = start_url.path();

    // Normalize BOTH paths for consistent comparison
    let norm_url_path = url_path.trim_end_matches('/');
    let norm_start_path = start_path.trim_end_matches('/');

    // Root path allows all paths on this domain
    if norm_start_path.is_empty() || norm_start_path == "/" {
        // Continue to excluded patterns check below
    } else {
        // URL must be exact match or child of start path
        let path_allowed = norm_url_path == norm_start_path
            || norm_url_path.starts_with(&format!("{}/", norm_start_path));

        if !path_allowed {
            return false;  // REJECT - outside path scope
        }
    }

    // Check excluded patterns
    for regex in config.excluded_patterns_compiled() {
        if regex.is_match(url) {
            return false;
        }
    }

    if let Some(excluded_patterns) = config.excluded_patterns() {
        for pattern in excluded_patterns {
            if url.contains(pattern) {
                return false;
            }
        }
    }

    true
}
