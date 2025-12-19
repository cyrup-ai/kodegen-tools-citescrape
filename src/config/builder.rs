//! Type-safe builder for `CrawlConfig` using the typestate pattern
//!
//! This module provides a fluent builder interface with compile-time validation
//! ensuring that required fields are set before building a `CrawlConfig`.

use crate::utils::{
    DEFAULT_CRAWL_RATE_RPS, DEFAULT_MAX_DEPTH, SCREENSHOT_QUALITY, SEARCH_BATCH_SIZE,
};
use anyhow::{anyhow, Result};
use regex::Regex;
use std::marker::PhantomData;
use std::path::PathBuf;

use super::types::CrawlConfig;

/// Compile a glob pattern into a regex
///
/// Converts glob patterns (where * matches any sequence) into proper regex patterns.
/// This is done once at config creation time to avoid repeated compilation in hot paths.
///
/// # Errors
///
/// Returns an error if the resulting regex pattern is invalid.
fn compile_glob_pattern(pattern: &str) -> Result<Regex> {
    // Convert glob pattern to regex: * becomes .*
    let regex_pattern = pattern.replace('*', ".*");

    // Anchor pattern to match full string
    let anchored = format!("^{regex_pattern}$");

    // Compile the regex
    Regex::new(&anchored).map_err(|e| anyhow!("Invalid glob pattern '{pattern}': {e}"))
}

// Type states for the builder
pub struct WithStorageDir;
pub struct WithStartUrl;
pub struct Complete;

pub struct CrawlConfigBuilder<State = ()> {
    pub(crate) storage_dir: Option<PathBuf>,
    pub(crate) only_html: bool,
    pub(crate) full_resources: bool,
    pub(crate) start_url: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) screenshot_quality: u8,
    pub(crate) stealth_mode: bool,
    pub(crate) allow_subdomains: bool,
    pub(crate) allow_external_domains: bool,
    pub(crate) save_screenshots: bool,
    pub(crate) save_json: bool,
    pub(crate) save_raw_html: bool,
    pub(crate) save_markdown: bool,
    pub(crate) headless: bool,
    pub(crate) content_selector: Option<String>,
    pub(crate) allowed_domains: Option<Vec<String>>,
    pub(crate) excluded_patterns: Option<Vec<String>>,
    pub(crate) generate_components: bool,
    pub(crate) progressive: bool,
    pub(crate) presentation_style: String,
    pub(crate) max_depth: u8,
    pub(crate) search_index_dir: Option<PathBuf>,
    pub(crate) search_memory_limit: Option<usize>,
    pub(crate) search_batch_size: Option<usize>,
    pub(crate) crawl_rate_rps: Option<f64>,
    pub(crate) max_inline_image_size_bytes: Option<usize>,
    pub(crate) max_deferred_queue_size: Option<usize>,
    pub(crate) enable_cache_validation: bool,
    pub(crate) ignore_cache: bool,
    pub(crate) cache_validation_timeout_secs: Option<u64>,
    pub(crate) page_load_timeout_secs: Option<u64>,
    pub(crate) navigation_timeout_secs: Option<u64>,
    pub(crate) event_timeout_secs: Option<u64>,
    pub(crate) circuit_breaker_enabled: bool,
    pub(crate) circuit_breaker_failure_threshold: u32,
    pub(crate) circuit_breaker_retry_delay_secs: u64,
    pub(crate) max_concurrent_pages: Option<usize>,
    pub(crate) max_concurrent_per_domain: Option<usize>,
    pub(crate) compression_threshold_bytes: Option<usize>,
    pub(crate) max_page_retries: Option<u8>,
    pub(crate) _phantom: PhantomData<State>,
}

impl Default for CrawlConfigBuilder<()> {
    fn default() -> Self {
        Self {
            storage_dir: None,
            only_html: false,
            full_resources: true,
            start_url: None,
            limit: None,
            screenshot_quality: SCREENSHOT_QUALITY,
            stealth_mode: false,
            allow_subdomains: false,
            allow_external_domains: false,
            save_screenshots: true,
            save_json: true,
            save_raw_html: false,
            save_markdown: true,
            headless: true,
            content_selector: None,
            allowed_domains: None,
            excluded_patterns: None,
            generate_components: false,
            progressive: false,
            presentation_style: String::new(),
            max_depth: DEFAULT_MAX_DEPTH,
            search_index_dir: None,
            search_memory_limit: None,
            search_batch_size: Some(SEARCH_BATCH_SIZE),
            crawl_rate_rps: Some(DEFAULT_CRAWL_RATE_RPS),
            max_inline_image_size_bytes: None,
            max_deferred_queue_size: Some(10_000),
            enable_cache_validation: true,
            ignore_cache: false,
            cache_validation_timeout_secs: Some(15),
            page_load_timeout_secs: Some(30),
            navigation_timeout_secs: Some(30),
            event_timeout_secs: Some(10),
            circuit_breaker_enabled: true,
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_retry_delay_secs: 300,
            max_concurrent_pages: Some(10),
            max_concurrent_per_domain: Some(2),
            compression_threshold_bytes: Some(1_048_576), // 1MB default
            max_page_retries: Some(3),  // Default: 3 retry attempts
            _phantom: PhantomData,
        }
    }
}

