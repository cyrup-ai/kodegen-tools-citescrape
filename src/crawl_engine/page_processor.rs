//! Single page processing logic
//!
//! Handles the complete lifecycle of processing a single web page:
//! - Rate limiting and circuit breaker checks
//! - Page navigation and loading
//! - Content extraction and conversion
//! - Screenshot capture
//! - Link discovery and queueing
//! - Event publishing

use anyhow::Result;
use chromiumoxide::browser::Browser;
use chromiumoxide::Page;
use dashmap::DashSet;
use chromiumoxide::cdp::browser_protocol::network::{
    EnableParams,
    EventResponseReceived,
};
use futures::StreamExt;
use log::{debug, error, warn};
use rand::Rng;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::content_validator::validate_page_content;
use super::crawl_types::CrawlQueue;
use super::{CircuitBreaker, extract_domain};
use super::page_timeout::with_page_timeout;
use crate::config::CrawlConfig;
use crate::content_saver;
use crate::content_saver::{read_cached_etag, check_etag_from_events};
use crate::content_saver::markdown_converter::{ConversionOptions, convert_html_to_markdown, ensure_h1_at_start};
use crate::crawl_events::{CrawlEventBus, types::{CrawlEvent, PageCrawlMetadata}};
use crate::link_rewriter::LinkRewriter;
use crate::page_extractor;

/// RAII guard for chromiumoxide Page that ensures proper cleanup
///
/// Provides two cleanup paths:
/// 1. Explicit async close() - preferred, allows error handling
/// 2. Drop fallback - spawns background cleanup task for error paths
///
/// # Why This Matters
///
/// chromiumoxide's Page has no Drop implementation and requires explicit
/// async close() to properly release CDP connections and browser resources.
/// Without cleanup, pages leak memory and connection handles, eventually
/// exhausting browser limits under concurrent load.
///
/// # Runtime Handle Storage
///
/// The guard captures a tokio::runtime::Handle at construction time to
/// ensure cleanup can always occur in Drop, even during panic unwinding
/// or runtime shutdown. This follows the RAII principle of "capture
/// dependencies at construction time" and eliminates panic risk from
/// missing runtime contexts.
///
/// # Usage
///
/// ```rust
/// let guard = PageGuard::new(browser.new_page("about:blank").await?, url);
/// 
/// // Use page via Deref
/// guard.goto("https://example.com").await?;
/// let content = guard.content().await?;
///
/// // Explicit close (preferred)
/// guard.close().await?;
/// 
/// // Or let Drop handle it on error paths (spawns background cleanup)
/// ```
pub struct PageGuard {
    page: Option<chromiumoxide::Page>,
    url: String,
    runtime_handle: tokio::runtime::Handle,
}

impl PageGuard {
    /// Create new guard wrapping a Page
    pub fn new(page: chromiumoxide::Page, url: String) -> Self {
        Self {
            page: Some(page),
            url,
            runtime_handle: tokio::runtime::Handle::current(),
        }
    }

    /// Explicitly close the page, consuming the guard
    ///
    /// This is the preferred cleanup path as it:
    /// - Properly awaits the async close operation
    /// - Allows error handling of close failures
    /// - Runs beforeunload hooks
    /// - Sends CDP Target.closeTarget command
    ///
    /// # Errors
    ///
    /// Returns error if CDP close command fails. Non-critical - page
    /// will still be cleaned up by browser, but may leave zombie resources.
    pub async fn close(mut self) -> Result<()> {
        if let Some(page) = self.page.take() {
            if let Err(e) = page.close().await {
                warn!("Failed to close page for {}: {}", self.url, e);
                // Non-fatal: browser will eventually clean up, but we tried
                return Err(e.into());
            } else {
                debug!("Page explicitly closed for {}", self.url);
            }
        }
        Ok(())
    }

    /// Get reference to inner page (for methods that need &Page)
    pub fn page(&self) -> &chromiumoxide::Page {
        self.page.as_ref().expect("PageGuard: page already consumed")
    }
}

