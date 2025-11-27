use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::oneshot;
use url::Url;

use super::crawl_types::{CrawlError, Crawler};
use crate::config::CrawlConfig;
use crate::content_saver::{self};
use crate::page_extractor::link_rewriter::LinkRewriter;
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
    link_rewriter: LinkRewriter,
}

impl ChromiumoxideCrawler {
    #[must_use]
    pub fn new(config: CrawlConfig) -> Self {
        let link_rewriter = LinkRewriter::new(config.storage_dir());
        let chrome_data_dir = config.chrome_data_dir().cloned();
        Self {
            config,
            chrome_data_dir,
            link_rewriter,
        }
    }

    async fn crawl_impl(&mut self) -> Result<()> {
        let config = self.config.clone();
        let link_rewriter = self.link_rewriter.clone();
        let chrome_data_dir = self.chrome_data_dir.clone();

        let chrome_data_dir_path =
            super::crawl_impl(config, link_rewriter, chrome_data_dir).await?;

        self.chrome_data_dir = chrome_data_dir_path;
        Ok(())
    }
}

impl Crawler for ChromiumoxideCrawler {
    fn new(config: CrawlConfig) -> Self {
        let link_rewriter = LinkRewriter::new(config.storage_dir());
        let chrome_data_dir = config.chrome_data_dir().cloned();
        Self {
            config,
            chrome_data_dir,
            link_rewriter,
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
        .filter(|link| {
            // Skip external links unless explicitly allowed
            if link.is_external && !config.allow_external_domains() {
                return false;
            }

            // Check if URL should be visited based on config
            should_visit_url(&link.url, config)
        })
        .map(|link| link.url.clone())
        .collect()
}

#[must_use]
pub fn should_visit_url(url: &str, config: &CrawlConfig) -> bool {
    let parsed_url = match Url::parse(url) {
        Ok(parsed_url) => parsed_url,
        Err(_) => return false,
    };

    let start_url = match Url::parse(config.start_url()) {
        Ok(parsed_start_url) => parsed_start_url,
        Err(_) => return false,
    };

    if parsed_url.scheme() != start_url.scheme() {
        return false;
    }

    let url_host = parsed_url.host_str().unwrap_or_default();
    let start_host = start_url.host_str().unwrap_or_default();

    // Check basic host matching
    let host_allowed = url_host == start_host
        || config.allow_subdomains() && url_host.ends_with(start_host)
        || config.allow_external_domains();

    if !host_allowed {
        return false;
    }

    // Check allowed_domains list if configured
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

    // Path validation: ensure URL path is beneath the start URL's path
    // Only validate path boundaries when on the same host or subdomain
    // (external domains are handled by allow_external_domains flag)
    if url_host == start_host || (config.allow_subdomains() && url_host.ends_with(start_host)) {
        let url_path = parsed_url.path();
        let start_path = start_url.path();

        // Normalize the start path by removing trailing slashes
        // (unless it's the root path "/")
        let normalized_start_path = if start_path == "/" {
            start_path
        } else {
            start_path.trim_end_matches('/')
        };

        // Check if URL path is beneath the start path
        // Allow: exact match, or child path (starting with start_path + "/")
        let path_allowed = if normalized_start_path == "/" {
            // Root path: allow all paths on this domain
            true
        } else if url_path == normalized_start_path {
            // Exact match (e.g., "/docs" matches "/docs")
            true
        } else if url_path.starts_with(&format!("{}/", normalized_start_path)) {
            // Child path (e.g., "/docs/api" starts with "/docs/")
            true
        } else if url_path == format!("{}/", normalized_start_path) {
            // Exact match with trailing slash (e.g., "/docs/" matches "/docs")
            true
        } else {
            false
        };

        if !path_allowed {
            return false;
        }
    }

    // Check excluded_patterns using pre-compiled regexes
    // Patterns are compiled once at config creation to avoid hot-path compilation
    for regex in config.excluded_patterns_compiled() {
        if regex.is_match(url) {
            return false;
        }
    }

    // Also check for simple string matching if original patterns exist
    if let Some(excluded_patterns) = config.excluded_patterns() {
        for pattern in excluded_patterns {
            if url.contains(pattern) {
                return false;
            }
        }
    }

    true
}