impl CrawlConfig {
    /// Create a builder for configuring a `CrawlConfig` with a fluent interface
    #[must_use]
    pub fn builder() -> CrawlConfigBuilder<()> {
        CrawlConfigBuilder::default()
    }
}

impl CrawlConfigBuilder<()> {
    pub fn storage_dir(self, dir: impl Into<PathBuf>) -> CrawlConfigBuilder<WithStorageDir> {
        CrawlConfigBuilder {
            storage_dir: Some(dir.into()),
            only_html: self.only_html,
            full_resources: self.full_resources,
            start_url: self.start_url,
            limit: self.limit,
            screenshot_quality: self.screenshot_quality,
            stealth_mode: self.stealth_mode,
            allow_subdomains: self.allow_subdomains,
            allow_external_domains: self.allow_external_domains,
            save_screenshots: self.save_screenshots,
            save_json: self.save_json,
            save_raw_html: self.save_raw_html,
            save_markdown: self.save_markdown,
            headless: self.headless,
            content_selector: self.content_selector,
            allowed_domains: self.allowed_domains,
            excluded_patterns: self.excluded_patterns,
            generate_components: self.generate_components,
            progressive: self.progressive,
            presentation_style: self.presentation_style,
            max_depth: self.max_depth,
            search_index_dir: self.search_index_dir,
            search_memory_limit: self.search_memory_limit,
            search_batch_size: self.search_batch_size,
            crawl_rate_rps: self.crawl_rate_rps,
            max_inline_image_size_bytes: self.max_inline_image_size_bytes,
            max_deferred_queue_size: self.max_deferred_queue_size,
            enable_cache_validation: self.enable_cache_validation,
            ignore_cache: self.ignore_cache,
            cache_validation_timeout_secs: self.cache_validation_timeout_secs,
            page_load_timeout_secs: self.page_load_timeout_secs,
            navigation_timeout_secs: self.navigation_timeout_secs,
            event_timeout_secs: self.event_timeout_secs,
            circuit_breaker_enabled: self.circuit_breaker_enabled,
            circuit_breaker_failure_threshold: self.circuit_breaker_failure_threshold,
            circuit_breaker_retry_delay_secs: self.circuit_breaker_retry_delay_secs,
            max_concurrent_pages: self.max_concurrent_pages,
            max_concurrent_per_domain: self.max_concurrent_per_domain,
            compression_threshold_bytes: self.compression_threshold_bytes,
            max_page_retries: self.max_page_retries,
            _phantom: PhantomData,
        }
    }
}

impl CrawlConfigBuilder<WithStorageDir> {
    pub fn start_url(self, url: impl Into<String>) -> CrawlConfigBuilder<WithStartUrl> {
        let url_string = url.into();

        // Normalize URL: add https:// if no scheme is present
        let normalized_url =
            if url_string.starts_with("http://") || url_string.starts_with("https://") {
                url_string
            } else {
                format!("https://{url_string}")
            };

        CrawlConfigBuilder {
            storage_dir: self.storage_dir,
            only_html: self.only_html,
            full_resources: self.full_resources,
            start_url: Some(normalized_url.clone()),
            limit: self.limit,
            screenshot_quality: self.screenshot_quality,
            stealth_mode: self.stealth_mode,
            allow_subdomains: self.allow_subdomains,
            allow_external_domains: self.allow_external_domains,
            save_screenshots: self.save_screenshots,
            save_json: self.save_json,
            save_raw_html: self.save_raw_html,
            save_markdown: self.save_markdown,
            headless: self.headless,
            content_selector: self.content_selector,
            allowed_domains: self.allowed_domains,
            excluded_patterns: self.excluded_patterns,
            generate_components: self.generate_components,
            progressive: self.progressive,
            presentation_style: self.presentation_style,
            max_depth: self.max_depth,
            search_index_dir: self.search_index_dir,
            search_memory_limit: self.search_memory_limit,
            search_batch_size: self.search_batch_size,
            crawl_rate_rps: self.crawl_rate_rps,
            max_inline_image_size_bytes: self.max_inline_image_size_bytes,
            max_deferred_queue_size: self.max_deferred_queue_size,
            enable_cache_validation: self.enable_cache_validation,
            ignore_cache: self.ignore_cache,
            cache_validation_timeout_secs: self.cache_validation_timeout_secs,
            page_load_timeout_secs: self.page_load_timeout_secs,
            navigation_timeout_secs: self.navigation_timeout_secs,
            event_timeout_secs: self.event_timeout_secs,
            circuit_breaker_enabled: self.circuit_breaker_enabled,
            circuit_breaker_failure_threshold: self.circuit_breaker_failure_threshold,
            circuit_breaker_retry_delay_secs: self.circuit_breaker_retry_delay_secs,
            max_concurrent_pages: self.max_concurrent_pages,
            max_concurrent_per_domain: self.max_concurrent_per_domain,
            compression_threshold_bytes: self.compression_threshold_bytes,
            max_page_retries: self.max_page_retries,
            _phantom: PhantomData,
        }
    }
}

