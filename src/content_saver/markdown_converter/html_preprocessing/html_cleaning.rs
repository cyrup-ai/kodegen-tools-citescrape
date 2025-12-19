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
    Regex::new(r"(?s)<[^>]+\shidden(?:\s|>)[^>]*>.*?</[^>]+>")
        .expect("HIDDEN_ATTR: hardcoded regex is valid")
});

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

/// Matches accessibility navigation anchor links with "Navigate to header" text
/// These appear in various documentation sites as screen-reader navigation aids
/// 
/// Pattern: `(?s)<a[^>]*>\s*Navigate\s+to\s+header\s*</a>`
/// 
/// Matches:
/// - `<a href="#section">Navigate to header</a>`
/// - `<a href="#heading"> Navigate to header </a>` (leading/trailing spaces)
/// - `<a href="#target">\n  Navigate\n  to\n  header\n</a>` (newlines and indentation)
/// - `<a href="#overview">Navigate  to  header</a>` (multiple spaces between words)
/// 
/// Does NOT match:
/// - `<a href="#section">Navigate header</a>` (missing "to")
/// - `<a href="#section">Navigate to</a>` (incomplete text)
/// - `<a href="#section">Please navigate to header</a>` (extra prefix text)
/// 
/// Whitespace handling:
/// - `\s*` at start/end: Matches zero or more whitespace (spaces, newlines, tabs)
/// - `\s+` between words: Matches one or more whitespace characters
/// - `(?s)` flag: Enables dot-matches-newline mode (though not using dot here)
static HEADING_NAVIGATE_TO_HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a[^>]*>\s*Navigate\s+to\s+header\s*</a>"#)
        .expect("HEADING_NAVIGATE_TO_HEADER_RE: hardcoded regex is valid")
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

/// Extract language from class attribute (e.g., "language-rust", "lang-python")
fn extract_language_from_class_attr(class: &str) -> Option<String> {
    for token in class.split_whitespace() {
        if let Some(lang) = token.strip_prefix("language-") {
            return Some(lang.to_string());
        }
        if let Some(lang) = token.strip_prefix("lang-") {
            return Some(lang.to_string());
        }
    }
    None
}

/// Extract language from element attributes
/// 
/// Checks <pre> element attributes and child <code> element attributes
/// for language hints (class="language-X", data-language="X")
fn extract_language_from_element(node: &kuchiki::NodeRef) -> Option<String> {
    use kuchiki::NodeData;
    
    // Check if this is an element node
    if let NodeData::Element(elem_data) = node.data() {
        let attrs = elem_data.attributes.borrow();
        
        // Check class attribute
        if let Some(class) = attrs.get("class")
            && let Some(lang) = extract_language_from_class_attr(class)
        {
            return Some(lang);
        }
        
        // Check data-language attribute
        if let Some(lang) = attrs.get("data-language") {
            return Some(lang.to_string());
        }
    }
    
    // Check child <code> elements
    if let Ok(code_iter) = node.select("code") {
        for code_elem in code_iter {
            let attrs = code_elem.attributes.borrow();
            if let Some(class) = attrs.get("class")
                && let Some(lang) = extract_language_from_class_attr(class)
            {
                return Some(lang);
            }
        }
    }
    
    None
}

/// Check if two nodes are adjacent code elements that should be merged
///
/// Returns true if node2 is the next non-whitespace sibling of node1.
/// Whitespace-only text nodes between elements are allowed and ignored.
fn is_adjacent_code_element(node1: &kuchiki::NodeRef, node2: &kuchiki::NodeRef) -> bool {
    use kuchiki::NodeData;
    
    // Get next sibling of node1
    let mut current = node1.next_sibling();
    
    while let Some(sibling) = current {
        match sibling.data() {
            NodeData::Element(_) => {
                // Found an element - is it node2?
                // Compare by pointer equality (sibling is already a NodeRef)
                return std::ptr::eq(&sibling as *const _, node2 as *const _);
            }
            NodeData::Text(text) => {
                // Allow whitespace-only text nodes between code blocks
                if !text.borrow().trim().is_empty() {
                    // Non-whitespace content - not adjacent
                    return false;
                }
                // Continue checking
                current = sibling.next_sibling();
            }
            _ => {
                // Comments, processing instructions - continue
                current = sibling.next_sibling();
            }
        }
    }
    
    false
}

