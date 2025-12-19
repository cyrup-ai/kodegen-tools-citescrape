//! HTML cleaning utilities for removing scripts, styles, ads, and non-content elements.
//!
//! This module provides aggressive HTML sanitization by removing:
//! - `<script>` and `<style>` tags and their contents
//! - Forms, iframes, and tracking elements
//! - Social media widgets, cookie notices, and advertisements
//! - HTML5 semantic elements without markdown equivalents
//!
//! Note: Event handler attributes and HTML comments are automatically ignored
//! by htmd's DOM-based text extraction and do not require explicit removal.

use anyhow::{Context, Result};
use ego_tree::NodeId;
use htmlentity::entity::{decode, ICodedDataTrait};
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

// NOTE: FORM_RE, IFRAME_RE, SOCIAL_RE, COOKIE_RE, AD_RE have been removed.
// Widget filtering (social, cookie notices, ads) is now handled by htmd element handlers:
// - src/content_saver/markdown_converter/htmd/element_handler/div.rs
// - src/content_saver/markdown_converter/htmd/element_handler/section.rs
// - src/content_saver/markdown_converter/htmd/element_handler/aside.rs
// These handlers use is_widget_element() from element_util.rs to filter widget elements.

// Remove empty <code> and <pre> elements (whitespace-only content)
// These are often styling placeholders that break markdown conversion
static EMPTY_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<code[^>]*>\s*</code>")
        .expect("EMPTY_CODE: hardcoded regex is valid")
});

static EMPTY_PRE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<pre[^>]*>\s*</pre>")
        .expect("EMPTY_PRE: hardcoded regex is valid")
});

static EMPTY_PRE_CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<pre[^>]*>\s*<code[^>]*>\s*</code>\s*</pre>")
        .expect("EMPTY_PRE_CODE: hardcoded regex is valid")
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



// Special case: needs to be compiled for closure captures in details processing
static SUMMARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<summary[^>]*>(.*?)</summary>").expect("SUMMARY_RE: hardcoded regex is valid")
});

// ============================================================================
// CSS Selectors for Content Protection
// ============================================================================

// Pre-compiled CSS selectors for content elements that should be preserved
// during interactive element removal. These simple tag selectors never fail to parse.

static PRE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("pre").expect("PRE_SELECTOR: 'pre' is a valid CSS selector")
});

static UL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("ul").expect("UL_SELECTOR: 'ul' is a valid CSS selector")
});

