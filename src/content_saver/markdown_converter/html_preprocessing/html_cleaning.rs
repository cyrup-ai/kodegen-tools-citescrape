//! HTML cleaning utilities for removing scripts, styles, ads, and non-content elements.
//!
//! This module provides aggressive HTML sanitization by removing:
//! - `<script>` and `<style>` tags and their contents
//! - Inline event handlers (onclick, onload, etc.)
//! - HTML comments
//! - Forms, iframes, and tracking elements
//! - Social media widgets, cookie notices, and advertisements
//! - Hidden elements (display:none, visibility:hidden)
//! - HTML5 semantic elements without markdown equivalents

use anyhow::{Context, Result};
use ego_tree::NodeId;
use html_escape::decode_html_entities;
use kuchiki::traits::TendrilSink;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

// Re-use the size limit from main_content_extraction
use super::main_content_extraction::MAX_HTML_SIZE;

// ============================================================================
// Regex Patterns for HTML Cleaning
// ============================================================================

// Compile regex patterns once at first use
// These are hardcoded patterns that will never fail to compile

static SCRIPT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<script[^>]*>.*?</script>").expect("SCRIPT_RE: hardcoded regex is valid")
});

static STYLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<style[^>]*>.*?</style>").expect("STYLE_RE: hardcoded regex is valid")
});

static EVENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"on\w+="[^"]*""#).expect("EVENT_RE: hardcoded regex is valid"));

static COMMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--.*?-->").expect("COMMENT_RE: hardcoded regex is valid"));

static FORM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<form[^>]*>.*?</form>").expect("FORM_RE: hardcoded regex is valid")
});

static IFRAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<iframe[^>]*>.*?</iframe>").expect("IFRAME_RE: hardcoded regex is valid")
});

static SOCIAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*(?:social|share|follow)[^"]*"[^>]*>.*?</div>"#)
        .expect("SOCIAL_RE: hardcoded regex is valid")
});

static COOKIE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<div[^>]*(?:id|class)="[^"]*(?:cookie|popup|modal|overlay)[^"]*"[^>]*>.*?</div>"#,
    )
    .expect("COOKIE_RE: hardcoded regex is valid")
});

static AD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*(?:id|class)="[^"]*(?:ad-|ads-|advertisement)[^"]*"[^>]*>.*?</div>"#)
        .expect("AD_RE: hardcoded regex is valid")
});

// Matches elements with display:none in style attribute (supports single/double quotes)
static HIDDEN_DISPLAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<[^>]+style\s*=\s*["'][^"']*display\s*:\s*none[^"']*["'][^>]*>.*?</[^>]+>"#)
        .expect("HIDDEN_DISPLAY: hardcoded regex is valid")
});

// Matches elements with boolean hidden attribute
static HIDDEN_ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<[^>]+\bhidden(?:\s|>|/|=)[^>]*>.*?</[^>]+>")
        .expect("HIDDEN_ATTR: hardcoded regex is valid")
});

// Matches elements with visibility:hidden in style attribute
static HIDDEN_VISIBILITY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<[^>]+style\s*=\s*["'][^"']*visibility\s*:\s*hidden[^"']*["'][^>]*>.*?</[^>]+>"#,
    )
    .expect("HIDDEN_VISIBILITY: hardcoded regex is valid")
});

static DETAILS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<details[^>]*>(.*?)</details>").expect("DETAILS_RE: hardcoded regex is valid")
});

static SEMANTIC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"<(/?)(?:article|section|aside|nav|header|footer|figure|figcaption|mark|time)[^>]*>",
    )
    .expect("SEMANTIC_RE: hardcoded regex is valid")
});

/// Matches img tags to extract and normalize their attributes
/// Captures: (1) all attributes before src, (2) src value, (3) all attributes after src
static IMG_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<img([^>]*?)\s+src="([^"]+)"([^>]*?)>"#)
        .expect("IMG_TAG_RE: hardcoded regex is valid")
});