/// Merge a group of adjacent <pre> elements into one
///
/// Extracts language from first element and combines all text content
/// with newlines between fragments.
fn merge_code_elements(group: &[kuchiki::NodeRef]) -> Result<kuchiki::NodeRef> {
    if group.is_empty() {
        return Err(anyhow::anyhow!("Cannot merge empty group"));
    }
    
    // Extract language from first element (most reliable)
    let language = extract_language_from_element(&group[0]);
    
    // Collect text content from all elements
    let mut merged_content = String::new();
    
    for node in group {
        let text = node.text_contents();
        if !merged_content.is_empty() && !text.starts_with('\n') {
            merged_content.push('\n'); // Ensure line break between fragments
        }
        merged_content.push_str(&text);
    }
    
    // HTML-escape the code text
    let escaped_code = html_escape::encode_text(&merged_content).to_string();
    
    // Create new merged element
    let new_html = if let Some(lang) = language {
        format!(
            "<pre><code class=\"language-{}\">{}</code></pre>",
            lang,
            escaped_code
        )
    } else {
        format!(
            "<pre><code>{}</code></pre>",
            escaped_code
        )
    };
    
    let new_node = kuchiki::parse_html().one(new_html);
    Ok(new_node)
}

/// Preprocess code blocks by merging adjacent <pre> elements that should be one block
///
/// This fixes syntax highlighter output that splits single code blocks across
/// multiple DOM elements, causing incorrect fence boundaries in markdown.
///
/// # Strategy
///
/// 1. Parse HTML to DOM using kuchiki
/// 2. Identify adjacent <pre> elements
/// 3. Merge if they represent fragments of the same code block:
///    - No intervening non-whitespace content
/// 4. Consolidate into single <pre><code> element
/// 5. Serialize back to HTML for htmd processing
///
/// # Arguments
///
/// * `html` - HTML string potentially containing fragmented code blocks
///
/// # Returns
///
/// * `Ok(String)` - HTML with merged code blocks
/// * `Err(anyhow::Error)` - If HTML parsing fails
fn preprocess_code_blocks(html: &str) -> Result<String> {
    // Parse HTML into mutable DOM
    let document = kuchiki::parse_html().one(html.to_string());
    
    // Select all <pre> elements
    let selector = "pre";
    let matches: Vec<_> = document
        .select(selector)
        .map_err(|()| anyhow::anyhow!("Invalid CSS selector: {}", selector))?
        .collect();
    
    if matches.is_empty() {
        return Ok(html.to_string());
    }
    
    // Group adjacent <pre> elements
    let mut merge_groups: Vec<Vec<kuchiki::NodeRef>> = Vec::new();
    let mut current_group: Vec<kuchiki::NodeRef> = Vec::new();
    
    for pre_elem in matches.iter() {
        let node = pre_elem.as_node().clone();
        
        if current_group.is_empty() {
            current_group.push(node);
            continue;
        }
        
        // Check if this element is adjacent to previous
        if let Some(prev_node) = current_group.last() {
            if is_adjacent_code_element(prev_node, &node) {
                // Same group
                current_group.push(node);
            } else {
                // New group - save current and start fresh
                if current_group.len() > 1 {
                    merge_groups.push(current_group.clone());
                }
                current_group = vec![node];
            }
        }
    }
    
    // Don't forget the last group
    if current_group.len() > 1 {
        merge_groups.push(current_group);
    }
    
    // For each merge group, create merged element and replace
    for group in merge_groups {
        if group.len() < 2 {
            continue; // Nothing to merge
        }
        
        let merged_node = merge_code_elements(&group)?;
        
        // Insert merged node before first element
        if let Some(first_child) = merged_node.first_child() {
            group[0].insert_before(first_child);
        }
        
        // Remove all original elements in group
        for node in group {
            node.detach();
        }
    }
    
    // Serialize back to HTML
    let mut output = Vec::new();
    document
        .serialize(&mut output)
        .context("Failed to serialize HTML after code block merging")?;
    
    String::from_utf8(output)
        .context("Failed to convert HTML bytes to UTF-8 after code block merging")
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

/// Helper: Check if a node is a code element (<pre> or <code>)
fn is_code_element(node: &kuchiki::NodeRef) -> bool {
    use kuchiki::NodeData;
    
    if let NodeData::Element(element) = node.data() {
        let tag_name = &*element.name.local;
        tag_name == "pre" || tag_name == "code"
    } else {
        false
    }
}

/// Helper: Check if nodes contain non-whitespace content
fn has_non_whitespace_content(nodes: &[kuchiki::NodeRef]) -> bool {
    use kuchiki::NodeData;
    
    for node in nodes {
        match node.data() {
            NodeData::Text(text) => {
                if !text.borrow().trim().is_empty() {
                    return true;
                }
            }
            NodeData::Element(_) => {
                return true; // Element nodes count as content
            }
            _ => {}
        }
    }
    false
}

/// Helper: Create a new element node by parsing HTML
fn create_element(tag_name: &str) -> kuchiki::NodeRef {
    // Create element by parsing minimal HTML
    // This avoids needing to import markup5ever directly
    let html = format!("<{0}></{0}>", tag_name);
    let doc = kuchiki::parse_html().one(html);
    
    // Extract the first child element from the parsed document
    if let Some(child) = doc.first_child() {
        child
    } else {
        // Fallback: return the document itself (should never happen)
        doc
    }
}

/// Fix bold/strong tags that span code blocks by restructuring the HTML
///
/// This prevents htmd from emitting `**` markers around code fences, which
/// causes trailing asterisks after closing fences.
///
/// # Transformation
///
/// Before:
/// ```html
/// <strong>Text <pre><code>code</code></pre> more text</strong>
/// ```
///
/// After:
/// ```html
/// <strong>Text </strong><pre><code>code</code></pre><strong> more text</strong>
/// ```
///
/// This ensures htmd emits clean markdown without bold spanning code blocks.
///
/// # Example
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::html_preprocessing::html_cleaning::fix_bold_spanning_code_blocks;
/// let html = r#"<strong>Text<code>code</code>more text</strong>"#;
/// let fixed = fix_bold_spanning_code_blocks(html)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn fix_bold_spanning_code_blocks(html: &str) -> Result<String> {
    use kuchiki::traits::*;
    
    // Parse HTML to mutable DOM
    let document = kuchiki::parse_html().one(html.to_string());
    
    // Find all <strong> and <b> elements
    let bold_selectors = vec!["strong", "b"];
    
    for selector_str in &bold_selectors {
        // Must collect before iteration because we'll detach/restructure nodes
        let matches: Vec<_> = match document.select(selector_str) {
            Ok(iter) => iter.collect(),
            Err(_) => {
                // This should never fail for simple selectors, but handle gracefully
                log::warn!("Failed to parse selector '{}', skipping", selector_str);
                continue;
            }
        };
        
        for bold_elem in matches {
            let node = bold_elem.as_node();
            
            // Check if this bold element contains <pre> or <code> descendants
            let has_code_descendant = node
                .select("pre, code")
                .map(|mut iter| iter.next().is_some())
                .unwrap_or(false);
            
            if !has_code_descendant {
                continue; // This bold element is fine
            }
            
            // Strategy: Split the bold element into segments:
            // 1. Text before code block (wrapped in bold)
            // 2. Code block (NOT wrapped in bold)
            // 3. Text after code block (wrapped in bold)
            
            let tag_name = bold_elem.name.local.to_string();
            let mut before_code = Vec::new();
            let mut code_blocks = Vec::new();
            let mut current_after: Option<Vec<kuchiki::NodeRef>> = None;
            
            // Traverse children and categorize them
            for child in node.children() {
                if is_code_element(&child) {
                    // This is a code block - add it to code_blocks
                    code_blocks.push(child.clone());
                    // Start collecting "after" content
                    current_after = Some(Vec::new());
                } else if let Some(ref mut after) = current_after {
                    // We're after a code block
                    after.push(child.clone());
                } else {
                    // We're before any code blocks
                    before_code.push(child.clone());
                }
            }
            
            // Build replacement structure
            let mut replacements = Vec::new();
            
            // Add "before" content wrapped in bold (if non-empty)
            if !before_code.is_empty() && has_non_whitespace_content(&before_code) {
                let before_bold = create_element(&tag_name);
                for child in before_code {
                    before_bold.append(child);
                }
                replacements.push(before_bold);
            }
            
            // Add code blocks unwrapped
            for code_block in code_blocks {
                replacements.push(code_block);
            }
            
            // Add "after" content wrapped in bold (if non-empty)
            if let Some(after_nodes) = current_after
                && !after_nodes.is_empty() && has_non_whitespace_content(&after_nodes) {
                let after_bold = create_element(&tag_name);
                for child in after_nodes {
                    after_bold.append(child);
                }
                replacements.push(after_bold);
            }
            
            // Insert replacements before original, then detach original
            for replacement in replacements {
                node.insert_before(replacement);
            }
            node.detach();
        }
    }
    
    // Serialize back to HTML
    let mut output = Vec::new();
    document
        .serialize(&mut output)
        .context("Failed to serialize HTML after fixing bold spanning code blocks")?;
    
    String::from_utf8(output)
        .context("Failed to convert HTML bytes to UTF-8 after fixing bold tags")
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

// ============================================================================
// Syntax Highlighting Span Removal
// ============================================================================

/// Regex to match <pre> blocks
static PRE_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<pre([^>]*)>(.*?)</pre>")
        .expect("PRE_BLOCK_RE: hardcoded regex is valid")
});

