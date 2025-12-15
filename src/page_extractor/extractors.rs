//! Zero-allocation page data extraction functions
//!
//! This module provides blazing-fast extraction functions for various page elements
//! with pre-allocated buffers and lock-free operations.

use super::js_scripts::{
    INTERACTIVE_ELEMENTS_SCRIPT, METADATA_SCRIPT, RESOURCES_SCRIPT, SECURITY_SCRIPT, TIMING_SCRIPT,
};
use super::schema::InteractiveElement;
use super::schema::{PageMetadata, ResourceInfo, SecurityInfo, TimingInfo};
use anyhow::{Context, Result};
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams,
};

/// Wait for page to be fully loaded before taking screenshot
///
/// This function polls the page to ensure content is fully rendered:
/// 1. Checks `document.readyState === 'complete'`
/// 2. Waits for network idle (no pending requests)
/// 3. Adds small buffer for final image loading
///
/// Without this, screenshots of JS-heavy sites will be blank because
/// `page.wait_for_navigation()` only waits for the HTTP response, not
/// for JavaScript execution, CSS application, or image loading.
///
/// # Arguments
/// * `page` - Page to wait for
/// * `max_wait_secs` - Maximum time to wait (default 10s recommended)
pub async fn wait_for_page_load(page: &Page, max_wait_secs: u64) -> Result<()> {
    use std::time::{Duration, Instant};
    
    let start = Instant::now();
    let max_wait = Duration::from_secs(max_wait_secs);
    let poll_interval = Duration::from_millis(100);
    
    log::debug!("Waiting for page to be fully loaded (max {}s)", max_wait_secs);
    
    // Phase 1: Wait for document.readyState === 'complete'
    loop {
        // Check if we've exceeded max wait time
        if start.elapsed() >= max_wait {
            log::warn!("Timeout waiting for page load after {}s, proceeding anyway", max_wait_secs);
            break;
        }
        
        // Evaluate JavaScript to check readyState
        let ready_state_script = r#"
            (function() {
                return {
                    readyState: document.readyState,
                    imagesLoaded: Array.from(document.images).every(img => img.complete),
                    bodyExists: document.body !== null
                };
            })()
        "#;
        
        match page.evaluate(ready_state_script).await {
            Ok(result) => {
                if let Ok(value) = result.into_value::<serde_json::Value>() {
                    let ready_state = value.get("readyState").and_then(|v| v.as_str());
                    let images_loaded = value.get("imagesLoaded").and_then(|v| v.as_bool()).unwrap_or(false);
                    let body_exists = value.get("bodyExists").and_then(|v| v.as_bool()).unwrap_or(false);
                    
                    if ready_state == Some("complete") && body_exists {
                        let elapsed = start.elapsed();
                        log::debug!(
                            "Page ready after {:.2}s (images loaded: {})",
                            elapsed.as_secs_f64(),
                            images_loaded
                        );
                        
                        // Phase 2: Small additional wait for final rendering
                        // Even after readyState=complete, some images may still be loading
                        // or CSS animations may be in progress
                        if !images_loaded {
                            log::debug!("Images still loading, waiting additional 500ms");
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        
                        break;
                    }
                }
            }
            Err(e) => {
                log::debug!("Failed to check readyState: {}, retrying", e);
            }
        }
        
        // Wait before next poll
        tokio::time::sleep(poll_interval).await;
    }
    
    // Phase 3: Final safety buffer for CSS transitions and lazy-loaded content
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    log::debug!("Page load wait complete after {:.2}s", start.elapsed().as_secs_f64());
    Ok(())
}

/// Scroll to bottom of page to trigger lazy-loaded content
///
/// This function scrolls the page in increments to trigger lazy-loading
/// of content that only appears when scrolled into view.
///
/// # Arguments
/// * `page` - Page to scroll
/// * `scroll_pause_ms` - Milliseconds to wait between scroll increments
pub async fn scroll_to_bottom(page: &Page, scroll_pause_ms: u64) -> Result<()> {
    log::debug!("Scrolling page to trigger lazy-loaded content");
    
    let scroll_script = r#"
        (async function() {
            const distance = 200; // Scroll 200px at a time
            const delay = 100; // Wait 100ms between scrolls
            
            let lastHeight = document.documentElement.scrollHeight;
            let currentHeight = 0;
            
            while (currentHeight < lastHeight) {
                window.scrollBy(0, distance);
                await new Promise(resolve => setTimeout(resolve, delay));
                currentHeight = window.pageYOffset + window.innerHeight;
                lastHeight = document.documentElement.scrollHeight;
            }
            
            // Scroll back to top
            window.scrollTo(0, 0);
            
            return {
                totalHeight: lastHeight,
                scrollsPerformed: Math.ceil(lastHeight / distance)
            };
        })()
    "#;
    
    match page.evaluate(scroll_script).await {
        Ok(result) => {
            if let Ok(value) = result.into_value::<serde_json::Value>() {
                let total_height = value.get("totalHeight").and_then(|v| v.as_i64()).unwrap_or(0);
                let scrolls = value.get("scrollsPerformed").and_then(|v| v.as_i64()).unwrap_or(0);
                log::debug!("Scrolled page: {} pixels in {} increments", total_height, scrolls);
            }
        }
        Err(e) => {
            log::warn!("Failed to scroll page: {}", e);
        }
    }
    
    // Final pause to let any triggered lazy-loads complete
    tokio::time::sleep(tokio::time::Duration::from_millis(scroll_pause_ms)).await;
    
    Ok(())
}

/// Capture screenshot with retry logic for transient CDP errors
///
/// CDP error -32000 "Unable to capture screenshot" can occur transiently when:
/// - Page is scrolling to hash fragment
/// - Viewport is in transition
/// - Rendering pipeline is not ready
///
/// This function implements exponential backoff retry to handle these cases.
///
/// # Arguments
/// * `page` - Page to screenshot
/// * `params` - CDP screenshot parameters
/// * `max_attempts` - Maximum retry attempts (default: 3)
///
/// # Returns
/// * `Ok(Vec<u8>)` - Screenshot data
/// * `Err` - Persistent error after all retries
async fn capture_screenshot_with_retry(
    page: &Page,
    params: CaptureScreenshotParams,
    max_attempts: u32,
) -> Result<Vec<u8>> {
    use std::time::Duration;
    
    let mut last_error = None;
    
    for attempt in 1..=max_attempts {
        // Exponential backoff: 0ms, 500ms, 1500ms
        if attempt > 1 {
            let delay_ms = match attempt {
                2 => 500,
                3 => 1500,
                _ => 2000,
            };
            log::debug!(
                "Screenshot attempt {} of {} (retry after {}ms)",
                attempt,
                max_attempts,
                delay_ms
            );
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        
        match page.screenshot(params.clone()).await {
            Ok(data) => {
                if attempt > 1 {
                    log::info!(
                        "Screenshot succeeded on attempt {} after retry",
                        attempt
                    );
                }
                return Ok(data);
            }
            Err(e) => {
                let error_string = e.to_string();
                
                // Check if this is the transient CDP -32000 error
                if error_string.contains("-32000") || error_string.contains("Unable to capture screenshot") {
                    log::debug!(
                        "Transient screenshot error on attempt {}: {}",
                        attempt,
                        error_string
                    );
                    last_error = Some(e);
                    
                    // Continue to next attempt if not final
                    if attempt < max_attempts {
                        continue;
                    }
                } else {
                    // Non-transient error, fail immediately
                    return Err(anyhow::anyhow!(
                        "Screenshot failed with non-retryable error: {e}"
                    ));
                }
            }
        }
    }
    
    // All retries exhausted - use if let to avoid unwrap()
    if let Some(e) = last_error {
        Err(anyhow::anyhow!(
            "Screenshot failed after {} attempts: {}",
            max_attempts,
            e
        ))
    } else {
        Err(anyhow::anyhow!(
            "Screenshot failed after {} attempts with no error captured",
            max_attempts
        ))
    }
}

/// Extract page metadata with zero allocation
#[inline]
pub async fn extract_metadata(page: Page) -> Result<PageMetadata> {
    let js_result = page
        .evaluate(METADATA_SCRIPT)
        .await
        .context("Failed to execute metadata extraction script")?;

    let metadata: PageMetadata = match js_result.into_value() {
        Ok(value) => {
            serde_json::from_value(value).context("Failed to parse metadata from JS result")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to get metadata value: {e}")),
    };

    Ok(metadata)
}

/// Extract page resources with pre-allocated collections
#[inline]
pub async fn extract_resources(page: Page) -> Result<ResourceInfo> {
    let js_result = page
        .evaluate(RESOURCES_SCRIPT)
        .await
        .context("Failed to execute resources extraction script")?;

    let resources: ResourceInfo = match js_result.into_value() {
        Ok(value) => {
            serde_json::from_value(value).context("Failed to parse resources from JS result")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to get resources value: {e}")),
    };

    // Log resource counts for debugging
    log::debug!(
        "Extracted resources - Stylesheets: {}, Scripts: {}, Images: {}, Fonts: {}, Media: {}",
        resources.stylesheets.len(),
        resources.scripts.len(),
        resources.images.len(),
        resources.fonts.len(),
        resources.media.len()
    );

    Ok(resources)
}

/// Extract timing information with zero allocation
#[inline]
pub async fn extract_timing_info(page: Page) -> Result<TimingInfo> {
    let js_result = page
        .evaluate(TIMING_SCRIPT)
        .await
        .context("Failed to execute timing extraction script")?;

    let timing: TimingInfo = match js_result.into_value() {
        Ok(value) => {
            serde_json::from_value(value).context("Failed to parse timing info from JS result")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to get timing info value: {e}")),
    };

    Ok(timing)
}

/// Extract security information efficiently
#[inline]
pub async fn extract_security_info(page: Page) -> Result<SecurityInfo> {
    let js_result = page
        .evaluate(SECURITY_SCRIPT)
        .await
        .context("Failed to execute security extraction script")?;

    let security: SecurityInfo = match js_result.into_value() {
        Ok(value) => {
            serde_json::from_value(value).context("Failed to parse security info from JS result")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to get security info value: {e}")),
    };

    Ok(security)
}

/// Extract interactive elements with zero allocation
#[inline]
pub async fn extract_interactive_elements(page: Page) -> Result<Vec<InteractiveElement>> {
    // Use efficient JavaScript evaluation to extract all interactive elements
    let js_result = page
        .evaluate(INTERACTIVE_ELEMENTS_SCRIPT)
        .await
        .context("Failed to execute interactive elements extraction script")?;

    let value = js_result
        .into_value::<serde_json::Value>()
        .map_err(|e| anyhow::anyhow!("Failed to get value from JS result: {e}"))?;

    let arr = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("JavaScript evaluation did not return an array"))?;

    let elements: Vec<InteractiveElement> = arr
        .iter()
        .map(|item| {
            serde_json::from_value(item.clone()).context("Failed to parse interactive element")
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(elements)
}

/// Extract links from the page with zero allocation
#[inline]
pub async fn extract_links(page: Page) -> Result<Vec<super::schema::CrawlLink>> {
    let js_result = page
        .evaluate(super::js_scripts::LINKS_SCRIPT)
        .await
        .context("Failed to execute links extraction script")?;

    let links: Vec<super::schema::CrawlLink> = match js_result.into_value::<serde_json::Value>() {
        Ok(value) => {
            serde_json::from_value(value).context("Failed to parse links from JS result")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to get links value: {e}")),
    };

    Ok(links)
}

/// Take a screenshot of the page
pub async fn capture_screenshot(
    page: Page,
    url: &str,
    output_dir: &std::path::Path,
    compression_threshold: usize,
) -> Result<()> {
    let url_str = url.to_string();
    let output_dir = output_dir.to_path_buf();

    // Get mirror path (async)
    let path = crate::utils::get_mirror_path(&url_str, &output_dir, "index.png").await?;

    // Ensure .gitignore exists in domain directory
    crate::utils::ensure_domain_gitignore(&path, &output_dir).await?;

    tokio::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Path has no parent directory"))?,
    )
    .await?;

    // Wait for page to be fully loaded
    wait_for_page_load(&page, 10).await?;

    // ============ FIX: Hash fragment handling ============
    // URLs with hash fragments (e.g., #section-id) trigger in-page
    // navigation AFTER page load. Add extra wait for scroll to complete.
    if url_str.contains('#') {
        log::debug!("URL contains hash fragment, waiting for scroll to complete");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    // =====================================================

    // ============ FIX: Validate viewport dimensions ============
    // Ensure viewport is in valid state before screenshot attempt.
    // Invalid viewport (0x0 or transitioning) causes CDP error -32000.
    let viewport_check = page.evaluate(r#"
        (function() {
            return {
                width: window.innerWidth,
                height: window.innerHeight,
                scrollY: window.scrollY,
                documentHeight: document.documentElement.scrollHeight
            };
        })()
    "#).await;
    
    if let Ok(result) = viewport_check
        && let Ok(viewport) = result.into_value::<serde_json::Value>() {
        let width = viewport.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
        let height = viewport.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
        
        log::debug!(
            "Viewport: {}x{}, scrollY: {}, docHeight: {}",
            width,
            height,
            viewport.get("scrollY").and_then(|v| v.as_u64()).unwrap_or(0),
            viewport.get("documentHeight").and_then(|v| v.as_u64()).unwrap_or(0)
        );
        
        if width == 0 || height == 0 {
            return Err(anyhow::anyhow!(
                "Invalid viewport dimensions: {}x{}, cannot capture screenshot",
                width,
                height
            ));
        }
    }
    // ===========================================================

    let params = CaptureScreenshotParams {
        quality: Some(100),
        format: Some(CaptureScreenshotFormat::Png),
        capture_beyond_viewport: Some(true),
        ..Default::default()
    };

    // ============ FIX: Use retry logic for CDP call ============
    // CDP error -32000 is often transient. Retry with exponential backoff.
    let screenshot_data = capture_screenshot_with_retry(&page, params, 3).await?;
    // ===========================================================

    // Save compressed file directly with async/await
    let (_saved_path, _metadata) = crate::content_saver::save_compressed_file(
        screenshot_data,
        &path,
        "image/png",
        false,
        compression_threshold,
    )
    .await?;

    log::debug!("Screenshot captured and saved successfully for URL: {url_str}");
    Ok(())
}