/// Matches img tags where src comes first, then other attributes
static IMG_TAG_SRC_FIRST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<img\s+src="([^"]+)"([^>]*?)>"#)
        .expect("IMG_TAG_SRC_FIRST_RE: hardcoded regex is valid")
});

/// Extract alt text from img tag attributes
static IMG_ALT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\s+alt="([^"]*)""#)
        .expect("IMG_ALT_RE: hardcoded regex is valid")
});

/// Extract title attribute from img tag attributes
static IMG_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\s+title="([^"]*)""#)
        .expect("IMG_TITLE_RE: hardcoded regex is valid")
});

// Special case: needs to be compiled for closure captures in details processing
static SUMMARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<summary[^>]*>(.*?)</summary>").expect("SUMMARY_RE: hardcoded regex is valid")
});

/// Matches heading anchor links (Starlight/Astro documentation pattern)
/// Removes anchor links with screen-reader text that appear after headings
/// Pattern: <a class="sl-anchor-link">...</a> or similar variants
static HEADING_ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<a[^>]*class="[^"]*(?:anchor-link|sl-anchor-link|heading-link)[^"]*"[^>]*>.*?</a>"#
    ).expect("HEADING_ANCHOR_RE: hardcoded regex is valid")
});

/// Matches heading wrapper divs (Starlight/Astro documentation pattern)
/// Unwraps wrapper divs that contain headings and anchor links
/// Pattern: <div class="sl-heading-wrapper">...</div> or similar variants
static HEADING_WRAPPER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<div[^>]*class="[^"]*(?:heading-wrapper|sl-heading-wrapper)[^"]*"[^>]*>(.*?)</div>"#
    ).expect("HEADING_WRAPPER_RE: hardcoded regex is valid")
});

// Matches permalink anchor links within headings (GitHub, Docusaurus, Jekyll, Eleventy style)
// These anchors contain only permalink symbols: #, §, ¶ (and whitespace)
// Examples:
//   <a href="#section">#</a>
//   <a href="#heading" class="anchor">§</a>
//   <a href="#overview" aria-hidden="true">¶</a>
static HEADING_ANCHOR_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a[^>]*href=["']#[^"']*["'][^>]*>\s*[#§¶]+\s*</a>"#)
        .expect("HEADING_ANCHOR_MARKERS: hardcoded regex is valid")
});

// ============================================================================
// DOM-Based Interactive Element Removal
// ============================================================================

/// Remove interactive elements from HTML using DOM parsing and CSS selectors.
///
/// This function handles complex patterns that regex cannot reliably match:
/// - Elements with specific data attributes (`[data-citescrape-interactive]`, `[data-clipboard-*]`)
/// - Elements with specific CSS classes (`.copy`, `.copy-button`, `.theme-toggle`)
/// - Interactive form elements (`button`, `input`, `select`, `textarea`)
///
/// Uses the same pattern as `main_content_extraction.rs::remove_elements_from_html()`
/// for efficient O(s + n) removal where s = selectors, n = elements.
fn remove_interactive_elements_from_dom(html: &str) -> String {
    // Parse HTML to DOM
    let document = Html::parse_fragment(html);
    
    // Get the root element (fragment wraps content in a root node)
    let root = document.root_element();
    
    // Define all selectors for interactive elements to remove
    let interactive_selectors = &[
        // Citescrape tracking attributes (added during extraction)
        "[data-citescrape-interactive]",
        
        // Interactive form elements (not all are in <form> tags)
        "button",
        "input",
        "select",
        "textarea",
        
        // Dialog elements
        "dialog",
        
        // Clipboard.js and copy-to-clipboard patterns
        "[data-clipboard-target]",
        "[data-clipboard-text]",
        "[data-clipboard-action]",
        "[data-copied]",
        ".copy",
        ".copy-button",
        ".copy-code-button",
        ".copy-to-clipboard",
        
        // Theme toggles and UI chrome
        ".theme-toggle",
        "[data-theme-toggle]",
        ".mobile-menu-toggle",
        ".hamburger",
        ".menu-toggle",
        
        // Search overlays (not all caught by existing patterns)
        ".search-button",
        "[data-search-toggle]",
    ];
    
    // Remove matching elements from DOM
    remove_elements_from_html(&root, interactive_selectors)
}

