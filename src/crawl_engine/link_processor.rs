//! Link processing and queue management
//!
//! This module handles extracting links from pages and managing the crawl queue.

use anyhow::Result;
use chromiumoxide::Page;
use log::{debug, warn};
use std::collections::VecDeque;

use super::crawl_types::CrawlQueue;
use crate::page_extractor::extractors::extract_links;

/// Normalize a URL string by stripping fragment anchors.
///
/// Fragment identifiers (#foo) are client-side navigation markers that don't
/// represent different HTTP resources. This function removes them to enable
/// proper URL deduplication during crawling.
///
/// # Arguments
/// * `url` - URL string to normalize
///
/// # Returns
/// * `Ok(String)` - Normalized URL without fragment
/// * `Err` - If URL parsing fails
///
/// # Examples
///
/// ```
/// let normalized = normalize_url("https://example.com/page#section")?;
/// assert_eq!(normalized, "https://example.com/page");
/// ```
fn normalize_url(url: &str) -> Result<String> {
    let mut parsed = url::Url::parse(url)
        .map_err(|e| anyhow::anyhow!("Failed to parse URL for normalization: {}", e))?;
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

/// State for managing the crawl queue
/// 
/// Note: URL deduplication is handled at two levels:
/// 1. Orchestrator level: DashSet prevents re-crawling visited URLs
/// 2. Queue level: Manual deduplication in page_processor prevents duplicate queue entries
pub struct CrawlState {
    pub queue: VecDeque<CrawlQueue>,
    pub max_depth: u8,
}

/// Process links from the current page and add them to the crawl queue
pub async fn process_page_links(
    page: Page,
    current_item: CrawlQueue,
    crawl_state: CrawlState,
    config: &crate::config::CrawlConfig,
) -> Result<VecDeque<CrawlQueue>> {
    let CrawlState {
        queue,
        max_depth,
    } = crawl_state;
    let mut crawl_queue = queue;
    let config = config.clone();

    // Extract links for next depth level if we haven't reached max depth
    if current_item.depth < max_depth {
        match extract_links(page.clone()).await {
            Ok(links) => {
                let filtered_links = super::crawler::extract_valid_urls(&links, &config);
                debug!(
                    target: "citescrape::links",
                    "Found {} links on {}, {} after filtering",
                    links.len(),
                    current_item.url,
                    filtered_links.len()
                );

                // Add new links to queue
                // Note: Deduplication is handled by orchestrator's DashSet
                for link_url in filtered_links {
                    // Normalize URL by stripping fragment for deduplication
                    let normalized_url = match normalize_url(&link_url) {
                        Ok(url) => url,
                        Err(e) => {
                            warn!(
                                target: "citescrape::links",
                                "Failed to normalize URL {}: {}, skipping",
                                link_url, e
                            );
                            continue;
                        }
                    };
                    
                    // Add to queue - deduplication happens at page_processor level (queue comparison)
                    // and orchestrator level (DashSet prevents re-visiting URLs)
                    if url::Url::parse(&normalized_url).is_ok() {
                        crawl_queue.push_back(CrawlQueue {
                            url: normalized_url,  // Store normalized URL (no fragment)
                            depth: current_item.depth + 1,
                        });
                    } else {
                        warn!(
                            target: "citescrape::links",
                            "Skipping invalid normalized URL: {normalized_url}"
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    target: "citescrape::links",
                    "Failed to extract links from {}: {}",
                    current_item.url,
                    e
                );
            }
        }
    }
    Ok(crawl_queue)
}