static OL_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("ol").expect("OL_SELECTOR: 'ol' is a valid CSS selector")
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
/// **Code Block Protection**: This function now preserves `<pre><code>` blocks even if
/// they're wrapped in containers that match removal selectors (e.g., `.copy-to-clipboard`).
///
/// Uses the same pattern as `main_content_extraction.rs::remove_elements_from_html()`
/// for efficient O(s + n) removal where s = selectors, n = elements.
fn remove_interactive_elements_from_dom(html: &str) -> String {
    // Parse the original HTML (code blocks are intact for detection)
    let document = Html::parse_fragment(html);
    
    // Get the root element (fragment wraps content in a root node)
    let root = document.root_element();
    
    // Define all selectors for interactive elements to remove
    let interactive_selectors = &[
        // ========================================================================
        // EXISTING SELECTORS (keep as-is)
        // ========================================================================
        
        // NOTE: [data-citescrape-interactive] was REMOVED - it was deleting ALL links!
        // That attribute is for discovery/extraction, not deletion.
        
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
        
        // ========================================================================
        // NEW SELECTORS (add these)
        // ========================================================================
        
        // ARIA role-based (semantic buttons that aren't <button> tags)
        // IMPORTANT: Use :not(a) to preserve <a role="button" href="..."> navigation links
        // which are legitimate links styled as buttons, not actual interactive buttons
        ":not(a)[role='button']",
        
        // ARIA label patterns for common UI actions
        // Note: CSS selectors support substring matching with *= operator
        "[aria-label*='Copy']",     // Catches "Copy page", "Copy code", "Copy to clipboard"
        "[aria-label*='copy']",     // Lowercase variant
        // Share buttons (but preserve share links that navigate to share pages)
        ":not(a)[aria-label*='Share']",
        ":not(a)[aria-label*='share']",
        // Note: "Edit" labels often appear on legitimate navigation links (e.g., "Edit on GitHub")
        // Use :not(a) to preserve anchor-based edit links while removing button-based ones
        ":not(a)[aria-label*='Edit']",
        ":not(a)[aria-label*='edit']",
        "[aria-label*='Print']",    // Print page buttons
        "[aria-label*='print']",
        
        // Page-level action buttons (common across frameworks)
        ".edit-this-page",
        ".edit-page-link",
        ".edit-page-button",
        ".share-page",
        ".share-page-button",
        ".share-button",
        ".print-button",
        ".print-page",
        
        // Documentation framework-specific patterns
        ".sl-copy-page-button",      // Starlight (Astro)
        ".vp-copy-button",           // VitePress
        ".vp-doc-footer-before",     // VitePress footer actions
        ".nextra-copy-button",       // Nextra
        ".nextra-edit-page",         // Nextra edit page
        ".theme-doc-footer",         // Docusaurus footer
        
        // ========================================================================
        // UI ARTIFACT REMOVAL (Issue #004) - Additional Patterns
        // ========================================================================
        
        // Data attribute patterns for interactive elements
        "[data-action]",                   // Generic action buttons
        "[data-command]",                  // Command palette triggers
        
        // AI assistance buttons (common across modern documentation)
        "[aria-label*='AI']",
        "[aria-label*='ai']",
        ".ai-assist",
        ".ai-assistant",
        ".ai-button",
        ".ask-ai",
        "[data-ai-action]",
        
        // Copy button variants (additional patterns)
        ".copy-code",
        ".copy-snippet",
        ".clipboard-copy",
        ".clipboard-button",
        "[title*='Copy']",
        "[title*='copy']",
        "[data-copy]",
        "[data-copy-code]",
        
        // Code block toolbars and action buttons
        ".code-toolbar",
        ".code-actions",
        ".code-header",
        ".toolbar-item",
        "[class*='code-button']",
        "[class*='code-action']",
        
        // Documentation framework patterns
        ".astro-code-toolbar",             // Astro/Starlight
        ".shiki-toolbar",                  // Shiki syntax highlighter
        ".prism-toolbar",                  // Prism.js
        ".hljs-toolbar",                   // Highlight.js
        ".monaco-toolbar",                 // Monaco editor
        ".cm-toolbar",                     // CodeMirror
        
        // Accessibility/screen-reader-only elements (often contain UI instructions)
        ".sr-only",
        ".screen-reader-only",
        ".visually-hidden",
        "[aria-hidden='true'][class*='button']",
        "[aria-hidden='true'][class*='action']",
        
        // Modern documentation platforms
        ".docusaurus-button",              // Docusaurus
        ".nextra-button",                  // Nextra
        ".vitepress-button",               // VitePress
        ".mkdocs-button",                  // MkDocs
        ".sphinx-button",                  // Sphinx
        ".hugo-button",                    // Hugo
        
        // Generic action containers
        "[class*='actions']",
        "[class*='toolbar']",
        "[class*='controls']",
        "[id*='actions']",
        "[id*='toolbar']",
        "[id*='controls']",
    ];
    
    // Remove interactive elements with unwrap support for code block containers
    remove_elements_from_html(&root, interactive_selectors)
}