/// Efficiently remove elements matching selectors from a DOM element's subtree.
///
/// This is adapted from `main_content_extraction.rs::remove_elements_from_html()`.
/// Overall complexity: O(s + n) instead of O(s × n²) from naive string replacement.
///
/// # Arguments
/// * `element` - Root element to process
/// * `remove_selectors` - CSS selectors to match for removal
///
/// # Returns
/// Serialized HTML string with matched elements removed
fn remove_elements_from_html(element: &ElementRef, remove_selectors: &[&str]) -> String {
    // Parse all selectors upfront - O(s)
    let parsed_selectors: Vec<Selector> = remove_selectors
        .iter()
        .filter_map(|&sel_str| match Selector::parse(sel_str) {
            Ok(s) => Some(s),
            Err(e) => {
                // Log but don't panic on dynamic selectors
                log::warn!("Failed to parse selector '{}': {}", sel_str, e);
                None
            }
        })
        .collect();

    // Build HashSet of all elements to remove (using NodeId for O(1) lookup) - O(n)
    let mut to_remove: HashSet<NodeId> = HashSet::new();
    for sel in &parsed_selectors {
        for elem in element.select(sel) {
            to_remove.insert(elem.id());
        }
    }

    // Serialize HTML while skipping removed elements - O(n)
    let mut result = String::new();
    serialize_html_excluding(element, &to_remove, &mut result);
    result
}

/// Recursively serialize an element and its descendants to HTML,
/// skipping elements in the removal set.
///
/// This is adapted from `main_content_extraction.rs::serialize_html_excluding()`.
/// Preserves HTML structure (tags, attributes, nesting) while excluding unwanted elements.
fn serialize_html_excluding(
    element: &ElementRef,
    to_remove: &HashSet<NodeId>,
    output: &mut String,
) {
    // Check if this element should be removed
    if to_remove.contains(&element.id()) {
        return; // Skip this element and all its children
    }

    // Serialize this element's children
    for child in element.children() {
        use scraper::node::Node;

        match child.value() {
            Node::Text(text) => {
                // Escape HTML special characters in text content
                for ch in text.chars() {
                    match ch {
                        '<' => output.push_str("&lt;"),
                        '>' => output.push_str("&gt;"),
                        '&' => output.push_str("&amp;"),
                        '"' => output.push_str("&quot;"),
                        c => output.push(c),
                    }
                }
            }
            Node::Element(_) => {
                // Element node - check if it should be removed
                if let Some(child_elem) = ElementRef::wrap(child) {
                    if to_remove.contains(&child_elem.id()) {
                        // Skip this element and its children
                        continue;
                    }

                    // Serialize the element with its tags and attributes
                    let elem_name = child_elem.value().name();
                    output.push('<');
                    output.push_str(elem_name);

                    // Serialize attributes
                    for (name, value) in child_elem.value().attrs() {
                        output.push(' ');
                        output.push_str(name);
                        output.push_str("=\"");
                        // Escape attribute value
                        for ch in value.chars() {
                            match ch {
                                '"' => output.push_str("&quot;"),
                                '&' => output.push_str("&amp;"),
                                '<' => output.push_str("&lt;"),
                                '>' => output.push_str("&gt;"),
                                c => output.push(c),
                            }
                        }
                        output.push('"');
                    }
                    output.push('>');

                    // Check if this is a void element (self-closing)
                    const VOID_ELEMENTS: &[&str] = &[
                        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
                        "param", "source", "track", "wbr",
                    ];

                    if VOID_ELEMENTS.contains(&elem_name) {
                        // Void element - no closing tag needed
                        continue;
                    }

                    // Recursively serialize children
                    serialize_html_excluding(&child_elem, to_remove, output);

                    // Closing tag (only for non-void elements)
                    output.push_str("</");
                    output.push_str(elem_name);
                    output.push('>');
                }
            }
            Node::Comment(comment) => {
                // Comments are already removed by regex, but preserve if present
                output.push_str("<!--");
                output.push_str(comment);
                output.push_str("-->");
            }
            _ => {
                // Other node types (Document, Doctype, ProcessingInstruction) - skip
            }
        }
    }
}

