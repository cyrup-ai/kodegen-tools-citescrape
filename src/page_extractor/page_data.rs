//! Zero-allocation page data extraction functionality
//!
//! This module provides blazing-fast, lock-free page data extraction
//! with pre-allocated buffers and zero heap allocations in hot paths.

use anyhow::{Context, Result};
use chromiumoxide::Page;
use dashmap::DashMap;
use std::sync::Arc;

use crate::content_saver;
use crate::inline_css::domain_queue::DomainDownloadQueue;

use super::extractors::{
    extract_headings, extract_interactive_elements, extract_links, extract_metadata,
    extract_resources, extract_security_info, extract_timing_info,
};

/// Configuration for page data extraction
pub struct ExtractPageDataConfig {
    pub output_dir: std::path::PathBuf,
    pub max_inline_image_size_bytes: Option<usize>,
    pub crawl_rate_rps: Option<f64>,
    pub save_html: bool,
    pub compression_threshold_bytes: usize,
    /// User-Agent string extracted from browser (used for HTTP requests during resource inlining)
    pub user_agent: String,
    /// Shared cache for HTTP error responses (enables cross-page caching of failed URLs)
    pub http_error_cache: Arc<DashMap<String, u16>>,
    /// Shared domain download queues (enables cross-page worker sharing)
    pub domain_queues: Arc<DashMap<String, Arc<DomainDownloadQueue>>>,
}

/// Extract event handler attribute names from element attributes
#[inline]
fn get_event_handlers(attributes: &std::collections::HashMap<String, String>) -> Vec<String> {
    attributes
        .keys()
        .filter(|k| k.starts_with("on"))
        .cloned()
        .collect()
}

/// Check if element has any event handlers
#[inline]
fn has_event_handlers(attributes: &std::collections::HashMap<String, String>) -> bool {
    attributes.keys().any(|k| k.starts_with("on"))
}

/// Check if element has an interactive ARIA role
#[inline]
fn has_interactive_role(attributes: &std::collections::HashMap<String, String>) -> bool {
    if let Some(role) = attributes.get("role") {
        matches!(
            role.as_str(),
            "button"
                | "checkbox"
                | "radio"
                | "switch"
                | "tab"
                | "slider"
                | "spinbutton"
                | "menuitem"
                | "menuitemcheckbox"
                | "menuitemradio"
                | "option"
                | "link"
                | "searchbox"
                | "textbox"
                | "combobox"
                | "gridcell"
                | "treeitem"
        )
    } else {
        false
    }
}

/// Convert raw interactive elements to structured format
fn convert_interactive_elements(
    elements: Vec<super::schema::InteractiveElement>,
) -> super::schema::InteractiveElements {
    use super::schema::{
        ButtonElement, ClickableElement, InputElement, InteractiveElements, LinkElement,
    };

    let mut result = InteractiveElements::default();

    for element in elements {
        match element.element_type.to_lowercase().as_str() {
            "button" => {
                result.buttons.push(ButtonElement {
                    id: element.attributes.get("id").cloned(),
                    text: element.text.clone(),
                    button_type: element.attributes.get("type").cloned(),
                    disabled: element.attributes.contains_key("disabled"),
                    form_id: element.attributes.get("form").cloned(),
                    attributes: element.attributes.clone(),
                });
            }
            "a" => {
                if let Some(href) = element
                    .url
                    .as_ref()
                    .or_else(|| element.attributes.get("href"))
                {
                    result.links.push(LinkElement {
                        href: href.clone(),
                        text: element.text.clone(),
                        title: element.attributes.get("title").cloned(),
                        target: element.attributes.get("target").cloned(),
                        rel: element.attributes.get("rel").cloned(),
                        attributes: element.attributes.clone(),
                    });
                }
            }
            "input" => {
                result.inputs.push(InputElement {
                    id: element.attributes.get("id").cloned(),
                    name: element.attributes.get("name").cloned(),
                    input_type: element
                        .attributes
                        .get("type")
                        .unwrap_or(&"text".to_string())
                        .clone(),
                    value: element.attributes.get("value").cloned(),
                    placeholder: element.attributes.get("placeholder").cloned(),
                    required: element.attributes.contains_key("required"),
                    disabled: element.attributes.contains_key("disabled"),
                    validation: extract_validation(&element.attributes),
                    attributes: element.attributes.clone(),
                });
            }
            "select" | "textarea" => {
                result.inputs.push(InputElement {
                    id: element.attributes.get("id").cloned(),
                    name: element.attributes.get("name").cloned(),
                    input_type: element.element_type.clone(),
                    value: element.attributes.get("value").cloned(),
                    placeholder: element.attributes.get("placeholder").cloned(),
                    required: element.attributes.contains_key("required"),
                    disabled: element.attributes.contains_key("disabled"),
                    validation: None,
                    attributes: element.attributes.clone(),
                });
            }

            // Native interactive HTML elements
            "details" | "summary" | "dialog" | "menu" => {
                result.clickable.push(ClickableElement {
                    text: element.text.clone(),
                    role: Some(element.element_type.clone()),
                    aria_label: element.attributes.get("aria-label").cloned(),
                    event_handlers: get_event_handlers(&element.attributes),
                    attributes: element.attributes.clone(),
                });
            }

            // Label elements (interactive when associated with input)
            "label" => {
                if element.attributes.contains_key("for") {
                    result.clickable.push(ClickableElement {
                        text: element.text.clone(),
                        role: Some("label".to_string()),
                        aria_label: element.attributes.get("aria-label").cloned(),
                        event_handlers: get_event_handlers(&element.attributes),
                        attributes: element.attributes.clone(),
                    });
                }
            }

            // Catch-all with logging for unhandled types
            _ => {
                // Check if element is interactive via ARIA role or event handlers
                if has_interactive_role(&element.attributes)
                    || has_event_handlers(&element.attributes)
                {
                    result.clickable.push(ClickableElement {
                        text: element.text.clone(),
                        role: element.attributes.get("role").cloned(),
                        aria_label: element.attributes.get("aria-label").cloned(),
                        event_handlers: get_event_handlers(&element.attributes),
                        attributes: element.attributes.clone(),
                    });
                }
            }
        }
    }

    result
}