impl Deref for PageGuard {
    type Target = chromiumoxide::Page;

    fn deref(&self) -> &Self::Target {
        self.page()
    }
}

impl Drop for PageGuard {
    fn drop(&mut self) {
        if let Some(page) = self.page.take() {
            let url = std::mem::take(&mut self.url);
            
            // Spawn fire-and-forget cleanup task
            // This ensures cleanup happens even on error paths, though we can't
            // await it from synchronous Drop. The tokio runtime will execute it.
            self.runtime_handle.spawn(async move {
                if let Err(e) = page.close().await {
                    // Just log - nothing we can do from Drop
                    log::warn!("PageGuard drop cleanup failed for {}: {}", url, e);
                } else {
                    log::trace!("PageGuard drop cleanup succeeded for {}", url);
                }
            });
        }
    }
}

/// Context for page processing containing shared crawler state
pub struct PageProcessorContext {
    pub config: CrawlConfig,
    pub link_rewriter: LinkRewriter,
    pub event_bus: Option<Arc<CrawlEventBus>>,
    pub circuit_breaker: Option<Arc<CircuitBreaker>>,
    pub total_pages: Arc<AtomicUsize>,
    pub queue: Arc<tokio::sync::Mutex<VecDeque<CrawlQueue>>>,
    pub indexing_sender: Option<Arc<crate::search::IndexingSender>>,
    pub visited: Arc<DashSet<String>>,
}

/// Navigate to a URL with timeout and circuit breaker error handling
///
/// This helper encapsulates the complete navigation workflow including:
/// - Timeout-wrapped page.goto() call with configurable duration
/// - Comprehensive error logging with URL context
/// - Circuit breaker failure recording for domain-level failure tracking
/// - Proper error propagation to caller
///
/// # Arguments
/// * `page` - Chromiumoxide Page instance to navigate
/// * `url` - Target URL to navigate to
/// * `timeout_secs` - Maximum navigation timeout in seconds
/// * `circuit_breaker` - Optional circuit breaker for failure tracking
///
/// # Returns
/// * `Ok(())` - Navigation succeeded within timeout
/// * `Err(anyhow::Error)` - Navigation failed (timeout, network error, etc.)
///
/// # Error Handling
/// On navigation failure:
/// 1. Logs warning with URL and error details
/// 2. Records failure in circuit breaker (if available) for domain
/// 3. Propagates error to caller for handling
///
/// # Example
/// ```ignore
/// navigate_to_page(
///     &page,
///     "https://example.com",
///     30,
///     &ctx.circuit_breaker
/// ).await?;
/// ```
async fn navigate_to_page(
    page: &Page,
    url: &str,
    timeout_secs: u64,
    circuit_breaker: &Option<Arc<CircuitBreaker>>,
) -> Result<()> {
    if let Err(e) = with_page_timeout(
        async {
            page.goto(url)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        },
        timeout_secs,
        "Page navigation",
    )
    .await
    {
        warn!("Navigation failed for {}: {}", url, e);
        if let Some(cb) = circuit_breaker
            && let Ok(domain) = extract_domain(url)
        {
            cb.record_failure(&domain, &e.to_string());
        }
        return Err(e);
    }
    Ok(())
}