// Build method only available when all required fields are set
impl CrawlConfigBuilder<WithStartUrl> {
    pub fn build(self) -> Result<CrawlConfig> {
        // Compile excluded patterns once at config creation
        let excluded_patterns_compiled = if let Some(ref patterns) = self.excluded_patterns {
            patterns
                .iter()
                .map(|p| compile_glob_pattern(p))
                .collect::<Result<Vec<_>>>()?
        } else {
            Vec::new()
        };

        // Enforce headless mode in release builds for production safety
        #[cfg(not(debug_assertions))]
        let headless = if !self.headless {
            // In release builds, override headed mode and force headless
            tracing::warn!(
                "Forcing headless mode in release build. \
                Headed mode is only available in debug builds for development."
            );
            true
        } else {
            self.headless
        };

        #[cfg(debug_assertions)]
        let headless = self.headless;

        Ok(CrawlConfig {
            storage_dir: self
                .storage_dir
                .ok_or_else(|| anyhow!("storage_dir is required"))?,
            start_url: self
                .start_url
                .ok_or_else(|| anyhow!("start_url is required"))?,
            only_html: self.only_html,
            full_resources: self.full_resources,
            limit: self.limit,
            screenshot_quality: self.screenshot_quality,
            stealth_mode: self.stealth_mode,
            allow_subdomains: self.allow_subdomains,
            allow_external_domains: self.allow_external_domains,
            save_screenshots: self.save_screenshots,
            save_json: self.save_json,
            save_raw_html: self.save_raw_html,
            save_markdown: self.save_markdown,
            headless,
            content_selector: self.content_selector,
            allowed_domains: self.allowed_domains,
            excluded_patterns: self.excluded_patterns,
            excluded_patterns_compiled,
            generate_components: self.generate_components,
            progressive: self.progressive,
            presentation_style: self.presentation_style,
            max_depth: self.max_depth,
            search_index_dir: self.search_index_dir,
            search_memory_limit: self.search_memory_limit,
            search_batch_size: self.search_batch_size,
            crawl_rate_rps: self.crawl_rate_rps,
            max_inline_image_size_bytes: self.max_inline_image_size_bytes,
            max_deferred_queue_size: self.max_deferred_queue_size,
            enable_cache_validation: self.enable_cache_validation,
            ignore_cache: self.ignore_cache,
            cache_validation_timeout_secs: self.cache_validation_timeout_secs,
            page_load_timeout_secs: self.page_load_timeout_secs,
            navigation_timeout_secs: self.navigation_timeout_secs,
            event_timeout_secs: self.event_timeout_secs,
            circuit_breaker_enabled: self.circuit_breaker_enabled,
            circuit_breaker_failure_threshold: self.circuit_breaker_failure_threshold,
            circuit_breaker_retry_delay_secs: self.circuit_breaker_retry_delay_secs,
            event_bus: None,
            indexing_sender: None,
            max_concurrent_pages: self.max_concurrent_pages,
            max_concurrent_per_domain: self.max_concurrent_per_domain,
            chrome_data_dir: None,
            browser_pool: None,
            compress_output: false, // Default to uncompressed
            compression_threshold_bytes: self.compression_threshold_bytes,
            max_page_retries: self.max_page_retries,
        })
    }
}

// Builder methods available at any state (since compression_threshold is optional)
impl<State> CrawlConfigBuilder<State> {
    /// Set compression threshold for spawn_blocking decision
    ///
    /// Content larger than this threshold will use `tokio::task::spawn_blocking()`
    /// to avoid blocking the async runtime. Smaller content is compressed directly.
    ///
    /// # Arguments
    /// * `bytes` - Threshold in bytes (recommended: 256KB to 10MB depending on hardware)
    ///
    /// # Example
    /// ```rust
    /// # use kodegen_tools_citescrape::config::CrawlConfig;
    /// # fn main() -> anyhow::Result<()> {
    /// let config = CrawlConfig::builder()
    ///     .storage_dir("./output")
    ///     .start_url("https://example.com")
    ///     .compression_threshold_bytes(5 * 1024 * 1024) // 5MB for high-perf server
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn compression_threshold_bytes(mut self, bytes: usize) -> Self {
        self.compression_threshold_bytes = Some(bytes);
        self
    }

    /// Set maximum page retry attempts for transient failures
    ///
    /// When a page fails due to timeout, network error, or browser crash,
    /// it will be retried up to this many times with exponential backoff.
    ///
    /// Set to 0 to disable page-level retries (errors become permanent).
    ///
    /// # Arguments
    /// * `retries` - Maximum number of retry attempts (default: 3)
    ///
    /// # Example
    /// ```rust
    /// # use kodegen_tools_citescrape::config::CrawlConfig;
    /// # fn main() -> anyhow::Result<()> {
    /// let config = CrawlConfig::builder()
    ///     .storage_dir("./output")
    ///     .start_url("https://example.com")
    ///     .max_page_retries(5)  // Allow up to 5 retries
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn max_page_retries(mut self, retries: u8) -> Self {
        self.max_page_retries = Some(retries);
        self
    }
}