/// Efficiently remove elements matching selectors from a DOM element's subtree.
///
/// This is adapted from `main_content_extraction.rs::remove_elements_from_html()`.
/// Overall complexity: O(s + n) instead of O(s × n²) from naive string replacement.
///
/// **Code Block Protection**: When a matched element contains `<pre>` code blocks,
/// the container is "unwrapped" (children serialized, container tags skipped) instead
/// of being fully removed. Non-code-block children are still removed.
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
    
    // Use pre-compiled static selectors for content elements that should be protected
    // These are parsed once at first use via LazyLock
    let pre_selector = &*PRE_SELECTOR;
    let ul_selector = &*UL_SELECTOR;
    let ol_selector = &*OL_SELECTOR;

    // Build HashSet of all elements to remove
    let mut to_remove: HashSet<NodeId> = HashSet::new();
    let mut to_unwrap: HashSet<NodeId> = HashSet::new();

    for sel in &parsed_selectors {
        for elem in element.select(sel) {
            // ✅ FIX: Check for ALL content elements (pre, ul, ol)
            let contains_pre = elem.select(pre_selector).next().is_some();
            let contains_ul = elem.select(ul_selector).next().is_some();
            let contains_ol = elem.select(ol_selector).next().is_some();
            let contains_content = contains_pre || contains_ul || contains_ol;
            
            if contains_content {
                // Don't remove the container - unwrap it instead
                // This preserves the content (pre/ul/ol) while removing the wrapper
                to_unwrap.insert(elem.id());
                
                // Remove non-content children (decorative elements, buttons, etc.)
                for child in elem.children() {
                    if let Some(child_elem) = ElementRef::wrap(child) {
                        let child_name = child_elem.value().name();
                        
                        // Check if this child is a content element or contains one
                        let is_content_element = matches!(child_name, "pre" | "ul" | "ol");
                        let child_has_content = 
                            child_elem.select(pre_selector).next().is_some() ||
                            child_elem.select(ul_selector).next().is_some() ||
                            child_elem.select(ol_selector).next().is_some();
                        
                        if !is_content_element && !child_has_content {
                            // Not a content element → safe to remove
                            to_remove.insert(child_elem.id());
                        }
                    }
                }
            } else {
                // No content elements found → remove entire element
                to_remove.insert(elem.id());
            }
        }
    }

    // Serialize HTML with both removal and unwrapping - O(n)
    let mut result = String::new();
    serialize_html_with_unwrap(element, &to_remove, &to_unwrap, &mut result);
    result
}