// ============================================================================
// HTML Cleaning Function
// ============================================================================

/// Preprocess Expressive Code blocks by extracting clean code text
///
/// Expressive Code (Astro's syntax highlighter) generates complex nested HTML:
/// ```html
/// <div class="expressive-code">
///   <pre data-language="rust">
///     <code>
///       <div class="ec-line">
///         <div class="code">
///           <span style="--0:#C792EA">pub</span>
///           ...
///         </div>
///       </div>
///     </code>
///   </pre>
///   <button data-code="pub struct Button {...}">Copy</button>
/// </div>
/// ```
///
/// This function extracts the clean code text and language, replacing the entire
/// structure with a standard `<pre><code class="language-{lang}">{text}</code></pre>`
/// that html2md can properly convert to markdown code fences.
///
/// # Strategy (Dual-Path Extraction)
///
/// 1. **Primary**: Extract from `button[data-code]` attribute (most reliable)
///    - The copy button contains the complete, clean code text
///    - Already unescaped and formatted correctly
///    - Fallback if button not found or data-code empty
///
/// 2. **Fallback**: Extract text from `.ec-line` elements
///    - Each `.ec-line` represents one line of code
///    - Extract text content from each line (strips spans/styling)
///    - Join with newlines to reconstruct code
///
/// # Arguments
///
/// * `html` - HTML string potentially containing Expressive Code blocks
///
/// # Returns
///
/// * `Ok(String)` - HTML with Expressive Code blocks replaced by standard pre/code tags
/// * `Err(anyhow::Error)` - If HTML parsing or serialization fails
fn preprocess_expressive_code(html: &str) -> Result<String> {
    // Parse HTML into mutable DOM
    let document = kuchiki::parse_html().one(html.to_string());

    // Select all Expressive Code blocks
    let selector = "div.expressive-code";
    
    // Must collect before iteration because we'll detach nodes
    let matches: Vec<_> = document
        .select(selector)
        .map_err(|()| anyhow::anyhow!("Invalid CSS selector: {}", selector))?
        .collect();
    
    // If no Expressive Code blocks found, return original HTML
    if matches.is_empty() {
        return Ok(html.to_string());
    }

    log::debug!("Found {} Expressive Code blocks to preprocess", matches.len());

    for ec_block in matches {
        let node = ec_block.as_node();
        
        // Extract language hint from pre[data-language]
        let language = {
            let pre_matches: Vec<_> = node
                .select("pre[data-language]")
                .map_err(|()| anyhow::anyhow!("Invalid selector: pre[data-language]"))?
                .collect();
            
            pre_matches
                .first()
                .and_then(|pre_elem| {
                    let attrs = pre_elem.attributes.borrow();
                    attrs.get("data-language").map(String::from)
                })
                .unwrap_or_else(|| String::from("plaintext"))
        };
        
        // Try Strategy 1: Extract from button[data-code] attribute
        let code_text = {
            let button_matches: Vec<_> = node
                .select("button[data-code]")
                .map_err(|()| anyhow::anyhow!("Invalid selector: button[data-code]"))?
                .collect();
            
            button_matches
                .first()
                .and_then(|button_elem| {
                    let attrs = button_elem.attributes.borrow();
                    attrs.get("data-code").map(String::from)
                })
        };
        
        let code_text = if let Some(text) = code_text {
            log::debug!("Extracted code from data-code attribute (length: {})", text.len());
            text
        } else {
            // Strategy 2: Fallback to .ec-line extraction
            log::debug!("Falling back to .ec-line text extraction");
            
            let lines: Vec<String> = match node.select(".ec-line") {
                Ok(iter) => iter.map(|line_elem| line_elem.text_contents()).collect(),
                Err(_) => Vec::new(),
            };
            
            if lines.is_empty() {
                log::warn!("Failed to extract code from Expressive Code block");
                String::new()
            } else {
                log::debug!("Extracted {} lines from .ec-line elements", lines.len());
                lines.join("\n")
            }
        };
        
        // HTML-escape the code text
        let escaped_code = html_escape::encode_text(&code_text).to_string();
        
        // Create standard pre/code element
        let replacement_html = format!(
            "<pre><code class=\"language-{}\">{}</code></pre>",
            language,
            escaped_code
        );
        
        let replacement = kuchiki::parse_html().one(replacement_html);
        
        // Insert replacement and remove original
        for child in replacement.children() {
            node.insert_before(child);
        }
        node.detach();
        
        log::debug!(
            "Replaced Expressive Code block with standard pre/code (language: {}, {} chars)",
            language,
            code_text.len()
        );
    }
    
    // Serialize back to HTML
    let mut output = Vec::new();
    document
        .serialize(&mut output)
        .context("Failed to serialize HTML after Expressive Code preprocessing")?;
    
    String::from_utf8(output)
        .context("Failed to convert HTML bytes to UTF-8 after Expressive Code preprocessing")
}

