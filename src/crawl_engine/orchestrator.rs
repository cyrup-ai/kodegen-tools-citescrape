//! Main crawl orchestration logic
//!
//! Coordinates multi-page crawling with:
//! - Browser lifecycle management
//! - Concurrent task execution
//! - Queue and concurrency control
//! - Event publishing and monitoring
//! - Graceful shutdown and cleanup

use anyhow::{Context, Result};
use chromiumoxide::Browser;
use dashmap::{DashMap, DashSet};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use log::{debug, error, info, warn};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use super::crawl_types::{CrawlQueue, FailureKind};
use rand::Rng;
use super::{CircuitBreaker, DomainLimiter, extract_domain};
use super::page_processor::{PageProcessorContext, PageResult, process_single_page};
use super::retry_queue::RetryQueue;
use super::progress::ProgressReporter;
use crate::browser_setup::launch_browser;
use crate::config::CrawlConfig;
use crate::crawl_events::{
    CrawlEventBus,
    types::CrawlEvent,
};
use crate::link_rewriter::LinkRewriter;

/// Calculate exponential backoff delay with jitter for page retries
///
/// Formula: base_delay * 2^(attempt-1) * failure_multiplier * (1 ± jitter)
fn calculate_retry_backoff(retry_count: u8, failure_kind: FailureKind) -> std::time::Duration {
    const BASE_DELAY_MS: u64 = 1000;  // 1 second
    const MAX_DELAY_MS: u64 = 30_000; // 30 seconds cap
    const JITTER_PERCENT: f64 = 0.2;  // ±20%
    
    // Exponential: 1s, 2s, 4s, 8s, 16s...
    let exp_delay = BASE_DELAY_MS.saturating_mul(1 << retry_count.min(5));
    
    // Apply failure kind multiplier
    let adjusted_delay = (exp_delay as f64 * failure_kind.delay_multiplier()) as u64;
    
    // Apply jitter to prevent thundering herd (using rand 0.9+ API)
    let jitter = rand::rng().random_range(-JITTER_PERCENT..=JITTER_PERCENT);
    let jittered_delay = (adjusted_delay as f64 * (1.0 + jitter)) as u64;
    
    // Cap at maximum
    std::time::Duration::from_millis(jittered_delay.min(MAX_DELAY_MS))
}

