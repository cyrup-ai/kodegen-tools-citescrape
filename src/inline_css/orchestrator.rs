//! Resource inlining orchestration
//!
//! This module provides orchestration functions that coordinate concurrent
//! downloading and inlining of CSS, image, and SVG resources.

use super::css_downloader::download_all_css;
use super::domain_queue::{DomainDownloadQueue, DomainQueueManager};
use super::downloaders::InlineConfig;
use super::image_downloader::download_all_images;
use super::svg_downloader::download_all_svgs;
use super::types::{InliningError, InliningResult, ResourceFuture, ResourceType};
use crate::page_extractor::schema::ResourceInfo;
use anyhow::Result;
use dashmap::DashMap;
use futures::future::join_all;
use futures::join;
use reqwest::Client;
use std::sync::Arc;

/// Inline all external resources with a single HTML parse
///
/// This function parses the HTML document once and extracts all resource information
/// synchronously, then processes the resources asynchronously. This eliminates the
/// performance cost of parsing the same HTML document three separate times.
///
/// Returns `InliningResult` containing the processed HTML along with success/failure metrics.
#[inline]
pub async fn inline_all_resources(
    html_content: String,
    base_url: String,
    config: &InlineConfig,
    max_inline_image_size_bytes: Option<usize>,
    rate_rps: Option<f64>,
) -> Result<InliningResult> {
    let config = config.clone();

    // Parse HTML once and extract all resource information synchronously
    // This eliminates redundant parsing and prepares for concurrent downloads
    let (css_links, images, svgs, extraction_failures) = {
        let document = scraper::Html::parse_document(&html_content);
        let (css, css_failures) = super::processors::extract_css_links(&document, &base_url)?;
        let (imgs, img_failures) = super::processors::extract_images(&document, &base_url)?;
        let (svg_list, svg_failures) = super::processors::extract_svgs(&document, &base_url)?;

        // Collect all extraction failures
        let mut all_extraction_failures = Vec::new();
        all_extraction_failures.extend(css_failures);
        all_extraction_failures.extend(img_failures);
        all_extraction_failures.extend(svg_failures);

        (css, imgs, svg_list, all_extraction_failures)
        // document is dropped here, safe to proceed with async operations
    };

    // Client already uses Arc internally, so just clone it
    let client = Client::new();

    // Process all resource types concurrently using futures::join!
    // Each download task runs in parallel, eliminating sequential bottleneck
    let (css_downloads, img_downloads, svg_downloads) = join!(
        download_all_css(css_links, client.clone(), &config, rate_rps),
        download_all_images(
            images,
            client.clone(),
            &config,
            max_inline_image_size_bytes,
            rate_rps
        ),
        download_all_svgs(svgs, client, &config, rate_rps)
    );

    // Destructure results to get successes and failures
    let (css_replacements, css_failures) = css_downloads?;
    let (image_replacements, image_failures) = img_downloads?;
    let (svg_replacements, svg_failures) = svg_downloads?;

    // Collect all failures (extraction + download)
    let mut all_failures = Vec::new();
    all_failures.extend(extraction_failures);
    all_failures.extend(css_failures);
    all_failures.extend(image_failures);
    all_failures.extend(svg_failures);

    // Count successes
    let successes = css_replacements.len() + image_replacements.len() + svg_replacements.len();

    // Apply all replacements in a single DOM parse/serialize cycle
    // This eliminates the performance bottleneck of parsing/serializing 3 times
    let html = super::utils::apply_all_replacements(
        html_content,
        css_replacements,
        image_replacements,
        svg_replacements,
    )?;

    Ok(InliningResult {
        html,
        successes,
        failures: all_failures,
    })
}

