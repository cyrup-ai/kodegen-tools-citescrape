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
use bloomfilter::Bloom;
use chromiumoxide::browser::Browser;
use html2md;
use log::{debug, error, info, warn};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use super::crawl_types::CrawlQueue;
use super::{CircuitBreaker, extract_domain};
use super::link_processor::{CrawlState, process_page_links};
use super::page_timeout::with_page_timeout;
use crate::config::CrawlConfig;
use crate::content_saver;
use crate::content_saver::markdown_converter::{ConversionOptions, convert_html_to_markdown};
use crate::crawl_events::{CrawlEventBus, types::{CrawlEvent, PageCrawlMetadata}};
use crate::page_extractor;
use crate::page_extractor::link_rewriter::LinkRewriter;

/// Process a single page concurrently
///
/// This function handles all aspects of crawling a single URL:
/// 1. Apply rate limiting to avoid overwhelming servers
/// 2. Check circuit breaker to skip domains with repeated failures
/// 3. Create and enhance browser page with stealth features
/// 4. Navigate to URL and wait for page load
/// 5. Extract page data (HTML, metadata, links)
/// 6. Save content in requested formats (markdown, JSON, screenshots)
/// 7. Process discovered links and add them to crawl queue
/// 8. Publish crawl events for monitoring
/// 9. Update circuit breaker with success/failure status
///
/// # Arguments
/// * `browser` - Shared browser instance
/// * `item` - Current crawl queue item (URL + depth)
/// * `config` - Crawl configuration
/// * `link_rewriter` - Link rewriting manager for offline navigation
/// * `event_bus` - Optional event bus for progress tracking
/// * `circuit_breaker` - Optional circuit breaker for failure management
/// * `total_pages` - Atomic counter for total pages crawled
/// * `queue` - Thread-safe crawl queue for discovered links
/// * `indexing_sender` - Optional sender for search indexing
///
/// # Returns
/// * `Ok(String)` - Successfully crawled URL
/// * `Err` - Any error during page processing
#[allow(clippy::too_many_arguments)]
pub async fn process_single_page(
    browser: Arc<Browser>,
    item: CrawlQueue,
    config: CrawlConfig,
    link_rewriter: LinkRewriter,
    event_bus: Option<Arc<CrawlEventBus>>,
    circuit_breaker: Option<Arc<tokio::sync::Mutex<CircuitBreaker>>>,
    total_pages: Arc<AtomicUsize>,
    queue: Arc<tokio::sync::Mutex<VecDeque<CrawlQueue>>>,
    indexing_sender: Option<Arc<crate::search::IndexingSender>>,
) -> Result<String> {
    let page_start = Instant::now();

    // Apply rate limiting
    if let Some(rate) = config.crawl_rate_rps {
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
    if let Some(ref cb) = circuit_breaker {
        let domain = extract_domain(&item.url).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut cb_guard = cb.lock().await;
        if !cb_guard.should_attempt(&domain) {
            debug!("Circuit breaker OPEN, skipping: {}", item.url);
            return Err(anyhow::anyhow!("Circuit breaker open for domain"));
        }
    }

    info!("Crawling [depth {}]: {}", item.depth, item.url);

    // Create page
    let page = match browser.new_page("about:blank").await {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to create page for {}: {}", item.url, e);
            if let Some(ref cb) = circuit_breaker
                && let Ok(domain) = extract_domain(&item.url)
            {
                let mut cb_guard = cb.lock().await;
                cb_guard.record_failure(&domain, &e.to_string());
            }
            return Err(e.into());
        }
    };

    // Apply page enhancements
    match super::page_enhancer::enhance_page(page.clone()).await {
        Ok(()) => debug!("Page enhancements applied for: {}", item.url),
        Err(e) => warn!("Failed to apply page enhancements for {}: {}", item.url, e),
    }

    // Navigate to page
    let page_load_timeout = config.page_load_timeout_secs();
    if let Err(e) = with_page_timeout(
        async {
            page.goto(&item.url)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))
        },
        page_load_timeout,
        "Page navigation",
    )
    .await
    {
        warn!("Navigation failed for {}: {}", item.url, e);
        if let Some(ref cb) = circuit_breaker
            && let Ok(domain) = extract_domain(&item.url)
        {
            let mut cb_guard = cb.lock().await;
            cb_guard.record_failure(&domain, &e.to_string());
        }
        return Err(e);
    }

    // Wait for page load
    let navigation_timeout = config.navigation_timeout_secs();
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
        if let Some(ref cb) = circuit_breaker
            && let Ok(domain) = extract_domain(&item.url)
        {
            let mut cb_guard = cb.lock().await;
            cb_guard.record_failure(&domain, &e.to_string());
        }
        return Err(e);
    }

    // Extract page data
    let extract_config = crate::page_extractor::page_data::ExtractPageDataConfig {
        output_dir: config.storage_dir.clone(),
        link_rewriter: link_rewriter.clone(),
        max_inline_image_size_bytes: config.max_inline_image_size_bytes,
        crawl_rate_rps: config.crawl_rate_rps,
        save_html: config.save_raw_html(),
    };

    let page_data =
        page_extractor::extract_page_data(page.clone(), item.url.clone(), extract_config)
            .await
            .map_err(|e| {
                warn!("Failed to extract page data for {}: {}", item.url, e);
                e
            })?;

    let html_size = page_data.content.len();

    // Save markdown if requested
    if config.save_markdown() {
        let conversion_options = ConversionOptions::default();

        let processed_markdown =
            match convert_html_to_markdown(&page_data.content, &conversion_options).await {
                Ok(md) => md,
                Err(e) => {
                    warn!("Markdown conversion failed: {e}, using html2md fallback");
                    html2md::parse_html(&page_data.content)
                }
            };

        match content_saver::save_markdown_content(
            processed_markdown,
            item.url.clone(),
            config.storage_dir.clone(),
            crate::search::MessagePriority::Normal,
            indexing_sender.clone(),
            config.compress_output,
        )
        .await
        {
            Ok(()) => debug!("Markdown saved for {}", item.url),
            Err(e) => warn!("Failed to save markdown for {}: {}", item.url, e),
        }
    }

    // Save JSON if requested
    if config.save_json() {
        match content_saver::save_page_data(
            Arc::new(page_data.clone()),
            item.url.clone(),
            config.storage_dir.clone(),
        )
        .await
        {
            Ok(()) => debug!("Page data saved for {}", item.url),
            Err(e) => warn!("Failed to save page data for {}: {}", item.url, e),
        }
    }

    // Capture screenshot if requested
    let mut screenshot_captured = false;
    if config.save_screenshots() {
        match page_extractor::capture_screenshot(page.clone(), &item.url, config.storage_dir())
            .await
        {
            Ok(()) => {
                debug!("Screenshot saved for {}", item.url);
                screenshot_captured = true;
            }
            Err(e) => warn!("Failed to save screenshot for {}: {}", item.url, e),
        }
    }

    // Process page links - this needs thread-safe handling
    let links_found = {
        // Create temporary Bloom filter for compatibility with link processor
        let temp_bloom = match Bloom::new_for_fp_rate(10_000_000, 0.01) {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to create temp Bloom filter: {e}");
                return Ok(item.url);
            }
        };

        let q_snapshot = queue.lock().await.clone();
        let crawl_state = CrawlState {
            queue: q_snapshot,
            visited_urls: temp_bloom,
            max_depth: config.max_depth,
        };

        match process_page_links(page.clone(), item.clone(), crawl_state, &config).await {
            Ok((new_queue, _)) => {
                // Add new discovered links to shared queue
                let mut q = queue.lock().await;
                let mut added = 0;
                for new_item in new_queue {
                    if !q.iter().any(|existing| existing.url == new_item.url) {
                        q.push_back(new_item);
                        added += 1;
                    }
                }
                added
            }
            Err(e) => {
                warn!("Failed to process links for {}: {}", item.url, e);
                0
            }
        }
    };

    // Increment total pages counter
    total_pages.fetch_add(1, Ordering::Relaxed);

    // Record circuit breaker success
    if let Some(ref cb) = circuit_breaker
        && let Ok(domain) = extract_domain(&item.url)
    {
        let mut cb_guard = cb.lock().await;
        cb_guard.record_success(&domain);
    }

    // Publish PageCrawled event
    if let Some(bus) = &event_bus {
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
            &config.storage_dir,
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
                config
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

    Ok(item.url)
}
