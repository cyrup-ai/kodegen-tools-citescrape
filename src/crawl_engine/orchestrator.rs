//! Main crawl orchestration logic
//!
//! Coordinates multi-page crawling with:
//! - Browser lifecycle management
//! - Concurrent task execution
//! - Queue and concurrency control
//! - Event publishing and monitoring
//! - Graceful shutdown and cleanup

use anyhow::{Context, Result};
use dashmap::DashSet;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use log::{debug, error, info, warn};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use super::crawl_types::CrawlQueue;
use super::{CircuitBreaker, DomainLimiter, extract_domain};
use super::page_processor::{PageProcessorContext, process_single_page};
use super::progress::ProgressReporter;
use crate::browser_setup::launch_browser;
use crate::config::CrawlConfig;
use crate::crawl_events::{
    CrawlEventBus,
    types::CrawlEvent,
};
use crate::link_rewriter::LinkRewriter;

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
        });
        q
    }));

    // Lock-free visited set (replaces Bloom filter for thread-safety)
    // DashSet provides concurrent access without locks, ideal for multi-task crawling
    let visited: Arc<DashSet<String>> = Arc::new(DashSet::new());

    // Thread-safe circuit breaker for domain-level failure detection
    let circuit_breaker = if config.circuit_breaker_enabled() {
        Some(Arc::new(tokio::sync::Mutex::new(CircuitBreaker::new(
            config.circuit_breaker_failure_threshold(),
            2, // success_threshold: close circuit after 2 consecutive successes
            Duration::from_secs(config.circuit_breaker_retry_delay_secs()),
        ))))
    } else {
        None
    };

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

    // Setup browser - find system Chrome or download managed Chromium
    // launch_browser handles:
    // - Finding Chrome/Chromium executable (system paths + 'which' command)
    // - Downloading managed Chromium if not found (with proper dir creation)
    // - Configuring browser with stealth mode arguments
    // - Spawning handler task to drive CDP connection
    // - Using unique chrome_data_dir per session (if configured) to prevent profile lock contention

    let chrome_dir_param = config.chrome_data_dir().cloned();

    let (browser, handler_task, chrome_data_dir_path) = launch_browser(config.headless(), chrome_dir_param)
        .await
        .context("Failed to launch browser")?;

    progress.report_browser_launched();

    // Wrap browser in Arc for sharing across concurrent tasks
    let browser = Arc::new(browser);

    // Concurrency control
    let concurrency = config.max_concurrent_pages();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let domain_limiter = Arc::new(DomainLimiter::new(config.max_concurrent_per_domain()));

    // Main concurrent crawl loop
    let mut active_tasks = FuturesUnordered::new();

    loop {
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
            let total_pages = Arc::clone(&total_pages);
            let queue = Arc::clone(&queue);
            let indexing_sender = indexing_sender.clone();

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
                };

                match process_single_page(browser, item, ctx).await {
                    Ok(url) => {
                        debug!("Successfully crawled: {url}");
                        Ok(url)
                    }
                    Err(e) => {
                        debug!("Failed to crawl page: {e}");
                        Err(e)
                    }
                }
            });

            active_tasks.push(task);
        }

        // Wait for at least one task to complete
        match active_tasks.next().await {
            Some(Ok(result)) => match result {
                Ok(url) => {
                    debug!("Completed crawling: {url}");
                }
                Err(e) => {
                    warn!("Crawl task failed: {e}");
                }
            },
            Some(Err(e)) => {
                error!("Task panicked: {e}");
            }
            None => break, // All tasks completed
        }

        // Check if done
        let remaining = queue.lock().await.len();
        if remaining == 0 && active_tasks.is_empty() {
            break;
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

    // NOW abort the handler task (after browser is closed)
    info!("Aborting browser handler task");
    handler_task.abort();
    if let Err(e) = handler_task.await
        && !e.is_cancelled()
    {
        warn!("Handler task failed during abort: {e}");
    }

    progress.report_completed();

    Ok(Some(chrome_data_dir_path))
}