/// Regex to match <code> tags with attributes
static CODE_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<code([^>]*)>(.*?)</code>")
        .expect("CODE_TAG_RE: hardcoded regex is valid")
});

/// Strip syntax highlighting span tags from code blocks
///
/// Removes `<span>` tags with inline styles (syntax highlighting) from `<pre>` and `<code>` blocks.
/// This preprocessing step must run BEFORE code block protection to ensure clean code text.
///
/// # Patterns Handled
///
/// 1. **Style attribute spans**: `<span style="--0:#COLOR;--1:#COLOR">text</span>` → `text`
/// 2. **Indent class spans**: `<span class="indent">    </span>` → `    `
/// 3. **Nested spans**: Recursively extracts text from all levels
/// 4. **HTML entities**: Properly decodes entities in extracted text
///
/// # Arguments
///
/// * `html` - Raw HTML string potentially containing styled code blocks
///
/// # Returns
///
/// * `Ok(String)` - HTML with code blocks cleaned of span tags
/// * `Err(anyhow::Error)` - If HTML parsing fails
///
/// # Example
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::html_preprocessing::html_cleaning::strip_syntax_highlighting_spans;
/// let html = r#"<pre><code><span style="--0:#C792EA">pub</span> <span style="--0:#C792EA">fn</span> main()</code></pre>"#;
/// let clean = strip_syntax_highlighting_spans(html)?;
/// assert!(clean.contains("pub fn main()"));
/// assert!(!clean.contains("<span"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn strip_syntax_highlighting_spans(html: &str) -> Result<String> {
    // Fast path: skip if no span tags in code blocks
    if !html.contains("<pre") || !html.contains("<span") {
        return Ok(html.to_string());
    }
    
    let result = PRE_BLOCK_RE.replace_all(html, |caps: &regex::Captures| {
        let pre_attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let inner_html = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        
        // Check if this pre block contains span tags
        if !inner_html.contains("<span") {
            // No spans - return unchanged
            return caps[0].to_string();
        }
        
        // STEP 1: Extract language attribute from <code> tag BEFORE cleaning
        // This must happen before extract_text_from_spans destroys the HTML structure
        let code_attrs = extract_code_lang_attribute(inner_html);
        
        // STEP 2: Extract clean text by stripping all HTML tags (spans, code, etc.)
        let clean_text = extract_text_from_spans(inner_html);
        
        // STEP 3: HTML-escape the clean text to prevent interpretation as HTML
        let escaped_text = html_escape::encode_text(&clean_text).to_string();
        
        // STEP 4: Reconstruct clean pre block with preserved language attribute
        if !code_attrs.is_empty() {
            format!("<pre{}><code{}>{}</code></pre>", pre_attrs, code_attrs, escaped_text)
        } else {
            format!("<pre{}><code>{}</code></pre>", pre_attrs, escaped_text)
        }
    });
    
    Ok(result.to_string())
}