/// Clean HTML content by removing scripts, styles, ads, tracking, and other non-content elements
///
/// This function performs aggressive cleaning including:
/// - Removing `<script>` and `<style>` tags and their contents
/// - Removing inline event handlers (onclick, onload, etc.)
/// - Removing HTML comments  
/// - Removing `<form>`, `<iframe>` elements
/// - Removing social media widgets, cookie notices, and ads
/// - Removing hidden elements (display:none, visibility:hidden)
/// - Converting `<details>`/`<summary>` to markdown-friendly format
/// - Removing semantic HTML5 elements without markdown equivalents
/// - **Decoding HTML entities** (&amp; → &, &lt; → <, etc.)
///
/// # Arguments
/// * `html` - HTML string to clean
///
/// # Returns
/// * `Ok(String)` - Cleaned HTML string
/// * `Err(anyhow::Error)` - If input exceeds size limit
///
/// # Example
/// ```
/// let html = r#"<div><script>alert('xss')</script><p>Hello &amp; goodbye</p></div>"#;
/// let clean = clean_html_content(html);
/// // Result: <div><p>Hello & goodbye</p></div>
/// ```
pub fn clean_html_content(html: &str) -> Result<String> {
    // Validate input size to prevent memory exhaustion
    if html.len() > MAX_HTML_SIZE {
        return Err(anyhow::anyhow!(
            "HTML input too large: {} bytes ({:.2} MB). Maximum allowed: {} bytes ({} MB). \
             This protects against memory exhaustion attacks.",
            html.len(),
            html.len() as f64 / 1_000_000.0,
            MAX_HTML_SIZE,
            MAX_HTML_SIZE / (1024 * 1024)
        ));
    }

    // ============================================================================
    // STEP 1: Preprocess Expressive Code blocks (DOM-based, before regex cleaning)
    // ============================================================================
    let html = preprocess_expressive_code(html)?;

    // ============================================================================
    // STEP 2: Regex-based cleaning (existing code)
    // ============================================================================
    // Use Cow to avoid unnecessary allocations
    let result = Cow::Borrowed(&html);

    // Remove script tags and their contents
    let result = SCRIPT_RE.replace_all(&result, "");

    // Remove style tags and their contents
    let result = STYLE_RE.replace_all(&result, "");

    // Remove inline event handlers
    let result = EVENT_RE.replace_all(&result, "");

    // Remove comments
    let result = COMMENT_RE.replace_all(&result, "");

    // Remove forms
    let result = FORM_RE.replace_all(&result, "");

    // Remove iframes
    let result = IFRAME_RE.replace_all(&result, "");

    // Remove social media widgets and buttons
    let result = SOCIAL_RE.replace_all(&result, "");

    // Remove cookie notices and popups
    let result = COOKIE_RE.replace_all(&result, "");

    // Remove ads
    let result = AD_RE.replace_all(&result, "");

    // Remove hidden elements (multiple patterns for comprehensive matching)
    let result = HIDDEN_DISPLAY.replace_all(&result, "");
    let result = HIDDEN_ATTR.replace_all(&result, "");
    let result = HIDDEN_VISIBILITY.replace_all(&result, "");

    // Remove permalink anchor markers from headings (GitHub, Docusaurus, Jekyll, Eleventy)
    // These are visual indicators for deep linking, not content
    // Must happen before html2md conversion to prevent "## # Heading" duplication
    let result = HEADING_ANCHOR_MARKERS.replace_all(&result, "");

    // Unwrap heading wrapper divs (must be done before removing anchors)
    let result = HEADING_WRAPPER_RE.replace_all(&result, "$1");

    // Remove heading anchor links
    let result = HEADING_ANCHOR_RE.replace_all(&result, "");

    // Handle HTML5 details/summary elements by extracting their content
    // These don't convert well to markdown
    let result = Cow::Owned(
        DETAILS_RE
            .replace_all(&result, |caps: &regex::Captures| {
                let content = &caps[1];
                // Extract summary text if present
                if let Some(summary_match) = SUMMARY_RE.captures(content) {
                    let summary_text = &summary_match[1];
                    let remaining = SUMMARY_RE.replace(content, "");
                    
                    // Use level 3 heading for summary, properly formatted
                    let summary_trimmed = summary_text.trim();
                    let content_trimmed = remaining.trim();
                    
                    if summary_trimmed.is_empty() {
                        // No summary text - just output the content
                        format!("\n\n{}\n\n", content_trimmed)
                    } else if content_trimmed.is_empty() {
                        // Summary but no content - just output summary as heading
                        format!("\n\n### {}\n\n", summary_trimmed)
                    } else {
                        // Both summary and content present
                        format!("\n\n### {}\n\n{}\n\n", summary_trimmed, content_trimmed)
                    }
                } else {
                    // No summary element found - just output the content
                    format!("\n\n{}\n\n", content.trim())
                }
            })
            .into_owned(),
    );

    // Remove any remaining HTML5 semantic elements that don't have markdown equivalents
    let result = SEMANTIC_RE.replace_all(&result, "");

    // Normalize image tags to remove HTML5 attributes that confuse html2md
    let result = Cow::Owned(normalize_image_tags(&result));

    // ========================================================================
    // STAGE 2: DOM-based removal (handles complex selectors)
    // ========================================================================
    
    // Remove interactive elements (buttons, inputs, data attributes, etc.)
    let result = Cow::Owned(remove_interactive_elements_from_dom(&result));

    // ========================================================================
    // FINAL STEP: Decode HTML entities
    // ========================================================================
    
    // Decode HTML entities (&amp; → &, &lt; → <, etc.)
    let result = decode_html_entities(&result);

    // Convert final Cow to owned String for return
    Ok(result.into_owned())
}