/// Download and inline external resources using extracted resource information
/// with concurrent downloads for maximum performance
///
/// # Arguments
/// * `html_content` - HTML content to process
/// * `base_url` - Base URL for resolving relative URLs
/// * `config` - Inline configuration with user agent
/// * `resources` - Extracted resource information (stylesheets, images, etc.)
/// * `max_inline_image_size_bytes` - Maximum image size to inline
/// * `rate_rps` - Optional rate limit in requests per second
/// * `http_error_cache` - Shared cache for HTTP error responses (enables cross-page caching)
/// * `domain_queues` - Shared domain download queues (enables cross-page worker sharing)
///
/// Returns `InliningResult` containing the processed HTML along with success/failure metrics.
#[inline]
pub async fn inline_resources_from_info(
    html_content: String,
    base_url: String,
    config: &InlineConfig,
    resources: ResourceInfo,
    max_inline_image_size_bytes: Option<usize>,
    rate_rps: Option<f64>,
    http_error_cache: Arc<DashMap<String, u16>>,
    domain_queues: Arc<DashMap<String, Arc<DomainDownloadQueue>>>,
) -> Result<InliningResult> {
    let _config = config.clone();
    let client = Client::new();
    
    // Create domain queue manager for coordinated downloads
    // Uses shared http_error_cache and domain_queues for cross-page caching and worker sharing
    let queue_manager = DomainQueueManager::new(client.clone(), rate_rps, config.user_agent.clone(), http_error_cache, domain_queues);
    
    let html = html_content;

    log::debug!("Starting to inline resources for base_url: {base_url}");
    log::debug!(
        "Found {} stylesheets to process",
        resources.stylesheets.len()
    );
    log::debug!("Found {} images to process", resources.images.len());

    // Collect all futures (don't await yet!)
    // Each future returns either Ok((url, content)) or the InliningError on failure
    let mut futures: Vec<ResourceFuture> = Vec::new();

    // Collect stylesheet futures
    for stylesheet in &resources.stylesheets {
        if !stylesheet.inline
            && let Some(ref href) = stylesheet.url
        {
            let base = base_url.clone();
            let href_clone = href.clone();
            let queue_manager_clone = queue_manager.clone();

            let future = Box::pin(async move {
                let css_url = match super::utils::resolve_url(&base, &href_clone) {
                    Ok(url) => url,
                    Err(e) => {
                        return Err(InliningError {
                            url: href_clone,
                            resource_type: ResourceType::Css,
                            error: e.to_string(),
                        });
                    }
                };

                log::debug!("Processing CSS: {href_clone} -> {css_url}");
                
                // Submit to queue (handles rate limiting internally)
                let bytes = match queue_manager_clone.submit_download(css_url.clone()).await {
                    Ok(b) => b,
                    Err(e) => {
                        return Err(InliningError {
                            url: css_url,
                            resource_type: ResourceType::Css,
                            error: e.to_string(),
                        });
                    }
                };
                
                // Convert bytes to string
                match String::from_utf8(bytes.to_vec()) {
                    Ok(content) => {
                        log::debug!("Downloaded CSS content length: {} chars", content.len());
                        log::debug!("Successfully downloaded CSS from: {css_url}");
                        Ok((href_clone, content, ResourceType::Css))
                    }
                    Err(e) => Err(InliningError {
                        url: css_url,
                        resource_type: ResourceType::Css,
                        error: format!("CSS content is not valid UTF-8: {e}"),
                    }),
                }
            });

            futures.push(future);
        }
    }

    // Collect image and SVG futures
    for image in &resources.images {
        let base = base_url.clone();
        let src = image.url.clone();

        // Check if this is an SVG based on the URL
        let is_svg = src.to_lowercase().contains(".svg");

        if is_svg {
            // Process as SVG
            let queue_manager_clone = queue_manager.clone();
            let future = Box::pin(async move {
                let svg_url = match super::utils::resolve_url(&base, &src) {
                    Ok(url) => url,
                    Err(e) => {
                        return Err(InliningError {
                            url: src,
                            resource_type: ResourceType::Svg,
                            error: e.to_string(),
                        });
                    }
                };

                log::debug!("Processing SVG: {src} -> {svg_url}");
                
                // Submit to queue (handles rate limiting internally)
                let bytes = match queue_manager_clone.submit_download(svg_url.clone()).await {
                    Ok(b) => b,
                    Err(e) => {
                        return Err(InliningError {
                            url: svg_url,
                            resource_type: ResourceType::Svg,
                            error: e.to_string(),
                        });
                    }
                };
                
                // Convert bytes to string and clean up
                match String::from_utf8(bytes.to_vec()) {
                    Ok(mut svg_content) => {
                        // Clean up SVG for inline usage (remove XML declaration, comment DOCTYPE)
                        svg_content = svg_content.replace("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", "");
                        
                        if let Some(doctype_start) = svg_content.find("<!DOCTYPE svg")
                            && let Some(doctype_end_offset) = svg_content[doctype_start..].find('>')
                        {
                            let doctype_end = doctype_start + doctype_end_offset + 1;
                            let doctype = &svg_content[doctype_start..doctype_end];
                            let commented = format!("<!--{doctype}-->");
                            svg_content.replace_range(doctype_start..doctype_end, &commented);
                        }
                        
                        log::debug!("Successfully downloaded SVG: {svg_url}");
                        Ok((src, svg_content, ResourceType::Svg))
                    }
                    Err(e) => Err(InliningError {
                        url: svg_url,
                        resource_type: ResourceType::Svg,
                        error: format!("SVG content is not valid UTF-8: {e}"),
                    }),
                }
            });

            futures.push(future);
        } else {
            // Process as regular image
            let queue_manager_clone = queue_manager.clone();
            let future = Box::pin(async move {
                let image_url = match super::utils::resolve_url(&base, &src) {
                    Ok(url) => url,
                    Err(e) => {
                        return Err(InliningError {
                            url: src,
                            resource_type: ResourceType::Image,
                            error: e.to_string(),
                        });
                    }
                };

                log::debug!("Processing image: {src} -> {image_url}");
                
                // Submit to queue (handles rate limiting internally)
                let bytes = match queue_manager_clone.submit_download(image_url.clone()).await {
                    Ok(b) => b,
                    Err(e) => {
                        return Err(InliningError {
                            url: image_url,
                            resource_type: ResourceType::Image,
                            error: e.to_string(),
                        });
                    }
                };
                
                // Check size limit and encode to base64 if appropriate
                if let Some(max_size) = max_inline_image_size_bytes
                    && bytes.len() > max_size
                {
                    log::debug!(
                        "Image size ({} bytes) exceeds max_inline_size_bytes ({} bytes), keeping as external URL: {image_url}",
                        bytes.len(),
                        max_size
                    );
                    return Ok((src, image_url, ResourceType::Image));
                }
                
                // Encode to base64 data URL
                use base64::Engine;
                let content_type = "image/jpeg"; // Default, ideally would detect from response headers
                let encoded_capacity = base64::encoded_len(bytes.len(), false).unwrap_or(0);
                let mut data_url = String::with_capacity(encoded_capacity + 30 + content_type.len());
                
                data_url.push_str("data:");
                data_url.push_str(content_type);
                data_url.push_str(";base64,");
                base64::engine::general_purpose::STANDARD.encode_string(&bytes, &mut data_url);
                
                log::debug!("Successfully downloaded and encoded image: {image_url}");
                Ok((src, data_url, ResourceType::Image))
            });

            futures.push(future);
        }
    }

    // Execute ALL downloads concurrently
    let results = join_all(futures).await;

    // Separate CSS, image, and SVG replacements and collect failures
    let mut css_replacements = Vec::new();
    let mut image_replacements = Vec::new();
    let mut svg_replacements = Vec::new();
    let mut failures = Vec::new();

    // Process results and apply replacements
    for result in results {
        match result {
            Ok((original_url, content, resource_type)) => match resource_type {
                ResourceType::Css => {
                    css_replacements.push((original_url, content));
                }
                ResourceType::Image => {
                    image_replacements.push((original_url, content));
                }
                ResourceType::Svg => {
                    svg_replacements.push((original_url, content));
                }
            },
            Err(inlining_error) => {
                log::warn!(
                    "Failed to download {} from {}: {}",
                    inlining_error.resource_type,
                    inlining_error.url,
                    inlining_error.error
                );
                failures.push(inlining_error);
            }
        }
    }

    // Count successes
    let successes = css_replacements.len() + image_replacements.len() + svg_replacements.len();

    // Apply all replacements in a single DOM parse/serialize cycle
    // This eliminates the performance bottleneck of parsing/serializing 3 times
    let html = super::utils::apply_all_replacements(
        html,
        css_replacements,
        image_replacements,
        svg_replacements,
    )?;

    Ok(InliningResult {
        html,
        successes,
        failures,
    })
}