/// Extract text content from HTML with spans, stripping all span tags
///
/// Uses scraper to parse the HTML fragment and extract only text nodes,
/// automatically handling nested spans and HTML entity decoding.
///
/// # Arguments
/// * `html` - HTML fragment containing span tags
///
/// # Returns
/// * Clean text with all span tags removed and entities decoded
fn extract_text_from_spans(html: &str) -> String {
    use scraper::node::Node;
    
    // Parse fragment
    let fragment = Html::parse_fragment(html);
    let mut result = String::new();
    
    // Recursively extract text from all nodes
    for node in fragment.root_element().descendants() {
        if let Node::Text(text) = node.value() {
            result.push_str(text);
        }
    }
    
    result
}

/// Extract language attribute from <code> tag while preserving original HTML structure
/// 
/// This function extracts ONLY the attributes from the <code> tag without modifying
/// the HTML structure. It looks for patterns like:
/// - `class="language-rust"`
/// - `class="lang-python"`
/// - `data-lang="javascript"`
///
/// # Arguments
/// * `html` - Original HTML fragment that may contain a <code> tag with language attribute
///
/// # Returns
/// * Attribute string like ` class="language-rust"` (with leading space), or empty string
fn extract_code_lang_attribute(html: &str) -> String {
    if let Some(caps) = CODE_TAG_RE.captures(html) {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        
        // Only return attributes if they contain language information
        // This prevents returning empty or irrelevant attributes
        if attrs.contains("language-") || attrs.contains("lang-") || attrs.contains("data-lang") {
            attrs.to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    }
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
    // STEP 0: Pre-escape code blocks (CRITICAL - must be FIRST)
    // ============================================================================
    // Protect angle brackets in code from being interpreted as HTML tags
    // by the DOM parser. This preserves generic type parameters like Result<T>.
    let html = escape_code_blocks(html);

    // ============================================================================
    // STEP 1: Preprocess Expressive Code blocks (uses kuchiki DOM parsing)
    // ============================================================================
    // IMPORTANT: Skip if we have protected code blocks (indicated by placeholder prefix).
    // The kuchiki DOM parser strips unknown elements like <citescrape-code-block>,
    // which would destroy our protected placeholders.
    // NOTE: Code block protection is now handled at a higher level in convert()
    // to protect blocks BEFORE extract_main_content is called.
    let html = if html.contains("<!--CITESCRAPE-PRE-") {
        // Already protected at higher level - skip DOM parsing
        html
    } else {
        preprocess_expressive_code(&html)?
    };

    // ============================================================================
    // STEP 1.5: Merge fragmented code blocks (uses DOM parsing)
    // ============================================================================
    // IMPORTANT: Skip if we have protected code blocks (indicated by placeholder prefix).
    // The kuchiki DOM parser strips unknown elements like <citescrape-code-block>,
    // which would destroy our protected placeholders.
    let html = if html.contains("<!--CITESCRAPE-PRE-") {
        // Already protected at higher level - skip DOM parsing
        html
    } else {
        preprocess_code_blocks(&html)?
    };

    // ============================================================================
    // STEP 2: Regex-based cleaning (existing code)
    // ============================================================================
    // Use Cow to avoid unnecessary allocations
    let result = Cow::Borrowed(html.as_str());

    // Remove script tags and their contents
    let result = SCRIPT_RE.replace_all(&result, "");

    // Remove style tags and their contents
    let result = STYLE_RE.replace_all(&result, "");

    // Remove inline event handlers
    let result = EVENT_RE.replace_all(&result, "");

    // Remove comments (but preserve CITESCRAPE-PRE placeholders for code block protection)
    let result = COMMENT_RE.replace_all(&result, |caps: &regex::Captures| {
        let comment = &caps[0];
        // Preserve CITESCRAPE-PRE placeholder comments - they protect code blocks from DOM parsing
        if comment.starts_with("<!--CITESCRAPE-PRE-") {
            comment.to_string()
        } else {
            String::new()
        }
    });

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

    // Remove empty code/pre elements (prevents orphaned fence markers in markdown)
    // These are often styling placeholders with no semantic content
    let result = EMPTY_PRE_CODE.replace_all(&result, "");
    let result = EMPTY_PRE.replace_all(&result, "");
    let result = EMPTY_CODE.replace_all(&result, "");

    // Remove permalink anchor markers from headings (GitHub, Docusaurus, Jekyll, Eleventy)
    // These are visual indicators for deep linking, not content
    // Must happen before html2md conversion to prevent "## # Heading" duplication
    let result = HEADING_ANCHOR_MARKERS.replace_all(&result, "");

    // Remove "Navigate to header" accessibility links (Issue #007)
    // These appear as literal text in anchor elements for screen reader navigation
    // Must be removed before htmd conversion to prevent markdown link duplication
    // Pattern: <a ...>Navigate to header</a>
    let result = HEADING_NAVIGATE_TO_HEADER_RE.replace_all(&result, "");

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
    let result: Cow<str> = Cow::Owned(remove_interactive_elements_from_dom(&result));

    // ========================================================================
    // STAGE 3: Fix bold tags spanning code blocks
    // ========================================================================
    let result: Cow<str> = Cow::Owned(fix_bold_spanning_code_blocks(&result)?);

    // NOTE: Code block restoration is handled at a higher level in convert()
    // after clean_html_content returns.

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
}