/// Main crawl orchestration with event bus integration
///
/// This function implements the complete multi-page crawling logic:
/// - Browser initialization
/// - Breadth-first queue-based crawling with depth control
/// - Page processing and content extraction
/// - Link discovery and filtering
/// - Progress reporting via `ProgressReporter` trait
/// - Event publishing via optional `CrawlEventBus`
///
/// # Arguments
/// * `config` - Crawl configuration
/// * `link_rewriter` - Link rewriting manager
/// * `chrome_data_dir` - Optional Chrome data directory
/// * `progress` - Progress reporter (`NoOpProgress`)
/// * `event_bus` - Optional event bus for crawl events
pub async fn crawl_pages<P: ProgressReporter>(
    config: CrawlConfig,
    link_rewriter: LinkRewriter,
    _chrome_data_dir: Option<PathBuf>,  // Deprecated parameter, kept for API compatibility
    progress: P,
    event_bus: Option<Arc<CrawlEventBus>>,
) -> Result<Option<PathBuf>> {
    // Extract indexing_sender early to avoid borrow issues
    let indexing_sender = config.indexing_sender().map(Arc::clone);

    let start_time = Instant::now();

    progress.report_initializing();

    // Initialize thread-safe crawl queue
    let queue = Arc::new(tokio::sync::Mutex::new({
        let mut q = VecDeque::new();
        q.push_back(CrawlQueue {
            url: config.start_url.clone(),
            depth: 0,
            retry_count: 0,
        });
        q
    }));

    // Lock-free visited set (replaces Bloom filter for thread-safety)
    // DashSet provides concurrent access without locks, ideal for multi-task crawling
    let visited: Arc<DashSet<String>> = Arc::new(DashSet::new());

    // Thread-safe circuit breaker for domain-level failure detection
    let circuit_breaker = if config.circuit_breaker_enabled() {
        Some(Arc::new(CircuitBreaker::new(
            config.circuit_breaker_failure_threshold(),
            2, // success_threshold: close circuit after 2 consecutive successes
            Duration::from_secs(config.circuit_breaker_retry_delay_secs()),
        )))
    } else {
        None
    };

    // Retry queue for circuit-breaker-rejected items
    let retry_queue = circuit_breaker
        .as_ref()
        .map(|cb| Arc::new(RetryQueue::new(Arc::clone(cb))));

    // Atomic counter for total pages crawled
    let total_pages = Arc::new(AtomicUsize::new(0));

    // Publish CrawlStarted event (CRITICAL - fail if this doesn't work)
    if let Some(bus) = &event_bus {
        let event = CrawlEvent::crawl_started(
            config.start_url.clone(),
            config.storage_dir.clone(),
            u32::from(config.max_depth),
        );
        bus.publish(event)
            .await
            .context("Failed to publish CrawlStarted event - event bus may be shutdown or full")?;
    }

    // Setup browser - either from pool (instant) or launch fresh (2-5 second cold start)
    //
    // Pool mode: Acquire pre-warmed browser from pool, returns to pool when done
    // Fresh mode: Launch new browser, clean up temp directory when done
    //
    // launch_browser handles:
    // - Finding Chrome/Chromium executable (system paths + 'which' command)
    // - Downloading managed Chromium if not found (with proper dir creation)
    // - Configuring browser with stealth mode arguments
    // - Spawning handler task to drive CDP connection
    // - Using unique chrome_data_dir per session (if configured) to prevent profile lock contention

    // Track whether we're using a pooled browser (for cleanup logic)
    let pool_guard: Option<crate::browser_pool::PooledBrowserGuard>;
    let handler_task: Option<tokio::task::JoinHandle<()>>;
    let chrome_data_dir_path: PathBuf;
    let browser: Arc<Browser>;

    if let Some(pool) = config.browser_pool() {
        // Pool mode: acquire pre-warmed browser
        match pool.acquire().await {
            Ok(guard) => {
                info!("Acquired pre-warmed browser from pool (id={})", guard.id());
                browser = guard.browser_arc();
                chrome_data_dir_path = std::env::temp_dir()
                    .join(format!("kodegen_citescrape_pooled_{}", std::process::id()));
                handler_task = None; // Pool manages the handler
                pool_guard = Some(guard);
            }
            Err(e) => {
                warn!("Failed to acquire from pool, launching fresh browser: {}", e);
                // Fall back to fresh browser launch
                let chrome_dir_param = config.chrome_data_dir().cloned();
                let (fresh_browser, fresh_handler, fresh_dir) = launch_browser(config.headless(), chrome_dir_param)
                    .await
                    .context("Failed to launch browser")?;
                browser = Arc::new(fresh_browser);
                handler_task = Some(fresh_handler);
                chrome_data_dir_path = fresh_dir;
                pool_guard = None;
            }
        }
    } else {
        // Fresh mode: launch new browser
        let chrome_dir_param = config.chrome_data_dir().cloned();
        let (fresh_browser, fresh_handler, fresh_dir) = launch_browser(config.headless(), chrome_dir_param)
            .await
            .context("Failed to launch browser")?;
        browser = Arc::new(fresh_browser);
        handler_task = Some(fresh_handler);
        chrome_data_dir_path = fresh_dir;
        pool_guard = None;
    }

    // Extract user agent from browser for HTTP requests during resource inlining
    // This ensures HTTP requests use the same User-Agent as the browser,
    // preventing 403 Forbidden errors from servers that block non-browser requests
    let user_agent = browser.user_agent().await
        .context("Failed to get user agent from browser")?;
    debug!("Browser user agent: {user_agent}");

    // Session-level HTTP error cache for static asset downloads
    // This cache persists across all pages in this crawl session, preventing
    // repeated failed requests to the same broken URLs (404s, 400s, etc.)
    let http_error_cache: Arc<DashMap<String, u16>> = Arc::new(DashMap::new());

    // Session-level domain download queues for static asset downloads
    // Sharing queues across pages ensures ONE worker per domain (serial downloads)
    // which makes the http_error_cache effective (cache checked before each download)
    let domain_queues: Arc<DashMap<String, Arc<crate::inline_css::domain_queue::DomainDownloadQueue>>> = Arc::new(DashMap::new());

    progress.report_browser_launched();

    // Browser is already Arc-wrapped (either from pool or fresh launch above)

    // Concurrency control
    let concurrency = config.max_concurrent_pages();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let domain_limiter = Arc::new(DomainLimiter::new(config.max_concurrent_per_domain()));

    // Main concurrent crawl loop
    let mut active_tasks = FuturesUnordered::new();

    loop {
        // Check retry queue for items ready to re-process
        if let Some(ref rq) = retry_queue {
            let ready_items = rq.drain_ready();
            if !ready_items.is_empty() {
                let mut q = queue.lock().await;
                for item in ready_items {
                    // Only re-queue if not already visited
                    if !visited.contains(&item.url) {
                        q.push_back(item);
                    }
                }
            }
        }

        // Fill up to concurrency limit
        while active_tasks.len() < concurrency {
            // Pop next item from queue
            let item = {
                let mut q = queue.lock().await;
                match q.pop_front() {
                    Some(item) => item,
                    None if active_tasks.is_empty() => break, // All done
                    None => break,                            // Wait for tasks to complete
                }
            };

            // Check limit
            if let Some(limit) = config.limit
                && total_pages.load(Ordering::Relaxed) >= limit
            {
                info!("Reached page limit of {limit}");
                break;
            }

            // Check if already visited (lock-free check)
            if !visited.insert(item.url.clone()) {
                continue; // Already visited
            }

            // Acquire global semaphore permit (limits total concurrency)
            let permit = if let Ok(p) = semaphore.clone().acquire_owned().await {
                p
            } else {
                error!("Semaphore closed unexpectedly");
                continue;
            };

            // Acquire domain-specific permit (prevents rate limiting)
            let domain = match extract_domain(&item.url) {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to extract domain from {}: {}", item.url, e);
                    continue;
                }
            };
            let domain_permit = domain_limiter.acquire(domain).await;

            // Clone all shared state for the task
            let browser = Arc::clone(&browser);
            let config = config.clone();
            let link_rewriter = link_rewriter.clone();
            let event_bus = event_bus.clone();
            let circuit_breaker = circuit_breaker.clone();
            // Note: retry_queue is NOT passed to spawned task - it's only used
            // by orchestrator to handle PageResult::NeedsRetry
            let total_pages = Arc::clone(&total_pages);
            let queue = Arc::clone(&queue);
            let indexing_sender = indexing_sender.clone();
            let visited = Arc::clone(&visited);
            let user_agent = user_agent.clone();
            let http_error_cache = Arc::clone(&http_error_cache);
            let domain_queues = Arc::clone(&domain_queues);

            // Spawn concurrent task
            let task = tokio::spawn(async move {
                let _permit = permit; // Hold until task completes
                let _domain_permit = domain_permit; // Hold until task completes

                // Process single page with full error handling
                // Note: visited deduplication happens at orchestrator level (line 153),
                // not in page_processor - prevents double-insert bug
                let ctx = PageProcessorContext {
                    config,
                    link_rewriter,
                    event_bus,
                    circuit_breaker,
                    total_pages,
                    queue,
                    indexing_sender,
                    visited,
                    user_agent,
                    http_error_cache,
                    domain_queues,
                };

                process_single_page(browser, item, ctx).await
            });

            active_tasks.push(task);
        }

        // Wait for at least one task to complete
        match active_tasks.next().await {
            Some(Ok(result)) => match result {
                PageResult::Success(url) => {
                    debug!("Completed crawling: {url}");
                }
                
                PageResult::NeedsRetry(item) => {
                    // Circuit breaker rejected - queue for later retry
                    if let Some(ref rq) = retry_queue {
                        debug!("Circuit breaker: queueing for retry: {}", item.url);
                        rq.add(item);
                    } else {
                        warn!("No retry queue available, discarding: {}", item.url);
                    }
                }
                
                PageResult::FailedRetryable { mut item, error, failure_kind } => {
                    let max_retries = config.max_page_retries();
                    
                    if failure_kind.is_retryable() && item.retry_count < max_retries {
                        item.retry_count += 1;
                        
                        // Calculate backoff delay
                        let delay = calculate_retry_backoff(item.retry_count, failure_kind);
                        
                        warn!(
                            "Page failed (attempt {}/{}): {} [{:?}] - retrying in {:?}",
                            item.retry_count, max_retries, item.url, failure_kind, delay
                        );
                        
                        // Remove from visited to allow re-processing
                        visited.remove(&item.url);
                        
                        // Apply backoff delay before requeueing
                        tokio::time::sleep(delay).await;
                        
                        // Re-add to main queue
                        queue.lock().await.push_back(item);
                    } else {
                        warn!(
                            "Page failed after {} attempts: {} [{:?}] - giving up: {}",
                            item.retry_count, item.url, failure_kind, error
                        );
                        
                        // Record failure in circuit breaker
                        if let Some(ref cb) = circuit_breaker
                            && let Ok(domain) = extract_domain(&item.url)
                        {
                            cb.record_failure(&domain, &error.to_string());
                        }
                        
                        // Publish RetryExhausted event
                        if let Some(bus) = &event_bus {
                            let event = CrawlEvent::retry_exhausted(
                                item.url,
                                item.retry_count,
                                error.to_string(),
                            );
                            if let Err(e) = bus.publish(event).await {
                                warn!("Failed to publish RetryExhausted event: {e}");
                            }
                        }
                    }
                }
                
                PageResult::FailedPermanent { url, error } => {
                    warn!("Permanent failure for {}: {}", url, error);
                    // No retry, record failure in circuit breaker
                    if let Some(ref cb) = circuit_breaker
                        && let Ok(domain) = extract_domain(&url)
                    {
                        cb.record_failure(&domain, &error.to_string());
                    }
                }
            },
            Some(Err(e)) => {
                error!("Task panicked: {e}");
            }
            None => break, // All tasks completed
        }

        // Check if done
        let remaining = queue.lock().await.len();
        let retry_remaining = retry_queue.as_ref().map_or(0, |rq| rq.len());
        if remaining == 0 && retry_remaining == 0 && active_tasks.is_empty() {
            break;
        }

        // If only retry queue has items, add small sleep to avoid busy-spinning
        if remaining == 0 && retry_remaining > 0 && active_tasks.is_empty() {
            debug!(
                "Main queue empty, {} items in retry queue waiting for circuit recovery",
                retry_remaining
            );
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }

    // Publish LinkRewriteCompleted if rewriting happened
    let urls_registered = link_rewriter.get_registration_count().await;
    let total_pages_final = total_pages.load(Ordering::Relaxed);
    if urls_registered > 0 {
        if let Some(bus) = &event_bus {
            let event = CrawlEvent::link_rewrite_completed(
                config.start_url.clone(),
                total_pages_final,
                urls_registered,
            );
            if let Err(e) = bus.publish(event).await {
                warn!("Failed to publish LinkRewriteCompleted event: {e}");
            }
        }
        info!("Link rewriting enabled: {urls_registered} URLs registered for local navigation");
    }

    // Publish CrawlCompleted event (CRITICAL - this is the final summary)
    if let Some(bus) = &event_bus {
        let event =
            CrawlEvent::crawl_completed(total_pages_final, urls_registered, start_time.elapsed());
        bus.publish(event).await.context(
            "Failed to publish CrawlCompleted event - event bus may be shutdown or full",
        )?;

        // Report final metrics before shutdown
        let metrics = bus.metrics().snapshot();
        if metrics.events_failed > 0 || metrics.events_dropped > 0 {
            warn!(
                "Event bus metrics - Published: {}, Dropped: {}, Failed: {}, Success rate: {:.1}%",
                metrics.events_published,
                metrics.events_dropped,
                metrics.events_failed,
                metrics.success_rate() * 100.0
            );
        } else {
            debug!(
                "Event bus metrics - Published: {} events to {} subscribers",
                metrics.events_published, metrics.active_subscribers
            );
        }

        // Graceful shutdown
        bus.shutdown_gracefully(crate::crawl_events::types::ShutdownReason::CrawlCompleted)
            .await;
    }

    progress.report_cleanup_started();

    // Cleanup differs based on whether we used a pooled or fresh browser
    if pool_guard.is_some() {
        // POOLED BROWSER: Just drop the guard to return browser to pool
        // The pool manages browser lifecycle and cleanup
        info!("Returning browser to pool");
        drop(pool_guard);
        drop(browser); // Drop our Arc reference
        debug!("Browser returned to pool successfully");
    } else {
        // FRESH BROWSER: Full cleanup required
        // CRITICAL: Cleanup order matters!
        // 1. Unwrap browser from Arc (must happen before close)
        // 2. Close browser gracefully (browser.close() + browser.wait())
        // 3. Clean up temp directory
        // 4. THEN abort handler task
        //
        // This ensures the browser is properly closed before the handler loses its CDP connection.

        // Try to unwrap browser from Arc - if there are still references, skip cleanup
        let browser_for_cleanup = match Arc::try_unwrap(browser) {
            Ok(b) => Some(b),
            Err(arc) => {
                warn!(
                    "Browser still has {} strong references, cleanup will happen on drop",
                    Arc::strong_count(&arc)
                );
                None
            }
        };

        if let Some(browser_owned) = browser_for_cleanup {
            // Close browser gracefully and clean up temp directory
            match super::cleanup::cleanup_browser_and_data(browser_owned, chrome_data_dir_path.clone()).await {
                Ok(super::cleanup::CleanupResult::Success) => {
                    debug!("Browser and data cleanup completed successfully");
                }
                Ok(super::cleanup::CleanupResult::PartialFailure(errors)) => {
                    warn!("Cleanup completed with failures: {errors:?}");
                    for error in &errors {
                        progress.report_error(&format!("Cleanup error: {error}"));
                    }
                }
                Err(e) => {
                    warn!("Cleanup failed: {e}");
                    progress.report_error(&format!("Cleanup failed: {e}"));
                }
            }
        }

        // NOW abort the handler task (after browser is closed) - only for fresh browsers
        if let Some(task) = handler_task {
            info!("Aborting browser handler task");
            task.abort();
            if let Err(e) = task.await
                && !e.is_cancelled()
            {
                warn!("Handler task failed during abort: {e}");
            }
        }
    }

    progress.report_completed();

    Ok(Some(chrome_data_dir_path))
}