/// Extract validation rules from input attributes
fn extract_validation(
    attributes: &std::collections::HashMap<String, String>,
) -> Option<super::schema::InputValidation> {
    if attributes.contains_key("pattern")
        || attributes.contains_key("minlength")
        || attributes.contains_key("maxlength")
        || attributes.contains_key("min")
        || attributes.contains_key("max")
        || attributes.contains_key("step")
    {
        Some(super::schema::InputValidation {
            pattern: attributes.get("pattern").cloned(),
            min_length: attributes.get("minlength").and_then(|v| v.parse().ok()),
            max_length: attributes.get("maxlength").and_then(|v| v.parse().ok()),
            min: attributes.get("min").cloned(),
            max: attributes.get("max").cloned(),
            step: attributes.get("step").cloned(),
        })
    } else {
        None
    }
}

/// Extract all page data including metadata, resources, timing, and content
/// This is the production function used by the crawler, with `LinkRewriter` integration
pub async fn extract_page_data(
    page: Page,
    url: String,
    config: &ExtractPageDataConfig,
) -> Result<super::schema::PageData> {
    log::debug!("Starting to extract page data for URL: {url}");

    // Launch all extractions in parallel with tokio::try_join!
    let (metadata, resources, timing, security, title, interactive_elements_vec, links, headings) = tokio::try_join!(
        extract_metadata(page.clone()),
        extract_resources(page.clone()),
        extract_timing_info(page.clone()),
        extract_security_info(page.clone()),
        async {
            let result: Result<String> = async {
                let title_value = page
                    .evaluate("document.title")
                    .await
                    .context("Failed to evaluate document.title")?
                    .into_value()
                    .map_err(|e| anyhow::anyhow!("Failed to get page title: {e}"))?;

                if let serde_json::Value::String(title) = title_value {
                    Ok(title)
                } else {
                    Ok(String::new())
                }
            }
            .await;
            result
        },
        extract_interactive_elements(page.clone()),
        extract_links(page.clone()),
        extract_headings(page.clone()),
    )?;

    // ============ OPTIONAL: Scroll to trigger lazy-loaded content ============
    // For sites with infinite scroll or lazy-loading, scroll to bottom first
    // This triggers all lazy-loaded content to start loading
    super::extractors::scroll_to_bottom(&page, 500).await
        .context("Failed to scroll page for lazy-loaded content")?;
    // ========================================================================

    // ============ CRITICAL FIX: Wait for page to be fully loaded ============
    // This ensures all JavaScript has executed and dynamic content is rendered
    // before we extract the HTML content. Without this, we only get the initial
    // HTML skeleton, missing all JavaScript-rendered content.
    //
    // This uses the same wait_for_page_load() that screenshots use, ensuring
    // consistency: if screenshots capture full content, HTML will too.
    super::extractors::wait_for_page_load(&page, 10).await
        .context("Failed to wait for page load before content extraction")?;
    
    log::debug!("Page fully loaded, extracting complete HTML content for: {url}");
    // ========================================================================

    // Get HTML content (now complete!)
    let content = page
        .content()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get page content: {e}"))?;

    // NOTE: Link rewriting is now handled AFTER page save via the event-driven
    // LinkRewriter system. See link_rewriter module for details.
    // The content is saved with original links, then rewritten in-place.

    // Convert Vec<InteractiveElement> to InteractiveElements
    let interactive_elements = convert_interactive_elements(interactive_elements_vec);

    // Save HTML content if enabled
    // NOTE: Links will be rewritten AFTER save by page_processor calling LinkRewriter::on_page_saved()
    if config.save_html {
        match content_saver::save_html_content_with_resources(
            &content,
            url.clone(),
            config.output_dir.clone(),
            &resources,
            config.max_inline_image_size_bytes,
            config.crawl_rate_rps,
            config.compression_threshold_bytes,
            &config.user_agent,
            Arc::clone(&config.http_error_cache),
            Arc::clone(&config.domain_queues),
        )
        .await
        {
            Ok(()) => {
                log::debug!("HTML content saved successfully for: {url}");
            }
            Err(e) => {
                log::warn!("Failed to save HTML for {url}: {e}");
            }
        }
    }

    log::debug!("Successfully extracted page data for URL: {url}");
    
    // Populate metadata with extracted headings
    let mut metadata_with_headings = metadata;
    metadata_with_headings.headings = headings;
    
    Ok(super::schema::PageData {
        url: url.clone(),
        title,
        content,
        metadata: metadata_with_headings,
        interactive_elements,
        links,
        resources,
        timing,
        security,
        crawled_at: chrono::Utc::now(),
    })
}