/// Recursively serialize an element and its descendants to HTML,
/// skipping elements in the removal set and unwrapping elements in the unwrap set.
///
/// This is adapted from `main_content_extraction.rs::serialize_html_excluding()`.
/// Preserves HTML structure (tags, attributes, nesting) while:
/// - Excluding elements in `to_remove` (skip entirely)
/// - Unwrapping elements in `to_unwrap` (serialize children but not tags)
fn serialize_html_with_unwrap(
    element: &ElementRef,
    to_remove: &HashSet<NodeId>,
    to_unwrap: &HashSet<NodeId>,
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

                    let elem_name = child_elem.value().name();
                    let should_unwrap = to_unwrap.contains(&child_elem.id());

                    // Emit opening tag (unless unwrapping)
                    if !should_unwrap {
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
                    }

                    // Check if this is a void element (self-closing)
                    const VOID_ELEMENTS: &[&str] = &[
                        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
                        "param", "source", "track", "wbr",
                    ];

                    if !should_unwrap && VOID_ELEMENTS.contains(&elem_name) {
                        // Void element - no closing tag needed
                        continue;
                    }

                    // Recursively serialize children
                    serialize_html_with_unwrap(&child_elem, to_remove, to_unwrap, output);

                    // Closing tag (only for non-void elements, and not when unwrapping)
                    if !should_unwrap {
                        output.push_str("</");
                        output.push_str(elem_name);
                        output.push('>');
                    }
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
        
        // Try Strategy 1: Extract from .ec-line elements (preserves newlines!)
        // This is the primary strategy because it preserves the actual code structure
        let lines: Vec<String> = match node.select(".ec-line") {
            Ok(iter) => iter.map(|line_elem| line_elem.text_contents()).collect(),
            Err(_) => Vec::new(),
        };

        let code_text = if !lines.is_empty() {
            log::debug!("Extracted {} lines from .ec-line elements", lines.len());
            lines.join("\n")
        } else {
            // Strategy 2: Fallback to button[data-code] attribute
            // Note: data-code often has flattened code with spaces instead of newlines,
            // but it's better than nothing when .ec-line elements aren't available
            log::debug!("Falling back to data-code attribute extraction");

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
                .unwrap_or_else(|| {
                    log::warn!("Failed to extract code from Expressive Code block");
                    String::new()
                })
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

/// Pre-escape angle brackets in code blocks to protect from HTML parser
///
/// This must run BEFORE any DOM parsing to prevent angle brackets being
/// interpreted as HTML tags (e.g., `<T>` in `Result<T>`).
///
/// Targets:
/// - `<pre>...</pre>` blocks
/// - `<code>...</code>` blocks (both inline and block)
///
/// # Arguments
/// * `html` - Raw HTML string
///
/// # Returns
/// * HTML with code block contents escaped
fn escape_code_blocks(html: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;
    
    // Match <pre>...</pre> blocks (including nested <code>)
    static PRE_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)<pre(?:\s+[^>]*)?>(.+?)</pre>")
            .expect("PRE_BLOCK_RE: hardcoded regex is valid")
    });
    
    // Match standalone <code>...</code> blocks (not inside <pre>)
    static CODE_INLINE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"<code(?:\s+[^>]*)?>(.+?)</code>")
            .expect("CODE_INLINE_RE: hardcoded regex is valid")
    });
    
    // First pass: escape <pre> blocks (these often contain <code> internally)
    let result = PRE_BLOCK_RE.replace_all(html, |caps: &regex::Captures| {
        let pre_content = &caps[1];
        let escaped = pre_content
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        
        // Preserve the <pre> tag structure (extract attributes if present)
        if let Some(close_pos) = caps[0].find('>') {
            let pre_opening = &caps[0][..=close_pos];
            format!("{}{}</pre>", pre_opening, escaped)
        } else {
            // Fallback - should never happen with valid regex match
            format!("<pre>{}</pre>", escaped)
        }
    });
    
    // Second pass: escape standalone <code> blocks not inside <pre>
    // This handles inline code like `<code>foo</code>`
    let result = CODE_INLINE_RE.replace_all(&result, |caps: &regex::Captures| {
        let code_content = &caps[1];
        let escaped = code_content
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        
        // Preserve the <code> tag structure
        if let Some(close_pos) = caps[0].find('>') {
            let code_opening = &caps[0][..=close_pos];
            format!("{}{}</code>", code_opening, escaped)
        } else {
            // Fallback - should never happen with valid regex match
            format!("<code>{}</code>", escaped)
        }
    });
    
    result.into_owned()
}

/// Normalize problematic HTML structures before conversion
///
/// Handles edge cases that can cause htmd to fail:
/// - Nested code blocks
/// - Malformed tag nesting  
/// - Empty elements
///
/// This is Layer 3 (PREVENTIVE) of the defense-in-depth strategy against HTML leakage.
///
/// # Arguments
/// * `html` - HTML string to normalize
///
/// # Returns
/// * `Ok(String)` - Normalized HTML string
///
/// # Note
/// Current implementation is a placeholder that passes through HTML unchanged.
/// Future implementation will manipulate the DOM to remove problematic structures.
pub fn normalize_html_structure(html: &str) -> Result<String> {
    // TODO: Implement element removal/normalization
    // This requires manipulating the scraper DOM and re-serializing
    // Planned features:
    // - Remove empty code/pre elements (they can confuse htmd)
    // - Remove nested code blocks (invalid HTML that htmd can't handle)
    // - Flatten malformed tag nesting
    
    // For now, pass through unchanged
    // The primary fix (handlers.walk_children) handles the main issue
    Ok(html.to_string())
}