/// Process a single page concurrently
///
/// This function handles all aspects of crawling a single URL:
/// 1. Apply rate limiting to avoid overwhelming servers
/// 2. Check circuit breaker to skip domains with repeated failures
/// 3. Create and enhance browser page with stealth features
/// 4. Navigate to URL and wait for page load
/// 5. Extract page data (HTML, metadata, links)
/// 6. Save content in requested formats (markdown, JSON, screenshots)
/// 7. Process discovered links and add them to crawl ctx.queue
/// 8. Publish crawl events for monitoring
/// 9. Update circuit breaker with success/failure status
///
/// # Arguments
/// * `browser` - Shared browser instance
/// * `item` - Current crawl ctx.queue item (URL + depth)
/// * `ctx` - Page processor context containing crawler state and configuration
///
/// # Returns
/// * `Ok(String)` - Successfully crawled URL
/// * `Err` - Any error during page processing
pub async fn process_single_page(
    browser: Arc<Browser>,
    item: CrawlQueue,
    ctx: PageProcessorContext,
) -> Result<String> {
    let page_start = Instant::now();

    // Apply rate limiting
    if let Some(rate) = ctx.config.crawl_rate_rps {
        use super::rate_limiter::{RateLimitDecision, check_crawl_rate_limit};
        match check_crawl_rate_limit(&item.url, rate).await {
            RateLimitDecision::Deny { retry_after } => {
                debug!("Rate limited, sleeping for {:?}: {}", retry_after, item.url);
                tokio::time::sleep(retry_after).await;
            }
            RateLimitDecision::Allow => {}
        }
    }

    // Check circuit breaker
    if let Some(ref cb) = ctx.circuit_breaker {
        let domain = extract_domain(&item.url).map_err(|e| anyhow::anyhow!("{e}"))?;
        if !cb.should_attempt(&domain) {
            debug!("Circuit breaker OPEN, skipping: {}", item.url);
            return Err(anyhow::anyhow!("Circuit breaker open for domain"));
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // CACHE CHECK: Read cached ETag (if exists) for later comparison
    // ═══════════════════════════════════════════════════════════════
    let cached_etag: Option<String> = match read_cached_etag(&item.url, &ctx.config.storage_dir).await {
        Ok(Some(etag)) => {
            debug!("Found cached ETag for {}: {}", item.url, etag);
            Some(etag)
        }
        Ok(None) => {
            debug!("No cached content for {}", item.url);
            None
        }
        Err(e) => {
            // Non-fatal: proceed without cache check
            warn!("Failed to read cached ETag for {}: {}", item.url, e);
            None
        }
    };

    debug!("Crawling [depth {}]: {}", item.depth, item.url);

    // Create page - wrap in RAII guard for automatic cleanup
    let page_guard = match browser.new_page("about:blank").await {
        Ok(p) => PageGuard::new(p, item.url.clone()),
        Err(e) => {
            warn!("Failed to create page for {}: {}", item.url, e);
            if let Some(ref cb) = ctx.circuit_breaker
                && let Ok(domain) = extract_domain(&item.url)
            {
                cb.record_failure(&domain, &e.to_string());
            }
            return Err(e.into());
        }
    };

    // Apply page enhancements
    match super::page_enhancer::enhance_page(page_guard.page().clone()).await {
        Ok(()) => debug!("Page enhancements applied for: {}", item.url),
        Err(e) => warn!("Failed to apply page enhancements for {}: {}", item.url, e),
    }

    // Extract page reference for subsequent operations
    let page = page_guard.page();

    // Enable Network domain to receive network events
    if let Err(e) = page.execute(EnableParams::default()).await {
        warn!("Failed to enable Network domain for {}: {}", item.url, e);
    }

    // ═══════════════════════════════════════════════════════════════
    // NETWORK EVENT HANDLING: HTTP status capture + ETag cache check
    // ═══════════════════════════════════════════════════════════════
    let (http_status, cache_hit) = if let Some(ref expected_etag) = cached_etag {
        // ─────────────────────────────────────────────────────────────
        // CACHE CHECK PATH: Use check_etag_from_events for cache validation
        // ─────────────────────────────────────────────────────────────
        match page.event_listener::<EventResponseReceived>().await {
            Ok(mut response_events) => {
                // Navigate to page first (cache check happens during navigation)
                let page_load_timeout = ctx.config.page_load_timeout_secs();
                navigate_to_page(&page_guard, &item.url, page_load_timeout, &ctx.circuit_breaker).await?;

                // Use check_etag_from_events which handles multiple Document resources
                // by matching normalized URLs (not just "first Document")
                let etag_match = check_etag_from_events(
                    &mut response_events,
                    &item.url,
                    expected_etag,
                    Duration::from_secs(ctx.config.page_load_timeout_secs()),
                ).await;

                if etag_match {
                    debug!("Cache HIT: ETag matches for {}", item.url);
                    (None, true)
                } else {
                    debug!("Cache MISS: ETag mismatch for {}", item.url);
                    (None, false)
                }
            }
            Err(e) => {
                warn!("Failed to subscribe to ResponseReceived events for {}: {}", item.url, e);
                
                // Navigate to page anyway (fallback without cache check)
                let page_load_timeout = ctx.config.page_load_timeout_secs();
                navigate_to_page(&page_guard, &item.url, page_load_timeout, &ctx.circuit_breaker).await?;
                
                (None, false) // No cache check possible, proceed with full processing
            }
        }
    } else {
        // ─────────────────────────────────────────────────────────────
        // STANDARD PATH: HTTP status capture (no cached ETag to compare)
        // ─────────────────────────────────────────────────────────────
        let status = match page.event_listener::<EventResponseReceived>().await {
            Ok(mut response_events) => {
                // Create channel to capture HTTP status from background task
                let (status_tx, status_rx) = tokio::sync::oneshot::channel::<u16>();

                // Spawn background task and STORE the JoinHandle for cleanup
                let target_url = item.url.clone();
                let status_task_handle = tokio::spawn(async move {
                    let mut response_count = 0;
                    
                    // Internal timeout - guarantees task exits within 10 seconds
                    let timeout = tokio::time::sleep(Duration::from_secs(10));
                    tokio::pin!(timeout);
                    
                    loop {
                        tokio::select! {
                            // Branch 1: Process next CDP event
                            Some(event) = response_events.next() => {
                                response_count += 1;
                                
                                // STRATEGY: Capture the FIRST main document response
                                // This handles redirects because we don't match exact URL
                                //
                                // Chrome CDP guarantees the first Document-type response
                                // is the navigation response (even after redirects)
                                let is_main_document = {
                                    // Check 1: Response has Document mime type (text/html, application/xhtml+xml)
                                    let mime = event.response.mime_type.to_lowercase();
                                    let is_html = mime.starts_with("text/html") || 
                                                  mime.starts_with("application/xhtml+xml");
                                    
                                    // Check 2: This is the first document response we've seen
                                    // (first matching response is always the navigation)
                                    let is_first = response_count == 1 || 
                                                   (is_html && response_count <= 3); // Allow up to 3 redirects
                                    
                                    is_html && is_first
                                };
                                
                                if is_main_document {
                                    let status = event.response.status as u16;
                                    debug!(
                                        "Captured HTTP status {} for navigation (url: {}, mime: {})",
                                        status,
                                        event.response.url,
                                        event.response.mime_type
                                    );
                                    let _ = status_tx.send(status);
                                    break; // Exit: Found main document response
                                }
                                
                                // Log skipped subresources (for debugging redirect issues)
                                if response_count <= 5 {
                                    debug!(
                                        "Skipped response #{}: url={}, mime={}, status={}",
                                        response_count,
                                        event.response.url,
                                        event.response.mime_type,
                                        event.response.status
                                    );
                                }
                            }
                            
                            // Branch 2: Internal timeout expires
                            _ = &mut timeout => {
                                debug!(
                                    "HTTP status capture internal timeout after {} responses for {}",
                                    response_count,
                                    target_url
                                );
                                break; // Exit: Timeout - no valid response within 10 seconds
                            }
                        }
                    }
                    
                    // Task guaranteed to exit via one of three paths:
                    // 1. Found main document response → break in select! branch 1
                    // 2. Stream closed (None) → select! branch 1 pattern doesn't match, loop exits
                    // 3. 10-second timeout → break in select! branch 2
                    debug!(
                        "HTTP status capture task exiting after {} responses for {}",
                        response_count,
                        target_url
                    );
                });

                // Navigate to page with timeout and circuit breaker error handling
                let page_load_timeout = ctx.config.page_load_timeout_secs();
                if let Err(e) = navigate_to_page(&page_guard, &item.url, page_load_timeout, &ctx.circuit_breaker).await {
                    // CRITICAL: Abort the status capture task
                    status_task_handle.abort();
                    return Err(e);
                }

                // Wait for HTTP status (with timeout to avoid blocking)
                // The timeout is for waiting on the channel, not for the task itself
                let status_result = tokio::time::timeout(
                    Duration::from_secs(5), 
                    status_rx
                ).await;
                
                match status_result {
                    Ok(Ok(status)) => {
                        debug!("HTTP status captured: {} for {}", status, item.url);
                        Some(status)
                    }
                    Ok(Err(_)) => {
                        debug!(
                            "HTTP status channel closed for {} (task exited without finding response)",
                            item.url
                        );
                        // Task should have already exited, but abort for safety
                        status_task_handle.abort();
                        None
                    }
                    Err(_timeout_elapsed) => {
                        debug!(
                            "HTTP status capture timeout for {} (task should exit via internal timeout)",
                            item.url
                        );
                        // Task has internal 10s timeout and should exit on its own,
                        // but abort as defensive programming
                        status_task_handle.abort();
                        None
                    }
                }
            }
            Err(e) => {
                warn!("Failed to subscribe to ResponseReceived events for {}: {}", item.url, e);
                
                // Navigate to page anyway (fallback without HTTP status capture)
                let page_load_timeout = ctx.config.page_load_timeout_secs();
                navigate_to_page(&page_guard, &item.url, page_load_timeout, &ctx.circuit_breaker).await?;
                
                None // No HTTP status available in fallback path
            }
        };
        (status, false) // No cache hit in standard path
    };

    // ═══════════════════════════════════════════════════════════════
    // EARLY RETURN ON CACHE HIT
    // ═══════════════════════════════════════════════════════════════
    if cache_hit {
        debug!("Skipping page processing due to cache hit: {}", item.url);
        
        // Still increment counter and record success
        ctx.total_pages.fetch_add(1, Ordering::Relaxed);
        if let Some(ref cb) = ctx.circuit_breaker
            && let Ok(domain) = extract_domain(&item.url)
        {
            cb.record_success(&domain);
        }
        
        // Publish cache hit event for monitoring
        if let Some(bus) = &ctx.event_bus {
            let event = CrawlEvent::cache_hit(item.url.clone());
            if let Err(e) = bus.publish(event).await {
                warn!("Failed to publish CacheHit event for {}: {}", item.url, e);
            }
        }
        
        // Close page and return success
        if let Err(e) = page_guard.close().await {
            warn!("Failed to close page for {}: {} (non-fatal)", item.url, e);
        }
        return Ok(item.url);
    }

    // Wait for page load
    let navigation_timeout = ctx.config.navigation_timeout_secs();
    if let Err(e) = with_page_timeout(
        async {
            page.wait_for_navigation()
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        },
        navigation_timeout,
        "Page load",
    )
    .await
    {
        warn!("Page load failed for {}: {}", item.url, e);
        if let Some(ref cb) = ctx.circuit_breaker
            && let Ok(domain) = extract_domain(&item.url)
        {
            cb.record_failure(&domain, &e.to_string());
        }
        return Err(e);
    }

    // Retry configuration constants
    const MAX_RETRIES: u32 = 3;
    const INITIAL_BACKOFF_MS: u64 = 1000; // 1 second
    const BACKOFF_MULTIPLIER: u64 = 2; // Exponential: 1s, 2s, 4s
    const JITTER_PERCENT: f64 = 0.2; // ±20% randomness to prevent thundering herd

    // Calculate exponential backoff with jitter
    let calculate_backoff_duration = |attempt: u32| -> Duration {
        let base_delay = INITIAL_BACKOFF_MS * BACKOFF_MULTIPLIER.pow(attempt);
        
        // Add jitter: ±20% randomness
        let jitter = rand::rng().random_range(-JITTER_PERCENT..=JITTER_PERCENT);
        let jittered_delay = (base_delay as f64 * (1.0 + jitter)) as u64;
        
        Duration::from_millis(jittered_delay)
    };

    // Extract page data with retry logic and content validation
    let mut page_data = None;
    let mut processed_markdown = None;

    let extract_config = crate::page_extractor::page_data::ExtractPageDataConfig {
        output_dir: ctx.config.storage_dir.clone(),
        max_inline_image_size_bytes: ctx.config.max_inline_image_size_bytes,
        crawl_rate_rps: ctx.config.crawl_rate_rps,
        save_html: ctx.config.save_raw_html(),
        compression_threshold_bytes: ctx.config.compression_threshold_bytes(),
    };

    for attempt in 0..MAX_RETRIES {
        debug!(
            "Attempt {}/{} to extract page data for: {}",
            attempt + 1,
            MAX_RETRIES,
            item.url
        );

        let extracted_data = match page_extractor::extract_page_data(
            page_guard.page().clone(),
            item.url.clone(),
            &extract_config,
        )
        .await
        {
            Ok(data) => data,
            Err(e) => {
                warn!(
                    "Attempt {}/{} failed to extract page data for {}: {}",
                    attempt + 1,
                    MAX_RETRIES,
                    item.url,
                    e
                );
                
                // Retry with backoff if not last attempt
                if attempt < MAX_RETRIES - 1 {
                    let backoff = calculate_backoff_duration(attempt);
                    debug!("Retrying after {:?}", backoff);
                    tokio::time::sleep(backoff).await;
                    continue;
                } else {
                    // All retries exhausted for extraction
                    if let Some(ref cb) = ctx.circuit_breaker
                        && let Ok(domain) = extract_domain(&item.url)
                    {
                        cb.record_failure(&domain, &e.to_string());
                    }
                    return Err(e);
                }
            }
        };

        // Convert HTML to markdown
        let conversion_options = ConversionOptions {
            base_url: Some(item.url.clone()),
            ..ConversionOptions::default()
        };

        let markdown = match convert_html_to_markdown(&extracted_data.content, &conversion_options).await {
            Ok(md) => md,
            Err(e) => {
                warn!(
                    "Attempt {}/{} markdown conversion failed for {}: {}, using htmd fallback",
                    attempt + 1,
                    MAX_RETRIES,
                    item.url,
                    e
                );
                htmd::convert(&extracted_data.content).unwrap_or_default()
            }
        };

        // CRITICAL: Validate content BEFORE saving
        let validation = validate_page_content(
            &extracted_data.content,
            &markdown,
            &item.url,
            http_status, // Pass captured HTTP status
        );

        if validation.is_valid {
            debug!(
                "Content validation passed for {} on attempt {}/{}",
                item.url,
                attempt + 1,
                MAX_RETRIES
            );
            page_data = Some(extracted_data);
            processed_markdown = Some(markdown);
            break; // Success - exit retry loop
        } else {
            warn!(
                "Attempt {}/{} content validation failed for {}: {} (confidence: {:.2})",
                attempt + 1,
                MAX_RETRIES,
                item.url,
                validation.reason.as_ref().unwrap_or(&"Unknown".to_string()),
                validation.confidence
            );
            // Retry with exponential backoff if not last attempt
            if attempt < MAX_RETRIES - 1 {
                let backoff = calculate_backoff_duration(attempt);
                debug!(
                    "Content invalid, retrying {} after {:?}",
                    item.url, backoff
                );
                tokio::time::sleep(backoff).await;
                // Continue to next attempt
            } else {
                // All retries exhausted - record failure
                warn!(
                    "All {} retry attempts exhausted for {}: {}",
                    MAX_RETRIES,
                    item.url,
                    validation.reason.as_ref().unwrap_or(&"Unknown error".to_string())
                );

                if let Some(ref cb) = ctx.circuit_breaker
                    && let Ok(domain) = extract_domain(&item.url)
                {
                    cb.record_failure(
                        &domain,
                        &format!(
                            "Content validation failed after {} retries: {}",
                            MAX_RETRIES,
                            validation.reason.as_ref().unwrap_or(&"Unknown".to_string())
                        ),
                    );
                }

                return Err(anyhow::anyhow!(
                    "Content validation failed after {} retries: {}",
                    MAX_RETRIES,
                    validation.reason.unwrap_or_else(|| "Unknown error".to_string())
                ));
            }
        }
    }

    // Unwrap validated data (guaranteed to be Some if we reach here)
    let mut page_data = page_data.ok_or_else(|| {
        anyhow::anyhow!("Failed to extract valid page data after {} retries", MAX_RETRIES)
    })?;
    let processed_markdown = processed_markdown.ok_or_else(|| {
        anyhow::anyhow!("Failed to produce valid markdown after {} retries", MAX_RETRIES)
    })?;

    // Ensure markdown starts with H1 (using extracted headings or title as fallback)
    let processed_markdown = ensure_h1_at_start(
        &processed_markdown,
        &page_data.metadata.headings,
        &page_data.title,
    );

    let html_size = page_data.content.len();

    // EVENT-DRIVEN LINK REWRITING: Register page and rewrite links
    // This happens AFTER HTML is saved to disk (in extract_page_data)
    if ctx.config.save_raw_html() {
        // Get the local path where HTML was saved
        if let Ok(local_path) = crate::utils::get_mirror_path(&item.url, &ctx.config.storage_dir, "index.html").await {
            // Extract outbound links from the HTML content
            let outbound_links = crate::link_rewriter::extract_links_from_html(&page_data.content, &item.url);

            // Trigger event-driven link rewriting
            match ctx.link_rewriter.on_page_saved(&item.url, &local_path, outbound_links).await {
                Ok(result) => {
                    if result.outbound_rewritten > 0 || result.inbound_updated > 0 {
                        debug!(
                            "Link rewriting for {}: {} outbound rewritten, {} inbound pages updated",
                            item.url, result.outbound_rewritten, result.inbound_updated
                        );
                    }
                }
                Err(e) => {
                    warn!("Link rewriting failed for {}: {}", item.url, e);
                    // Non-fatal: page is saved, just links weren't rewritten
                }
            }
        }
    }

    // Save markdown if requested (only executed if validation passed)
    if ctx.config.save_markdown() {
        match content_saver::save_markdown_content(
            processed_markdown,
            item.url.clone(),
            ctx.config.storage_dir.clone(),
            crate::search::MessagePriority::Normal,
            ctx.indexing_sender.clone(),
            ctx.config.compress_output,
            ctx.config.compression_threshold_bytes(),
        )
        .await
        {
            Ok(()) => debug!("Markdown saved for {}", item.url),
            Err(e) => warn!("Failed to save markdown for {}: {}", item.url, e),
        }
    }

    // Extract links before page_data is potentially moved to save_page_data()
    let extracted_links = std::mem::take(&mut page_data.links);
    
    // Save JSON if requested (only executed if validation passed)
    if ctx.config.save_json() {
        // Restore links for JSON serialization, then move page_data
        page_data.links = extracted_links.clone();
        match content_saver::save_page_data(
            page_data,
            item.url.clone(),
            ctx.config.storage_dir.clone(),
            ctx.config.compression_threshold_bytes(),
        )
        .await
        {
            Ok(()) => debug!("Page data saved for {}", item.url),
            Err(e) => warn!("Failed to save page data for {}: {}", item.url, e),
        }
    }

    // Capture screenshot if requested
    let mut screenshot_captured = false;
    if ctx.config.save_screenshots() {
        match page_extractor::capture_screenshot(
            page_guard.page().clone(),
            &item.url,
            ctx.config.storage_dir(),
            ctx.config.compression_threshold_bytes(),
        )
        .await
        {
            Ok(()) => {
                debug!("Screenshot saved for {}", item.url);
                screenshot_captured = true;
            }
            Err(e) => warn!("Failed to save screenshot for {}: {}", item.url, e),
        }
    }

    // Process page links and add discovered URLs to the crawl queue
    // Uses already-extracted links from page_data instead of re-extracting from browser
    let links_found = {
        use std::collections::HashSet;
        
        let new_links: Vec<CrawlQueue> = if item.depth < ctx.config.max_depth {
            let filtered_urls = super::crawler::extract_valid_urls(&extracted_links, &ctx.config);
            
            debug!(
                target: "citescrape::links",
                "Found {} links on {}, {} after filtering",
                extracted_links.len(),
                item.url,
                filtered_urls.len()
            );
            
            // Batch deduplication within this page's links
            let mut seen_in_batch: HashSet<String> = HashSet::new();
            let mut results = Vec::new();
            
            for link_url in filtered_urls {
                let normalized_url = crate::link_index::normalize_url(&link_url);
                
                // Skip duplicates within this batch
                if !seen_in_batch.insert(normalized_url.clone()) {
                    continue;
                }
                
                if url::Url::parse(&normalized_url).is_ok() {
                    results.push(CrawlQueue {
                        url: normalized_url,
                        depth: item.depth + 1,
                    });
                }
            }
            results
        } else {
            Vec::new()
        };
        
        let added = new_links.len();
        let mut actually_queued = 0;
        
        if !new_links.is_empty() {
            let mut q = ctx.queue.lock().await;
            for new_link in new_links {
                // Skip if URL already visited (orchestrator handles insert at dequeue time)
                if !ctx.visited.contains(&new_link.url) {
                    q.push_back(new_link);
                    actually_queued += 1;
                }
            }
            debug!(
                target: "citescrape::links",
                "Queued {} new URLs (out of {} filtered), queue size now: {}",
                actually_queued,
                added,
                q.len()
            );
        }
        
        added
    };

    // Increment total pages counter
    ctx.total_pages.fetch_add(1, Ordering::Relaxed);

    // Record circuit breaker success
    if let Some(ref cb) = ctx.circuit_breaker
        && let Ok(domain) = extract_domain(&item.url)
    {
        cb.record_success(&domain);
    }

    // Publish PageCrawled event
    if let Some(bus) = &ctx.event_bus {
        let metadata = PageCrawlMetadata {
            html_size,
            compressed_size: 0,
            links_found,
            links_for_crawling: links_found,
            screenshot_captured,
            processing_duration: page_start.elapsed(),
        };

        let local_path = match crate::content_saver::get_mirror_path_sync(
            &item.url,
            &ctx.config.storage_dir,
            "index.md",
        ) {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to compute local path for {}: {}", item.url, e);
                let url_hash = {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    item.url.hash(&mut hasher);
                    hasher.finish()
                };
                ctx.config
                    .storage_dir
                    .join("parse-failed")
                    .join(format!("{url_hash}.md"))
            }
        };

        let event = CrawlEvent::page_crawled(
            item.url.clone(),
            local_path,
            u32::from(item.depth),
            metadata,
        );

        if let Err(e) = bus.publish(event).await {
            warn!(
                "Failed to publish PageCrawled event for {}: {}",
                item.url, e
            );
        }
    }

    // Explicitly close page before returning success
    // This is the preferred cleanup path - awaits CDP close command
    if let Err(e) = page_guard.close().await {
        warn!("Failed to close page for {}: {} (non-fatal)", item.url, e);
        // Continue - page will be cleaned up by browser, this is best-effort
    }

    Ok(item.url)
}