/// Normalize image tags by stripping modern HTML5 attributes that confuse html2md
///
/// Removes problematic attributes while preserving essential ones:
/// - **Keep:** src, alt, title
/// - **Remove:** decoding, fetchpriority, loading, width, height, class, style, id
///
/// This preprocessing step ensures html2md v0.2 can properly convert images.
///
/// # Arguments
/// * `html` - HTML string containing img tags
///
/// # Returns
/// * Normalized HTML with simplified img tags
///
/// # Example
/// ```rust
/// let html = r#"<img alt="demo" decoding="async" src="/image.jpg" width="800">"#;
/// let result = normalize_image_tags(html);
/// // Result: <img alt="demo" src="/image.jpg">
/// ```
pub fn normalize_image_tags(html: &str) -> String {
    // Process images with src in middle/end
    let result = IMG_TAG_RE.replace_all(html, |caps: &regex::Captures| {
        let before_src = &caps[1];
        let src = &caps[2];
        let after_src = &caps[3];
        
        // Combine all attributes
        let all_attrs = format!("{} {}", before_src, after_src);
        
        // Extract alt and title if present
        let alt = IMG_ALT_RE
            .captures(&all_attrs)
            .and_then(|c| c.get(1))
            .map(|m| format!(r#" alt="{}""#, m.as_str()))
            .unwrap_or_default();
        
        let title = IMG_TITLE_RE
            .captures(&all_attrs)
            .and_then(|c| c.get(1))
            .map(|m| format!(r#" title="{}""#, m.as_str()))
            .unwrap_or_default();
        
        // Rebuild as simple img tag with only essential attributes
        format!(r#"<img src="{}"{}{}>"#, src, alt, title)
    });
    
    // Process images with src first
    let result = IMG_TAG_SRC_FIRST_RE.replace_all(&result, |caps: &regex::Captures| {
        let src = &caps[1];
        let attrs = &caps[2];
        
        // Extract alt and title if present
        let alt = IMG_ALT_RE
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| format!(r#" alt="{}""#, m.as_str()))
            .unwrap_or_default();
        
        let title = IMG_TITLE_RE
            .captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| format!(r#" title="{}""#, m.as_str()))
            .unwrap_or_default();
        
        format!(r#"<img src="{}"{}{}>"#, src, alt, title)
    });
    
    result.into_owned()
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_scripts() -> Result<()> {
        let html = r#"<div><script>alert('xss')</script><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("script"));
        assert!(!result.contains("alert"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_styles() -> Result<()> {
        let html = r#"<div><style>.test { color: red; }</style><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("style"));
        assert!(!result.contains("color: red"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_event_handlers() -> Result<()> {
        let html = r#"<button onclick="alert('click')">Click me</button>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("onclick"));
        assert!(result.contains("Click me"));
        Ok(())
    }

    #[test]
    fn test_removes_comments() -> Result<()> {
        let html = r"<div><!-- This is a comment --><p>Content</p></div>";
        let result = clean_html_content(html)?;
        assert!(!result.contains("This is a comment"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_decodes_html_entities() -> Result<()> {
        let html = r"<p>Hello &amp; goodbye &lt;test&gt;</p>";
        let result = clean_html_content(html)?;
        assert!(result.contains("&"));
        assert!(result.contains("<test>"));
        Ok(())
    }

    #[test]
    fn test_removes_forms() -> Result<()> {
        let html = r#"<div><form><input type="text"></form><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("form"));
        assert!(!result.contains("input"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_iframes() -> Result<()> {
        let html = r#"<div><iframe src="ads.html"></iframe><p>Content</p></div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("iframe"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_removes_hidden_elements() -> Result<()> {
        let html = r#"<div style="display:none">Hidden</div><p>Visible</p>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("Hidden"));
        assert!(result.contains("Visible"));
        Ok(())
    }

    #[test]
    fn test_converts_details_summary() -> Result<()> {
        let html = r"<details><summary>Click to expand</summary>Hidden content</details>";
        let result = clean_html_content(html)?;
        assert!(!result.contains("<details>"));
        assert!(!result.contains("<summary>"));
        assert!(result.contains("Click to expand"));
        assert!(result.contains("Hidden content"));
        Ok(())
    }

    #[test]
    fn test_size_limit_enforcement() {
        // Create HTML larger than MAX_HTML_SIZE
        let large_html = "x".repeat(MAX_HTML_SIZE + 1);
        let result = clean_html_content(&large_html);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("HTML input too large"));
    }
}