/// Clean HTML content by removing scripts, styles, ads, tracking, and other non-content elements
///
/// This function performs aggressive cleaning including:
/// - Removing `<script>` and `<style>` tags and their contents
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
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::html_preprocessing::clean_html_content;
/// let html = r#"<div><script>alert('xss')</script><p>Hello &amp; goodbye</p></div>"#;
/// let clean = clean_html_content(html)?;
/// assert!(clean.contains("<p>Hello"));
/// assert!(!clean.contains("<script>"));
/// # Ok::<(), anyhow::Error>(())
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
    // STEP 0: Preprocess Expressive Code blocks FIRST (uses kuchiki DOM parsing)
    // ============================================================================
    // CRITICAL: This MUST run BEFORE escape_code_blocks!
    // Expressive Code blocks contain <div class="ec-line"> elements inside <pre><code>.
    // If escape_code_blocks runs first, it escapes those divs into &lt;div&gt; text,
    // which destroys the DOM structure needed to find .ec-line elements.
    // 
    // Order: preprocess_expressive_code -> escape_code_blocks
    // This extracts code from .ec-line elements (preserving newlines), THEN
    // escape_code_blocks protects the resulting plain text code.
    let html = preprocess_expressive_code(html)?;

    // ============================================================================
    // STEP 1: Pre-escape code blocks (protect angle brackets)
    // ============================================================================
    // Now that Expressive Code is converted to plain <pre><code>, we can safely
    // escape angle brackets to protect generic type parameters like Result<T>
    // from being interpreted as HTML tags by subsequent DOM parsers.
    let html = escape_code_blocks(&html);

    // ============================================================================
    // STEP 1.5: Merge fragmented code blocks (uses DOM parsing)
    // REMOVED: preprocess_code_blocks() - kuchiki serialization stripped inline formatting tags

    // ============================================================================
    // STEP 2: Regex-based cleaning (existing code)
    // ============================================================================
    // Use Cow to avoid unnecessary allocations
    let result = Cow::Borrowed(html.as_str());

    // NOTE: Script and style elements are now handled by htmd's script_style_handler
    // in element_handler/mod.rs, which discards them entirely during conversion.

    // NOTE: Forms and iframes are now handled by htmd element handlers
    // (see element_handler/mod.rs form/iframe skip handler).
    // Social media widgets, cookie notices, and ads are now filtered
    // by htmd element handlers (div.rs, section.rs, aside.rs) instead of regex.
    // See is_widget_element() in element_util.rs for the filtering logic.

    // Remove hidden elements (multiple patterns for comprehensive matching)
    let result = HIDDEN_DISPLAY.replace_all(&result, "");
    let result = HIDDEN_ATTR.replace_all(&result, "");
    let result = HIDDEN_VISIBILITY.replace_all(&result, "");

    // Remove empty code/pre elements (prevents orphaned fence markers in markdown)
    // These are often styling placeholders with no semantic content
    let result = EMPTY_PRE_CODE.replace_all(&result, "");
    let result = EMPTY_PRE.replace_all(&result, "");
    let result = EMPTY_CODE.replace_all(&result, "");

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

    // ========================================================================
    // STAGE 2: DOM-based removal (handles complex selectors)
    // ========================================================================
    
    // Remove interactive elements (buttons, inputs, data attributes, etc.)
    let result: Cow<str> = Cow::Owned(remove_interactive_elements_from_dom(&result));

    // REMOVED: fix_bold_spanning_code_blocks() - kuchiki serialization stripped inline formatting tags

    // ========================================================================
    // FINAL STEP: Decode HTML entities
    // ========================================================================
    
    // Decode HTML entities (&amp; → &, &lt; → <, &#124; → |, &#x7C; → |, etc.)
    // Using htmlentity for comprehensive support of all HTML5 entities
    let decoded = decode(result.as_bytes()).to_string()?;

    // Return decoded string
    Ok(decoded)
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
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::html_preprocessing::html_cleaning::normalize_image_tags;
/// let html = r#"<img alt="demo" decoding="async" src="/image.jpg" width="800">"#;
/// let result = normalize_image_tags(html);
/// assert_eq!(result, r#"<img src="/image.jpg" alt="demo">"#);
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
        // Test that event handlers are removed (via regex cleaning)
        // Note: Buttons are also removed entirely by DOM-based interactive element removal
        let html = r#"<div onclick="alert('click')">Click me</div>"#;
        let result = clean_html_content(html)?;
        assert!(!result.contains("onclick"), "Event handler should be removed");
        assert!(result.contains("Click me"), "Content should remain when element is not interactive");
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

    #[test]
    fn test_escapes_angle_brackets_in_pre_blocks() {
        let html = r#"<pre><code>Result<T></code></pre>"#;
        let result = escape_code_blocks(html);
        // Should escape angle brackets to HTML entities
        assert!(result.contains("&lt;T&gt;"));
        assert!(!result.contains("<T>"));
    }

    #[test]
    fn test_escapes_nested_generics() {
        let html = r#"<pre>Result<Option<Vec<T>>></pre>"#;
        let result = escape_code_blocks(html);
        // Should escape all angle brackets
        assert!(result.contains("Result&lt;Option&lt;Vec&lt;T&gt;&gt;&gt;"));
    }

    #[test]
    fn test_escapes_inline_code_blocks() {
        let html = r#"<p>Use <code>HashMap<K, V></code> for maps</p>"#;
        let result = escape_code_blocks(html);
        // Should escape angle brackets in inline code
        assert!(result.contains("HashMap&lt;K, V&gt;"));
    }

    #[test]
    fn test_preserves_pre_attributes() {
        let html = r#"<pre class="language-rust" data-lang="rust"><code>Result<T></code></pre>"#;
        let result = escape_code_blocks(html);
        // Should preserve attributes
        assert!(result.contains(r#"class="language-rust""#));
        assert!(result.contains(r#"data-lang="rust""#));
        // Should escape content
        assert!(result.contains("&lt;T&gt;"));
    }

    #[test]
    fn test_preserves_code_attributes() {
        let html = r#"<code class="inline">Vec<String></code>"#;
        let result = escape_code_blocks(html);
        // Should preserve attributes
        assert!(result.contains(r#"class="inline""#));
        // Should escape content
        assert!(result.contains("Vec&lt;String&gt;"));
    }

    #[test]
    fn test_generic_types_preserved_through_full_pipeline() -> Result<()> {
        let html = r#"<pre><code>pub fn init_tui() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {</code></pre>"#;
        let result = clean_html_content(html)?;
        // After full pipeline, angle brackets should be preserved (escaped then unescaped)
        assert!(result.contains("Result<Terminal<CrosstermBackend<Stdout>>>"));
        Ok(())
    }

    #[test]
    fn test_decode_numeric_entities_decimal() -> Result<()> {
        let html = r#"<p>Use pipe &#124; for commands</p>"#;
        let result = clean_html_content(html)?;
        
        // Should decode &#124; to |
        assert!(
            result.contains("Use pipe | for commands"),
            "Should decode decimal numeric entity &#124;. Got: {}",
            result
        );
        Ok(())
    }

    #[test]
    fn test_decode_numeric_entities_hex() -> Result<()> {
        let html = r#"<p>Hex pipe: &#x7C; and lt: &#x3C;</p>"#;
        let result = clean_html_content(html)?;
        
        // Should decode hex entities
        assert!(
            result.contains("Hex pipe: | and lt: <"),
            "Should decode hexadecimal entities. Got: {}",
            result
        );
        Ok(())
    }

    #[test]
    fn test_decode_named_entities_extended() -> Result<()> {
        let html = r#"<p>Em dash&#8212;and copyright&#169;</p>"#;
        let result = clean_html_content(html)?;
        
        // Should decode extended named entities
        assert!(
            result.contains("Em dash—and copyright©"),
            "Should decode extended named entities. Got: {}",
            result
        );
        Ok(())
    }

    #[test]
    fn test_decode_entities_in_table_code() -> Result<()> {
        // This is the actual reported issue
        let html = r#"
            <table>
                <tr>
                    <td><code>cat file &#124; claude -p "query"</code></td>
                    <td>Process piped content</td>
                </tr>
            </table>
        "#;
        let result = clean_html_content(html)?;
        
        // Should decode &#124; to | even inside code and tables
        assert!(
            result.contains("cat file | claude"),
            "Should decode entities in table cells with code. Got: {}",
            result
        );
        Ok(())
    }

    #[test]
    fn test_decode_all_common_entities() -> Result<()> {
        let html = r#"<p>&lt; &gt; &amp; &quot; &#124; &#x27; &nbsp;</p>"#;
        let result = clean_html_content(html)?;
        
        // Should decode all types of entities
        assert!(
            result.contains("< > & \" | '"),
            "Should decode all common entity types. Got: {}",
            result
        );
        // Note: &nbsp; becomes a non-breaking space character (U+00A0)
        Ok(())
    }

    #[test]
    fn test_preserves_code_blocks_with_copy_buttons() -> Result<()> {
        // Pattern 1: Button as sibling within a wrapper div
        let html = r#"
            <div class="code-block-wrapper">
                <pre data-language="bash"><code>brew install claude-code</code></pre>
                <button class="copy-button">Copy</button>
            </div>
        "#;
        
        let result = clean_html_content(html)?;
        
        // Code block must be preserved
        assert!(
            result.contains("<pre") || result.contains("brew install claude-code"),
            "Pre tag or code content was removed! Got: {}",
            result
        );
        assert!(
            result.contains("brew install claude-code"),
            "Code content 'brew install claude-code' was removed! Got: {}",
            result
        );
        
        // Button should be removed
        assert!(
            !result.contains("<button"),
            "Button was not removed! Got: {}",
            result
        );
        
        Ok(())
    }

    #[test]
    fn test_preserves_code_blocks_in_copy_to_clipboard_div() -> Result<()> {
        // Pattern 2: Wrapper with copy-to-clipboard class
        let html = r#"
            <div class="copy-to-clipboard">
                <pre><code class="language-rust">fn main() {
    println!("Hello");
}</code></pre>
                <button>Copy</button>
            </div>
        "#;
        
        let result = clean_html_content(html)?;
        
        // Code block must be preserved
        assert!(
            result.contains("fn main()"),
            "Rust code 'fn main()' was removed! Got: {}",
            result
        );
        assert!(
            result.contains("println!"),
            "Code content 'println!' was removed! Got: {}",
            result
        );
        
        // Button should be removed
        assert!(
            !result.contains("<button"),
            "Button was not removed! Got: {}",
            result
        );
        
        Ok(())
    }

    #[test]
    fn test_preprocess_expressive_code_newlines() -> Result<()> {
        // Test fixture HTML with ec-line elements
        let html = r#"<div class="expressive-code">
<pre data-language="rust"><code><div class="ec-line"><div class="code">pub fn main() -&gt; io::Result&lt;()&gt; {</div></div>
<div class="ec-line"><div class="code">    init_panic_hook();</div></div>
<div class="ec-line"><div class="code">    let mut tui = init_tui()?;</div></div>
<div class="ec-line"><div class="code">    panic!("This is a panic!");</div></div>
<div class="ec-line"><div class="code">}</div></div></code></pre>
<div class="copy"><button data-code="pub fn main() -&gt; io::Result&lt;()&gt; {    init_panic_hook();    let mut tui = init_tui()?;    panic!(&quot;This is a panic!&quot;);}"></button></div>
</div>"#;

        println!("=== INPUT HTML ===");
        println!("{}", html);
        
        let result = preprocess_expressive_code(html)?;
        
        println!("\n=== AFTER preprocess_expressive_code ===");
        println!("{}", result);
        
        // Should have replaced the expressive-code div with a clean pre/code block
        assert!(result.contains("<pre><code"), "Should have pre/code element");
        
        // The code should have newlines between lines, not spaces
        // Each ec-line element should become a separate line
        assert!(
            result.contains("{\n    init_panic_hook();"),
            "Code should have newlines preserved from ec-line extraction. Got: {:?}",
            result
        );
        
        Ok(())
    }

    #[test]
    fn test_full_clean_html_content_preserves_newlines() -> Result<()> {
        // Test full clean_html_content function
        let html = r#"<div class="expressive-code">
<pre data-language="rust"><code><div class="ec-line"><div class="code">pub fn main() -&gt; io::Result&lt;()&gt; {</div></div>
<div class="ec-line"><div class="code">    init_panic_hook();</div></div>
<div class="ec-line"><div class="code">    let mut tui = init_tui()?;</div></div>
<div class="ec-line"><div class="code">    panic!("This is a panic!");</div></div>
<div class="ec-line"><div class="code">}</div></div></code></pre>
<div class="copy"><button data-code="pub fn main() -&gt; io::Result&lt;()&gt; {    init_panic_hook();    let mut tui = init_tui()?;    panic!(&quot;This is a panic!&quot;);}"></button></div>
</div>"#;

        println!("=== INPUT HTML ===");
        println!("{}", html);
        
        let result = clean_html_content(html)?;
        
        println!("\n=== AFTER clean_html_content ===");
        println!("{}", result);
        
        // The code should still have newlines after full cleaning
        assert!(
            result.contains("{\n    init_panic_hook();"),
            "Code should have newlines preserved after clean_html_content. Got: {:?}",
            result
        );
        
        Ok(())
    }

    #[test]
    fn test_dom_parsing_preserves_pre_code_newlines() -> Result<()> {
        // Test if scraper's DOM parsing preserves newlines in pre/code
        let html = r#"<pre><code class="language-rust">pub fn main() {
    init_panic_hook();
    test();
}</code></pre>"#;

        println!("=== INPUT HTML ===");
        println!("{:?}", html);
        
        let result = remove_interactive_elements_from_dom(html);
        
        println!("\n=== AFTER remove_interactive_elements_from_dom ===");
        println!("{:?}", result);
        
        // Newlines should be preserved inside pre/code
        assert!(
            result.contains("{\n    init_panic_hook();"),
            "Pre/code newlines should be preserved by DOM parsing. Got: {:?}",
            result
        );
        
        Ok(())
    }

    #[test]
    fn test_preprocess_then_dom_preserves_newlines() -> Result<()> {
        // Test the exact flow: preprocess_expressive_code -> remove_interactive_elements_from_dom
        let html = r#"<div class="expressive-code">
<pre data-language="rust"><code><div class="ec-line"><div class="code">pub fn main() {</div></div>
<div class="ec-line"><div class="code">    init();</div></div>
<div class="ec-line"><div class="code">}</div></div></code></pre>
</div>"#;

        println!("=== INPUT HTML ===");
        println!("{}", html);
        
        let after_preprocess = preprocess_expressive_code(html)?;
        println!("\n=== AFTER preprocess_expressive_code ===");
        println!("{}", after_preprocess);
        println!("Contains newline after brace: {}", after_preprocess.contains("{\n    init();"));
        
        let after_dom = remove_interactive_elements_from_dom(&after_preprocess);
        println!("\n=== AFTER remove_interactive_elements_from_dom ===");
        println!("{}", after_dom);
        println!("Contains newline after brace: {}", after_dom.contains("{\n    init();"));
        
        // Newlines should still be preserved
        assert!(
            after_dom.contains("{\n    init();"),
            "Newlines should be preserved through both steps. Got: {:?}",
            after_dom
        );
        
        Ok(())
    }

    #[test]
    fn test_correct_order_preprocess_then_escape() -> Result<()> {
        // Test the CORRECT order: preprocess_expressive_code FIRST, then escape_code_blocks
        // This is how clean_html_content now works after the fix.
        let html = r#"<div class="expressive-code">
<pre data-language="rust"><code><div class="ec-line"><div class="code">pub fn main() -&gt; io::Result&lt;()&gt; {</div></div>
<div class="ec-line"><div class="code">    init();</div></div>
<div class="ec-line"><div class="code">}</div></div></code></pre>
</div>"#;

        println!("=== INPUT HTML ===");
        println!("{}", html);
        
        // CORRECT ORDER: preprocess first to extract .ec-line content
        let after_preprocess = preprocess_expressive_code(html)?;
        println!("\n=== AFTER preprocess_expressive_code ===");
        println!("{}", after_preprocess);
        println!("Contains newline: {}", after_preprocess.contains("{\n    init();"));
        
        // Now escape_code_blocks can safely protect angle brackets
        let after_escape = escape_code_blocks(&after_preprocess);
        println!("\n=== AFTER escape_code_blocks ===");
        println!("{}", after_escape);
        
        // Newlines should be preserved through the correct order
        assert!(
            after_preprocess.contains("{\n    init();"),
            "Correct order should preserve newlines. Got: {:?}",
            after_preprocess
        );
        
        Ok(())
    }
}
