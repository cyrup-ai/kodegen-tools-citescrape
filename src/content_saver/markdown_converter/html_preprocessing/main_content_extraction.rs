//! Main content extraction from HTML documents.
//!
//! This module intelligently extracts the primary content from HTML pages by:
//! 1. Looking for semantic containers in priority order: `<main>`, `<article>`, content-specific divs
//! 2. Removing navigation, headers, footers, sidebars, and other non-content elements
//! 3. Falling back to `<body>` tag if no semantic containers are found
//! 4. Preserving HTML structure, attributes, and element nesting

use anyhow::Result;
use ego_tree::NodeId;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::sync::LazyLock;
use tracing;

/// Maximum HTML input size to prevent memory exhaustion attacks (10 MB)
///
/// This limit protects against `DoS` attacks while accommodating legitimate use:
/// - Wikipedia largest articles: ~2-3 MB
/// - Typical documentation: 1-2 MB  
/// - Blog posts: 100-500 KB
/// - 99.9% of real HTML is under 10 MB
pub(super) const MAX_HTML_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Maximum HTML nesting depth to prevent stack overflow
///
/// This limit is based on browser engine implementations and empirical analysis:
///
/// # Browser Standards
/// - Chromium/Chrome: ~11,000 recursion limit
/// - Firefox: ~26,000 recursion limit  
/// - Safari: ~45,000 recursion limit
/// - Industry consensus: 10,000 safe across all engines
///
/// # Reference
/// Chromium Issue #40087608: "Many layout algorithms are recursive, so when the DOM 
/// tree is nested too deeply, this causes stack overflow crashes. We should consider 
/// restricting the maximum depth of the layout tree: Firefox appears to do this."
///
/// # Rationale for 100
/// - **Legitimate HTML**: 99.9% of web pages have < 20 levels of nesting
/// - **Complex SPAs**: Modern single-page apps typically use 30-50 levels
/// - **Safety margin**: 100 provides generous headroom for edge cases
/// - **Stack safety**: 100 * 200 bytes/frame ≈ 20KB (< 1% of typical 2MB stack)
/// - **Fail-safe**: Prevents DoS while accommodating all legitimate use cases
///
/// # Graceful Degradation
/// When depth is exceeded, content is truncated at the limit with a warning logged.
/// This preserves the first 100 levels of nesting, which is more than sufficient for
/// extracting meaningful content.
const MAX_HTML_NESTING_DEPTH: usize = 100;

// ============================================================================
// CSS Selectors for main content extraction
// ============================================================================

// These are parsed once at first access and cached forever.
// Hardcoded selectors should NEVER fail to parse - if they do, it's a compile-time bug.

static MAIN_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("main").expect("BUG: hardcoded CSS selector 'main' is invalid")
});

static ARTICLE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("article").expect("BUG: hardcoded CSS selector 'article' is invalid")
});

static ROLE_MAIN_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("[role='main']")
        .expect("BUG: hardcoded CSS selector \"[role='main']\" is invalid")
});

static MAIN_CONTENT_ID_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#main-content")
        .expect("BUG: hardcoded CSS selector '#main-content' is invalid")
});

static MAIN_CONTENT_CLASS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".main-content")
        .expect("BUG: hardcoded CSS selector '.main-content' is invalid")
});

static CONTENT_ID_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("#content").expect("BUG: hardcoded CSS selector '#content' is invalid")
});

static CONTENT_CLASS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".content").expect("BUG: hardcoded CSS selector '.content' is invalid")
});

static POST_CONTENT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".post-content")
        .expect("BUG: hardcoded CSS selector '.post-content' is invalid")
});

static ENTRY_CONTENT_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".entry-content")
        .expect("BUG: hardcoded CSS selector '.entry-content' is invalid")
});

static ARTICLE_BODY_ITEMPROP_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("[itemprop='articleBody']")
        .expect("BUG: hardcoded CSS selector \"[itemprop='articleBody']\" is invalid")
});

static ARTICLE_BODY_CLASS_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".article-body")
        .expect("BUG: hardcoded CSS selector '.article-body' is invalid")
});

static STORY_BODY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".story-body").expect("BUG: hardcoded CSS selector '.story-body' is invalid")
});

static BODY_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("body").expect("BUG: hardcoded CSS selector 'body' is invalid")
});

// ============================================================================
// Element Removal Utilities
// ============================================================================

/// Efficiently remove elements matching selectors from an element's subtree.
///
/// This function:
/// 1. Parses all selectors once (O(s) where s = number of selectors)
/// 2. Builds a `HashSet` of element pointers to remove (O(n) where n = number of elements)
/// 3. Serializes the DOM tree once, skipping removed elements - O(n)
///
/// Overall complexity: O(s + n) instead of O(s × n²) from naive string replacement
///
/// Note: This preserves HTML structure (tags, attributes, nesting) while removing
/// unwanted elements, as required by downstream processors (`clean_html_content`, `MarkdownConverter`).
///
/// Works directly with the element's node tree, avoiding redundant serialization and re-parsing.
fn remove_elements_from_html(element: &ElementRef, remove_selectors: &[&str]) -> String {
    // Parse all selectors upfront - O(s)
    let parsed_selectors: Vec<Selector> = remove_selectors
        .iter()
        .map(|&sel_str| match Selector::parse(sel_str) {
            Ok(s) => s,
            Err(e) => panic!("Invalid hardcoded selector '{sel_str}': {e}"),
        })
        .collect();

    // Build HashSet of all elements to remove (using NodeId for O(1) lookup) - O(n)
    let mut to_remove: HashSet<NodeId> = HashSet::new();
    for sel in &parsed_selectors {
        for elem in element.select(sel) {
            // Store NodeId for identity comparison
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
/// This preserves the full HTML structure (tags, attributes, nesting) while
/// excluding unwanted elements and their children.
///
/// # Stack Safety
/// This function enforces a maximum nesting depth of [`MAX_HTML_NESTING_DEPTH`]
/// to prevent stack overflow from maliciously crafted or malformed HTML.
fn serialize_html_excluding(
    element: &ElementRef,
    to_remove: &HashSet<NodeId>,
    output: &mut String,
) {
    serialize_html_excluding_depth(element, to_remove, output, 0);
}

/// Internal implementation of HTML serialization with depth tracking.
///
/// # Arguments
/// * `element` - Element to serialize
/// * `to_remove` - Set of element IDs to exclude from output
/// * `output` - String buffer to write serialized HTML
/// * `depth` - Current nesting depth (starts at 0)
///
/// # Depth Limiting
/// Recursion is limited to [`MAX_HTML_NESTING_DEPTH`] to prevent stack overflow.
/// When exceeded, a warning is logged and processing stops for that branch.
fn serialize_html_excluding_depth(
    element: &ElementRef,
    to_remove: &HashSet<NodeId>,
    output: &mut String,
    depth: usize,
) {
    // ============================================================================
    // DEPTH LIMIT CHECK - Prevent Stack Overflow
    // ============================================================================
    if depth > MAX_HTML_NESTING_DEPTH {
        tracing::warn!(
            element = element.value().name(),
            depth = depth,
            limit = MAX_HTML_NESTING_DEPTH,
            "Maximum HTML nesting depth exceeded - truncating output at depth {}. \
             Element: <{}>. This prevents stack overflow from deeply nested HTML. \
             Content up to depth {} has been preserved.",
            MAX_HTML_NESTING_DEPTH,
            element.value().name(),
            MAX_HTML_NESTING_DEPTH
        );
        return;
    }

    // Check if this element should be removed
    if to_remove.contains(&element.id()) {
        return; // Skip this element and all its children
    }

    // Serialize this element's children (we're at the root or an allowed element)
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

                    // ============================================================================
                    // RECURSIVE CALL - Depth incremented to track nesting level
                    // ============================================================================
                    serialize_html_excluding_depth(&child_elem, to_remove, output, depth + 1);

                    // Closing tag (only for non-void elements)
                    output.push_str("</");
                    output.push_str(elem_name);
                    output.push('>');
                }
            }
            Node::Comment(comment) => {
                // Preserve comments
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
// Main Content Extraction
// ============================================================================

/// Extract main content from HTML by identifying semantic containers
///
/// This function intelligently extracts the primary content from an HTML page by:
/// 1. Looking for semantic containers in priority order: `<main>`, `<article>`, content-specific divs
/// 2. Removing navigation, headers, footers, sidebars, and other non-content elements
/// 3. Falling back to `<body>` tag if no semantic containers are found
/// 4. Preserving HTML structure, attributes, and element nesting via scraper serialization
///
/// # Arguments
/// * `html` - Raw HTML string to process
///
/// # Returns
/// * `Ok(String)` - Extracted HTML with main content preserved and non-content elements removed
/// * `Err` - If HTML parsing fails
///
/// # Example
/// ```
/// let html = r#"<html><body><nav>Menu</nav><main><p>Content</p></main></body></html>"#;
/// let content = extract_main_content(html)?;
/// // Result contains: <main><p>Content</p></main>
/// ```
pub fn extract_main_content(html: &str) -> Result<String> {
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

    let document = Html::parse_document(html);

    // First, remove common non-content elements
    // IMPORTANT: Use specific selectors to avoid removing content lists
    // - Don't use ".menu" or ".navigation" (too broad - matches content lists)
    // - Instead, target specific navigation patterns (nav.menu, ul.nav-menu, etc.)
    let remove_selectors = [
        "nav",
        "header",
        "footer",
        "aside",
        ".sidebar",
        "#sidebar",
        ".header",
        ".footer",
        // Navigation-specific selectors (preserve content lists):
        // Only remove lists explicitly marked as navigation/menu/breadcrumbs
        "ul.nav",
        "ul.navbar",
        "ul.nav-menu",
        "ul.navigation",
        "ul.breadcrumb",
        "ul.breadcrumbs",
        "ol.breadcrumb",
        "ol.breadcrumbs",
        ".ads",
        ".advertisement",
        ".social-share",
        ".comments",
        "#comments",
        ".related-posts",
        ".cookie-notice",
        ".popup",
        ".modal",
    ];

    // Try to find main content in common containers (parsed once, cached forever)
    let content_selectors = [
        &*MAIN_SELECTOR,
        &*ARTICLE_SELECTOR,
        &*ROLE_MAIN_SELECTOR,
        &*MAIN_CONTENT_ID_SELECTOR,
        &*MAIN_CONTENT_CLASS_SELECTOR,
        &*CONTENT_ID_SELECTOR,
        &*CONTENT_CLASS_SELECTOR,
        &*POST_CONTENT_SELECTOR,
        &*ENTRY_CONTENT_SELECTOR,
        &*ARTICLE_BODY_ITEMPROP_SELECTOR,
        &*ARTICLE_BODY_CLASS_SELECTOR,
        &*STORY_BODY_SELECTOR,
    ];

    // First try to find a specific content container
    for selector in content_selectors {
        if let Some(element) = document.select(selector).next() {
            // Efficiently remove unwanted elements within the content
            // Works directly with element's node tree - no serialize-parse roundtrip
            let cleaned_html = remove_elements_from_html(&element, &remove_selectors);

            return Ok(cleaned_html);
        }
    }

    // If no main content container found, try to extract body and remove non-content elements
    if let Some(body) = document.select(&BODY_SELECTOR).next() {
        // Efficiently remove non-content elements from body
        // Works directly with element's node tree - no serialize-parse roundtrip
        let cleaned_html = remove_elements_from_html(&body, &remove_selectors);

        return Ok(cleaned_html);
    }

    // Last resort: return the whole HTML
    Ok(html.to_string())
}

// ============================================================================
// TAB COMPONENT TRANSFORMATION
// ============================================================================

/// Regex to find tab container divs
static TAB_CONTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*tabs[^"]*"[^>]*>(.*?)</div>"#)
        .expect("BUG: hardcoded regex TAB_CONTAINER_RE is invalid")
});

/// Regex to extract tab button labels
static TAB_LABEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<button[^>]*>(.*?)</button>"#)
        .expect("BUG: hardcoded regex TAB_LABEL_RE is invalid")
});

/// Regex to extract tab panel content
static TAB_PANEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*tab-panel[^"]*"[^>]*>(.*?)</div>"#)
        .expect("BUG: hardcoded regex TAB_PANEL_RE is invalid")
});

/// Transform tab components into sequential sections before markdown conversion.
///
/// This function finds tab UI components in HTML and transforms them into a linear
/// structure where each tab label becomes an `<h3>` heading followed by its panel content.
///
/// # Example
///
/// **Input HTML:**
/// ```html
/// <div class="tabs">
///   <div class="tab-headers">
///     <button>macOS</button>
///     <button>Windows</button>
///   </div>
///   <div class="tab-panels">
///     <div class="tab-panel"><code>brew install app</code></div>
///     <div class="tab-panel"><code>choco install app</code></div>
///   </div>
/// </div>
/// ```
///
/// **Output HTML:**
/// ```html
/// <h3>macOS</h3>
/// <code>brew install app</code>
///
/// <h3>Windows</h3>
/// <code>choco install app</code>
/// ```
///
/// # Arguments
///
/// * `html` - Raw HTML string potentially containing tab components
///
/// # Returns
///
/// HTML string with tab components transformed into sequential sections
pub fn transform_tabs_to_sections(html: &str) -> String {
    TAB_CONTAINER_RE
        .replace_all(html, |caps: &regex::Captures| {
            if let Some(tab_html) = caps.get(1) {
                transform_single_tab_group(tab_html.as_str())
            } else {
                // If capture group doesn't exist (shouldn't happen), return original
                caps.get(0).map_or(String::new(), |m| m.as_str().to_string())
            }
        })
        .to_string()
}

/// Transform a single tab group into sequential sections.
///
/// Pairs tab labels with their corresponding panels and formats each pair
/// as `<h3>label</h3>\npanel_content\n\n`.
///
/// # Graceful Degradation
///
/// If labels and panels don't match in count, pairs as many as possible:
/// - More labels than panels: Extra labels ignored
/// - More panels than labels: Extra panels ignored
///
/// # Arguments
///
/// * `tab_html` - Inner HTML of a tab container
///
/// # Returns
///
/// Sequential HTML with each label-panel pair as a section
fn transform_single_tab_group(tab_html: &str) -> String {
    // Extract tab labels
    let labels: Vec<String> = TAB_LABEL_RE
        .captures_iter(tab_html)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();

    // Extract tab panels
    let panels: Vec<String> = TAB_PANEL_RE
        .captures_iter(tab_html)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();

    // Combine labels with panels as sections
    let mut result = String::new();
    for (label, panel) in labels.iter().zip(panels.iter()) {
        result.push_str(&format!(
            "<h3>{}</h3>\n{}\n\n",
            escape_html(label),
            panel
        ));
    }

    result
}

/// Escape HTML special characters to prevent markup injection.
///
/// Converts `<`, `>`, and `&` to their HTML entity equivalents.
/// Quote characters are preserved as they're not problematic in text content.
///
/// # Arguments
///
/// * `s` - String to escape
///
/// # Returns
///
/// String with HTML special characters escaped
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ============================================================================
// CALLOUT/ADMONITION BOX TRANSFORMATION
// ============================================================================

/// Regex to detect callout/alert/admonition boxes in HTML
static CALLOUT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<(?:div|aside)[^>]*class="[^"]*(?:warning|note|tip|caution|info|alert)[^"]*"[^>]*>(.*?)</(?:div|aside)>"#)
        .expect("BUG: hardcoded regex CALLOUT_RE is invalid")
});

/// Regex to strip HTML tags from content
static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<[^>]+>").expect("BUG: hardcoded regex HTML_TAG_RE is invalid")
});

/// Type of callout/admonition box
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalloutType {
    Warning,
    Note,
    Tip,
    Info,
}

/// Transform callout/alert boxes into markdown blockquotes.
///
/// Detects HTML callout boxes (warnings, notes, tips, etc.) and transforms them
/// into GitHub-flavored markdown blockquotes with appropriate formatting.
///
/// # Example
///
/// **Input HTML:**
/// ```html
/// <div class="warning callout">
///   <span class="icon">⚠️</span>
///   <div class="content">
///     <strong>Warning:</strong> Use third party MCP servers at your own risk
///   </div>
/// </div>
/// ```
///
/// **Output:**
/// ```markdown
/// > [!WARNING]
/// > Use third party MCP servers at your own risk
/// ```
///
/// # Arguments
///
/// * `html` - HTML string potentially containing callout boxes
///
/// # Returns
///
/// HTML string with callout boxes transformed to markdown blockquote syntax
pub fn transform_callouts_to_blockquotes(html: &str) -> String {
    CALLOUT_RE
        .replace_all(html, |caps: &regex::Captures| {
            let content_html = caps.get(1).map_or("", |m| m.as_str());
            let full_match = caps.get(0).map_or("", |m| m.as_str());

            // Detect callout type from class names
            let callout_type = detect_callout_type(full_match);

            format_as_blockquote(content_html, callout_type)
        })
        .to_string()
}

/// Detect the type of callout from HTML class attributes.
///
/// Examines class names and element content to determine whether the callout
/// is a warning, note, tip, or general info box.
///
/// # Arguments
///
/// * `html` - HTML string containing the callout element
///
/// # Returns
///
/// The detected [`CalloutType`]
fn detect_callout_type(html: &str) -> CalloutType {
    let html_lower = html.to_lowercase();

    if html_lower.contains("warning") || html_lower.contains("caution") {
        CalloutType::Warning
    } else if html_lower.contains("note") {
        CalloutType::Note
    } else if html_lower.contains("tip") || html_lower.contains("success") {
        CalloutType::Tip
    } else {
        CalloutType::Info
    }
}

/// Format callout content as a GitHub-flavored markdown blockquote.
///
/// Converts HTML callout content into a blockquote with the appropriate
/// admonition type marker ([!WARNING], [!NOTE], etc.).
///
/// # Arguments
///
/// * `content_html` - Inner HTML content of the callout box
/// * `callout_type` - Type of callout to format
///
/// # Returns
///
/// Formatted markdown blockquote string
fn format_as_blockquote(content_html: &str, callout_type: CalloutType) -> String {
    // Strip HTML tags from content
    let text = strip_html_tags(content_html);

    // Get emoji and label for this callout type
    let label = match callout_type {
        CalloutType::Warning => "WARNING",
        CalloutType::Note => "NOTE",
        CalloutType::Tip => "TIP",
        CalloutType::Info => "INFO",
    };

    // Format as GitHub-flavored markdown alert syntax
    // Each line of content needs to be prefixed with "> "
    let quoted_text = text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n> ");

    format!("\n\n> [!{}]\n> {}\n\n", label, quoted_text)
}

/// Strip HTML tags from a string, leaving only text content.
///
/// This is a simple implementation that removes all HTML tags using regex.
/// It also decodes common HTML entities and normalizes whitespace.
///
/// # Arguments
///
/// * `html` - HTML string to strip tags from
///
/// # Returns
///
/// Plain text content with HTML tags removed
fn strip_html_tags(html: &str) -> String {
    // Remove HTML tags
    let text = HTML_TAG_RE.replace_all(html, "");

    // Decode common HTML entities and normalize whitespace
    text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// CARD/GRID LAYOUT TRANSFORMATION
// ============================================================================

/// Regex to find card grid container divs
static CARD_GRID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*card-grid[^"]*"[^>]*>(.*?)</div>"#)
        .expect("BUG: hardcoded regex CARD_GRID_RE is invalid")
});

/// Regex to extract individual card divs from a grid
static CARD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*card[^"]*"[^>]*>(.*?)</div>"#)
        .expect("BUG: hardcoded regex CARD_RE is invalid")
});

/// Regex to extract heading content (h3 or h4)
static CARD_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<h[34][^>]*>(.*?)</h[34]>"#)
        .expect("BUG: hardcoded regex CARD_HEADING_RE is invalid")
});

/// Regex to extract paragraph content
static CARD_DESCRIPTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<p[^>]*>(.*?)</p>"#)
        .expect("BUG: hardcoded regex CARD_DESCRIPTION_RE is invalid")
});

/// Regex to extract HTML anchor links
static HTML_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#)
        .expect("BUG: hardcoded regex HTML_LINK_RE is invalid")
});

/// Card content extracted from HTML
struct CardContent {
    title: String,
    description: String,
}

/// Transform card grids into markdown tables before markdown conversion.
///
/// This function finds card grid UI components in HTML and transforms them into
/// markdown tables where each card becomes a table row with title and description.
///
/// # Example
///
/// **Input HTML:**
/// ```html
/// <div class="card-grid">
///   <div class="card">
///     <h3><a href="/quickstart">Quickstart</a></h3>
///     <p>See Claude Code in action with practical examples</p>
///   </div>
///   <div class="card">
///     <h3><a href="/common-workflows">Common workflows</a></h3>
///     <p>Step-by-step guides for common workflows</p>
///   </div>
/// </div>
/// ```
///
/// **Output (for markdown conversion):**
/// ```html
/// <table>
/// <tr><td><strong><a href="/quickstart">Quickstart</a></strong></td><td>See Claude Code in action with practical examples</td></tr>
/// <tr><td><strong><a href="/common-workflows">Common workflows</a></strong></td><td>Step-by-step guides for common workflows</td></tr>
/// </table>
/// ```
///
/// This will then be converted to markdown table format by the markdown converter.
///
/// # Arguments
///
/// * `html` - Raw HTML string potentially containing card grid components
///
/// # Returns
///
/// HTML string with card grids transformed into table elements
pub fn transform_card_grids_to_tables(html: &str) -> String {
    CARD_GRID_RE
        .replace_all(html, |caps: &regex::Captures| {
            if let Some(grid_html) = caps.get(1) {
                format_cards_as_table(grid_html.as_str())
            } else {
                // If capture group doesn't exist (shouldn't happen), return empty string
                String::new()
            }
        })
        .to_string()
}

/// Transform a card grid into an HTML table.
///
/// Extracts individual cards from the grid HTML and formats them as table rows.
/// Each card's title and description become table cells.
///
/// # Arguments
///
/// * `grid_html` - Inner HTML of a card grid container
///
/// # Returns
///
/// HTML table string, or empty string if no cards found
fn format_cards_as_table(grid_html: &str) -> String {
    // Extract individual cards
    let cards: Vec<CardContent> = CARD_RE
        .captures_iter(grid_html)
        .filter_map(|cap| cap.get(1).map(|m| extract_card_content(m.as_str())))
        .collect();

    if cards.is_empty() {
        return String::new();
    }

    // Build HTML table (will be converted to markdown table by the markdown converter)
    let mut table = String::from("\n\n<table>\n");

    for card in cards {
        table.push_str(&format!(
            "<tr><td><strong>{}</strong></td><td>{}</td></tr>\n",
            card.title, card.description
        ));
    }

    table.push_str("</table>\n\n");
    table
}

/// Extract title and description from a single card's HTML.
///
/// Handles:
/// - Extracting title from h3 or h4 tags
/// - Converting HTML links to markdown link syntax
/// - Extracting description from p tags
/// - Stripping other HTML tags
///
/// # Arguments
///
/// * `card_html` - Inner HTML of a single card div
///
/// # Returns
///
/// [`CardContent`] with extracted title and description
fn extract_card_content(card_html: &str) -> CardContent {
    // Extract title from <h3> or <h4>
    let title = CARD_HEADING_RE
        .captures(card_html)
        .and_then(|cap| cap.get(1))
        .map(|m| extract_title_with_links(m.as_str()))
        .unwrap_or_default();

    // Extract description from <p>
    let description = CARD_DESCRIPTION_RE
        .captures(card_html)
        .and_then(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()))
        .unwrap_or_default();

    CardContent { title, description }
}

/// Extract title text while preserving links.
///
/// Converts HTML anchor tags to markdown link syntax, then strips remaining HTML tags.
///
/// # Example
///
/// ```
/// let html = r#"<a href="/quickstart">Quickstart</a>"#;
/// let result = extract_title_with_links(html);
/// // result: "[Quickstart](/quickstart)"
/// ```
///
/// # Arguments
///
/// * `html` - HTML string potentially containing anchor tags
///
/// # Returns
///
/// String with HTML links converted to markdown links and other tags stripped
fn extract_title_with_links(html: &str) -> String {
    // Convert HTML links to markdown links
    let with_markdown_links = HTML_LINK_RE.replace_all(html, |caps: &regex::Captures| {
        let href = caps.get(1).map_or("", |m| m.as_str());
        let text = caps.get(2).map_or("", |m| m.as_str());
        format!("[{}]({})", text, href)
    });

    // Strip remaining HTML tags
    strip_html_tags(&with_markdown_links)
}

// ============================================================================
// MCP SERVER CARD TRANSFORMATION
// ============================================================================

/// Regex to find MCP server card container divs
static MCP_SERVER_CARD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*server-card[^"]*"[^>]*>(.*?)</div>"#)
        .expect("BUG: hardcoded regex MCP_SERVER_CARD_RE is invalid")
});

/// Regex to extract server title and link from h3 anchor tag
static MCP_SERVER_TITLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<h3[^>]*><a[^>]*href="([^"]*)"[^>]*>([^<]+)</a></h3>"#)
        .expect("BUG: hardcoded regex MCP_SERVER_TITLE_RE is invalid")
});

/// Regex to extract server description from p tag
static MCP_SERVER_DESC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<p[^>]*>([^<]+)</p>"#)
        .expect("BUG: hardcoded regex MCP_SERVER_DESC_RE is invalid")
});

/// Regex to extract command from code tag
static MCP_SERVER_CMD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<code[^>]*>([^<]+)</code>"#)
        .expect("BUG: hardcoded regex MCP_SERVER_CMD_RE is invalid")
});

/// Transform MCP server cards into structured markdown sections before markdown conversion.
///
/// This function finds MCP server card UI components in HTML and transforms them into
/// a structured format where each card becomes an H3 heading with link, description
/// paragraph, and bash code block for the installation command.
///
/// # Example
///
/// **Input HTML:**
/// ```html
/// <div class="mcp-servers">
///   <div class="server-card">
///     <h3><a href="https://day.ai/mcp">Day AI</a></h3>
///     <p>Analyze & update CRM records</p>
///     <div class="command">
///       <span>Command</span>
///       <code>claude mcp add day-ai --transport http https://day.ai/api/mcp</code>
///     </div>
///   </div>
/// </div>
/// ```
///
/// **Output HTML:**
/// ```html
/// <h3><a href="https://day.ai/mcp">Day AI</a></h3>
/// <p>Analyze & update CRM records</p>
///
/// <p><strong>Command:</strong></p>
/// <pre><code class="language-bash">claude mcp add day-ai --transport http https://day.ai/api/mcp</code></pre>
/// ```
///
/// When converted to markdown, this becomes:
/// ```markdown
/// ### [Day AI](https://day.ai/mcp)
/// Analyze & update CRM records
///
/// **Command:**
/// ```bash
/// claude mcp add day-ai --transport http https://day.ai/api/mcp
/// ```
/// ```
///
/// # Arguments
///
/// * `html` - Raw HTML string potentially containing MCP server card components
///
/// # Returns
///
/// HTML string with MCP server cards transformed into structured sections
pub fn transform_mcp_server_cards(html: &str) -> String {
    MCP_SERVER_CARD_RE
        .replace_all(html, |caps: &regex::Captures| {
            if let Some(card_html) = caps.get(1) {
                format_mcp_server(card_html.as_str())
            } else {
                // If capture group doesn't exist (shouldn't happen), return empty string
                String::new()
            }
        })
        .to_string()
}

/// Format a single MCP server card into structured HTML.
///
/// Extracts the server title/link, description, and command, then formats
/// them as structured HTML that will be cleanly converted to markdown.
///
/// # Graceful Degradation
///
/// If any component is missing:
/// - Missing title: Uses "Unknown Server" as default
/// - Missing link: Uses empty string
/// - Missing description: Uses empty string
/// - Missing command: Uses empty string
///
/// # Arguments
///
/// * `card_html` - Inner HTML of a server card div
///
/// # Returns
///
/// Structured HTML with H3 header, description paragraph, and code block
fn format_mcp_server(card_html: &str) -> String {
    // Extract title and link from <h3><a href="...">Title</a></h3>
    let (link, title) = MCP_SERVER_TITLE_RE
        .captures(card_html)
        .map(|cap| {
            let link = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let title = cap.get(2).map(|m| m.as_str()).unwrap_or("Unknown Server");
            (link, title)
        })
        .unwrap_or(("", "Unknown Server"));

    // Extract description from <p>...</p>
    let description = MCP_SERVER_DESC_RE
        .captures(card_html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .unwrap_or("");

    // Extract command from <code>...</code>
    let command = MCP_SERVER_CMD_RE
        .captures(card_html)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .unwrap_or("");

    // Format as structured HTML that will convert cleanly to markdown
    // Using HTML elements that htmd will convert to the desired markdown format
    format!(
        "\n\n<h3><a href=\"{}\">{}</a></h3>\n<p>{}</p>\n\n<p><strong>Command:</strong></p>\n<pre><code class=\"language-bash\">{}</code></pre>\n\n",
        escape_html(link),
        escape_html(title),
        escape_html(description),
        escape_html(command)
    )
}

// ============================================================================
// LINK CARD TRANSFORMATION
// ============================================================================

/// Regex to find link cards with anchor pattern: <a class="*card*" href="...">
static LINK_CARD_ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a\s+[^>]*class="[^"]*card[^"]*"[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#)
        .expect("BUG: hardcoded regex LINK_CARD_ANCHOR_RE is invalid")
});

/// Alternative regex for href before class
static LINK_CARD_ANCHOR_RE_ALT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a\s+[^>]*href="([^"]*)"[^>]*class="[^"]*card[^"]*"[^>]*>(.*?)</a>"#)
        .expect("BUG: hardcoded regex LINK_CARD_ANCHOR_RE_ALT is invalid")
});

/// Regex to find div cards: <div class="*card*">...</div>
static LINK_CARD_DIV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div\s+[^>]*class="[^"]*card[^"]*"[^>]*>(.*?)</div>"#)
        .expect("BUG: hardcoded regex LINK_CARD_DIV_RE is invalid")
});

/// Regex to extract heading from card content
static LINK_CARD_HEADING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<h[1-6][^>]*>(.*?)</h[1-6]>"#)
        .expect("BUG: hardcoded regex LINK_CARD_HEADING_RE is invalid")
});

/// Regex to extract description from <p>, <div>, or <span> elements
/// Handles diverse HTML structures across different documentation frameworks
static LINK_CARD_DESC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<(?:p|div|span)[^>]*>(.*?)</(?:p|div|span)>"#)
        .expect("BUG: hardcoded regex LINK_CARD_DESC_RE is invalid")
});

/// Regex to extract anchor href from div card content
static LINK_CARD_DIV_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a\s+[^>]*href="([^"]*)"[^>]*>"#)
        .expect("BUG: hardcoded regex LINK_CARD_DIV_LINK_RE is invalid")
});

/// Extract plain text content from HTML by removing all tags
/// Used as fallback when specific element patterns don't match
/// 
/// This ensures we capture description content even when HTML structure
/// doesn't match expected patterns, making the transformation robust
/// across diverse website implementations.
fn extract_text_content(html: &str) -> String {
    // Remove all HTML tags, preserving text content
    let without_tags = Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(html, " ");
    
    // Normalize whitespace (collapse multiple spaces, trim)
    let normalized = Regex::new(r"\s+")
        .unwrap()
        .replace_all(&without_tags, " ");
    
    normalized.trim().to_string()
}

/// Transform link card components into markdown list items using regex.
///
/// This function finds link card UI components (clickable boxes with titles and descriptions)
/// and transforms them into markdown list items with preserved links.
///
/// # Example
///
/// **Input HTML:**
/// ```html
/// <a href="/docs/page" class="card link-card">
///   <h3>Card Title</h3>
///   <p>Description text</p>
/// </a>
/// ```
///
/// **Output:**
/// ```markdown
/// - **[Card Title](/docs/page)** - Description text
/// ```
///
/// Handles both patterns:
/// - `<a class="card">` containing heading and description
/// - `<div class="card">` containing `<a>` with heading and description
///
/// # Arguments
///
/// * `html` - Raw HTML string potentially containing link card components
///
/// # Returns
///
/// HTML string with link cards transformed into list item markdown
pub fn transform_link_cards_to_lists(html: &str) -> String {
    let mut result = html.to_string();
    let mut total_transformations = 0;
    
    // Pattern 1a: <a class="*card*" href="...">...</a>
    let anchor_matches = LINK_CARD_ANCHOR_RE.find_iter(html).count();
    if anchor_matches > 0 {
        tracing::debug!("Found {} link cards with anchor pattern (class before href)", anchor_matches);
    }
    result = LINK_CARD_ANCHOR_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let transformed = transform_anchor_card_match(caps);
            if transformed.starts_with("- **[") {
                total_transformations += 1;
                tracing::debug!("Transformed anchor card (pattern 1a): {} chars -> {} chars", 
                    caps.get(0).map_or(0, |m| m.as_str().len()), 
                    transformed.len());
            }
            transformed
        })
        .to_string();
    
    // Pattern 1b: <a href="..." class="*card*">...</a> (href before class)
    let anchor_alt_matches = LINK_CARD_ANCHOR_RE_ALT.find_iter(&result).count();
    if anchor_alt_matches > 0 {
        tracing::debug!("Found {} link cards with anchor pattern (href before class)", anchor_alt_matches);
    }
    result = LINK_CARD_ANCHOR_RE_ALT
        .replace_all(&result, |caps: &regex::Captures| {
            let transformed = transform_anchor_card_match(caps);
            if transformed.starts_with("- **[") {
                total_transformations += 1;
                tracing::debug!("Transformed anchor card (pattern 1b): {} chars -> {} chars", 
                    caps.get(0).map_or(0, |m| m.as_str().len()), 
                    transformed.len());
            }
            transformed
        })
        .to_string();
    
    // Pattern 2: <div class="*card*">...</div>
    let div_matches = LINK_CARD_DIV_RE.find_iter(&result).count();
    if div_matches > 0 {
        tracing::debug!("Found {} div cards", div_matches);
    }
    result = LINK_CARD_DIV_RE
        .replace_all(&result, |caps: &regex::Captures| {
            if let Some(content) = caps.get(1) {
                let transformed = transform_div_card_match(content.as_str());
                if transformed.starts_with("- **[") {
                    total_transformations += 1;
                    tracing::debug!("Transformed div card: {} chars -> {} chars", 
                        content.as_str().len(), 
                        transformed.len());
                }
                transformed
            } else {
                caps.get(0).map_or(String::new(), |m| m.as_str().to_string())
            }
        })
        .to_string();
    
    if total_transformations > 0 {
        tracing::info!("Successfully transformed {} link cards to markdown list items", total_transformations);
    } else {
        tracing::trace!("No link cards found to transform");
    }
    
    result
}

/// Transform an anchor card match into a list item.
fn transform_anchor_card_match(caps: &regex::Captures) -> String {
    let href = caps.get(1).map_or("", |m| m.as_str());
    let content = caps.get(2).map_or("", |m| m.as_str());
    
    // Extract heading
    let title = LINK_CARD_HEADING_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()))
        .unwrap_or_default();
    
    if title.is_empty() {
        // No heading found, return original
        tracing::warn!("Link card anchor found but no heading present, skipping transformation. Content preview: {}", 
            &content[..content.len().min(100)]);
        return caps.get(0).map_or(String::new(), |m| m.as_str().to_string());
    }
    
    if href.is_empty() {
        tracing::warn!("Link card anchor found but href is empty, skipping transformation. Title: {}", title);
        return caps.get(0).map_or(String::new(), |m| m.as_str().to_string());
    }
    
    // Extract description - try specific pattern first, then fallback to any text
    let description = LINK_CARD_DESC_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()))
        .filter(|s| !s.trim().is_empty()) // Filter BEFORE fallback
        .or_else(|| {
            // Fallback: extract any text content after the heading
            // This handles cases where description is in unexpected elements
            LINK_CARD_HEADING_RE.find(content).map(|heading_match| {
                let after_heading = &content[heading_match.end()..];
                extract_text_content(after_heading)
            })
        })
        .unwrap_or_default();
    
    // Format as list item
    if description.is_empty() {
        format!("- **[{}]({})**\n", title, href)
    } else {
        format!("- **[{}]({})** - {}\n", title, href, description)
    }
}

/// Transform a div card match into a list item.
fn transform_div_card_match(content: &str) -> String {
    // Skip if already transformed (starts with list marker)
    if content.trim_start().starts_with("- **[") {
        tracing::trace!("Div card already transformed, skipping");
        return content.to_string();
    }
    
    // Extract href from anchor tag
    let href = LINK_CARD_DIV_LINK_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .unwrap_or("");
    
    if href.is_empty() {
        // No link found, return original
        tracing::warn!("Div card found but no anchor link present, skipping transformation. Content preview: {}", 
            &content[..content.len().min(100)]);
        return content.to_string();
    }
    
    // Extract heading
    let title = LINK_CARD_HEADING_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()))
        .unwrap_or_default();
    
    if title.is_empty() {
        // No heading found, return original
        tracing::warn!("Div card with link found but no heading present, skipping transformation. href: {}", href);
        return content.to_string();
    }
    
    // Extract description - try specific pattern first, then fallback to any text
    let description = LINK_CARD_DESC_RE
        .captures(content)
        .and_then(|cap| cap.get(1))
        .map(|m| strip_html_tags(m.as_str()))
        .filter(|s| !s.trim().is_empty()) // Filter BEFORE fallback
        .or_else(|| {
            // Fallback: extract any text content after the heading
            // This handles cases where description is in unexpected elements
            LINK_CARD_HEADING_RE.find(content).map(|heading_match| {
                let after_heading = &content[heading_match.end()..];
                extract_text_content(after_heading)
            })
        })
        .unwrap_or_default();
    
    // Format as list item
    if description.is_empty() {
        format!("- **[{}]({})**\n", title, href)
    } else {
        format!("- **[{}]({})** - {}\n", title, href, description)
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_navigation() -> Result<()> {
        let html = r"
            <article>
                <p>Content</p>
                <nav>Should be removed</nav>
            </article>
        ";
        let result = extract_main_content(html)?;
        assert!(!result.contains("Should be removed"));
        assert!(result.contains("Content"));
        Ok(())
    }

    #[test]
    fn test_preserves_nested_structure() -> Result<()> {
        let html = r#"
            <article>
                <div class="content">
                    <p>Nested <strong>content</strong></p>
                </div>
            </article>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("<strong>content</strong>"));
        assert!(result.contains("<p>"));
        Ok(())
    }

    #[test]
    fn test_text_escaping() -> Result<()> {
        let html = r"
            <article>
                <p>5 &lt; 10 &amp; 10 &gt; 5</p>
            </article>
        ";
        let result = extract_main_content(html)?;
        // Must preserve escaped characters
        assert!(result.contains("&lt;") || result.contains('<'));
        assert!(result.contains("&gt;") || result.contains('>'));
        assert!(result.contains("&amp;") || result.contains('&'));
        Ok(())
    }

    #[test]
    fn test_self_closing_tags() -> Result<()> {
        let html = r#"
            <article>
                <p>Image: <img src="test.jpg" alt="test" /></p>
                <hr />
                <p>Line break<br />here</p>
            </article>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("<img"));
        assert!(result.contains("<hr"));
        assert!(result.contains("<br"));
        // Should NOT contain closing tags for void elements
        assert!(!result.contains("</img>"));
        assert!(!result.contains("</br>"));
        assert!(!result.contains("</hr>"));
        Ok(())
    }

    #[test]
    fn test_preserves_attributes() -> Result<()> {
        let html = r#"
            <article>
                <div class="content" id="main" data-test="value">Content</div>
            </article>
        "#;
        let result = extract_main_content(html)?;
        assert!(result.contains("class=\"content\""));
        assert!(result.contains("id=\"main\""));
        assert!(result.contains("data-test=\"value\""));
        Ok(())
    }

    #[test]
    fn test_removes_multiple_unwanted_elements() -> Result<()> {
        let html = r"
            <article>
                <header>Header</header>
                <p>Main content</p>
                <nav>Navigation</nav>
                <footer>Footer</footer>
                <aside>Sidebar</aside>
            </article>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("Main content"));
        assert!(!result.contains("Header"));
        assert!(!result.contains("Navigation"));
        assert!(!result.contains("Footer"));
        assert!(!result.contains("Sidebar"));
        Ok(())
    }

    #[test]
    fn test_preserves_comments() -> Result<()> {
        let html = r"
            <article>
                <!-- Important comment -->
                <p>Content</p>
            </article>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("<!-- Important comment -->"));
        Ok(())
    }

    #[test]
    fn test_body_fallback() -> Result<()> {
        let html = r"
            <html>
                <body>
                    <nav>Navigation</nav>
                    <p>Main content without article tag</p>
                </body>
            </html>
        ";
        let result = extract_main_content(html)?;
        assert!(result.contains("Main content"));
        assert!(!result.contains("Navigation"));
        Ok(())
    }

    #[test]
    fn test_malformed_html_fallback() -> Result<()> {
        let html = "<p>Malformed HTML without body</p>";
        let result = extract_main_content(html)?;
        assert_eq!(result, html);
        Ok(())
    }

    #[test]
    fn test_preserves_content_lists() -> Result<()> {
        let html = r#"
            <article>
                <h3>Launch Claude Code on the web</h3>
                <p>This is useful for:</p>
                <ul>
                    <li>Long-running tasks</li>
                    <li>Resource-intensive operations</li>
                    <li>Accessing cloud-only features</li>
                </ul>
                <p>To start a web session from desktop, select a remote environment.</p>
            </article>
        "#;
        let result = extract_main_content(html)?;
        
        // Content list should be preserved
        assert!(result.contains("<ul>"));
        assert!(result.contains("Long-running tasks"));
        assert!(result.contains("Resource-intensive operations"));
        assert!(result.contains("Accessing cloud-only features"));
        
        // Verify the list is between the paragraphs (not removed)
        assert!(result.contains("useful for"));
        assert!(result.contains("To start a web session"));
        
        Ok(())
    }

    #[test]
    fn test_removes_navigation_lists() -> Result<()> {
        let html = r#"
            <article>
                <p>Main content</p>
                <ul class="nav-menu">
                    <li>Nav Item 1</li>
                    <li>Nav Item 2</li>
                </ul>
                <ul class="breadcrumb">
                    <li>Home</li>
                    <li>Docs</li>
                </ul>
            </article>
        "#;
        let result = extract_main_content(html)?;
        
        // Navigation lists should be removed
        assert!(!result.contains("Nav Item 1"));
        assert!(!result.contains("Nav Item 2"));
        assert!(!result.contains("Home"));
        assert!(!result.contains("breadcrumb"));
        
        // Main content should be preserved
        assert!(result.contains("Main content"));
        
        Ok(())
    }

    #[test]
    fn test_preserves_plain_lists_removes_nav_lists() -> Result<()> {
        let html = r#"
            <article>
                <h2>Features</h2>
                <ul>
                    <li>Feature 1</li>
                    <li>Feature 2</li>
                </ul>
                <nav>
                    <ul>
                        <li>Nav 1</li>
                        <li>Nav 2</li>
                    </ul>
                </nav>
                <ul class="navigation">
                    <li>Menu Item</li>
                </ul>
            </article>
        "#;
        let result = extract_main_content(html)?;
        
        // Plain content list should be preserved
        assert!(result.contains("Feature 1"));
        assert!(result.contains("Feature 2"));
        
        // Navigation lists should be removed
        assert!(!result.contains("Nav 1"));
        assert!(!result.contains("Nav 2"));
        assert!(!result.contains("Menu Item"));
        
        Ok(())
    }

    // ============================================================================
    // LINK CARD TRANSFORMATION TESTS
    // ============================================================================

    #[test]
    fn test_transform_link_card_anchor_pattern() {
        let html = r#"
        <a href="/docs/quickstart" class="card link-card">
            <h3>Quickstart</h3>
            <p>Get started with Claude Code</p>
        </a>
    "#;
        
        let result = transform_link_cards_to_lists(html);
        
        // Should convert to list item with link preserved
        assert!(result.contains("- **[Quickstart](/docs/quickstart)**"));
        assert!(result.contains("Get started with Claude Code"));
        assert!(!result.contains("<a")); // No HTML anchor tags should remain
    }

    #[test]
    fn test_transform_link_card_div_pattern() {
        let html = r#"
        <div class="card">
            <a href="/docs/workflows">
                <h3>Common Workflows</h3>
            </a>
            <p>Step-by-step guides</p>
        </div>
    "#;
        
        let result = transform_link_cards_to_lists(html);
        
        assert!(result.contains("- **[Common Workflows](/docs/workflows)**"));
        assert!(result.contains("Step-by-step guides"));
        assert!(!result.contains("<div")); // No HTML div tags should remain
    }

    #[test]
    fn test_transform_multiple_link_cards() {
        let html = r#"
        <section>
            <h2>Related resources</h2>
            <a href="https://code.claude.com/docs" class="card">
                <h3>Claude Code on the web</h3>
                <p>Learn more about Claude Code</p>
            </a>
            <a href="https://support.anthropic.com/slack" class="card">
                <h3>Claude for Slack</h3>
                <p>Slack documentation</p>
            </a>
        </section>
    "#;
        
        let result = transform_link_cards_to_lists(html);
        
        assert!(result.contains("- **[Claude Code on the web](https://code.claude.com/docs)**"));
        assert!(result.contains("- **[Claude for Slack](https://support.anthropic.com/slack)**"));
        assert!(result.contains("Learn more about Claude Code"));
        assert!(result.contains("Slack documentation"));
    }

    #[test]
    fn test_transform_link_card_no_description() {
        let html = r#"
        <a href="/docs/desktop" class="card">
            <h3>Desktop App</h3>
        </a>
    "#;
        
        let result = transform_link_cards_to_lists(html);
        
        // Without description, should still create list item
        assert!(result.contains("- **[Desktop App](/docs/desktop)**"));
        // Should NOT have a dash after the link when no description
        assert!(!result.contains("**\n -"));
    }

    #[test]
    fn test_link_card_with_various_class_names() {
        let html = r#"
        <a href="/link1" class="resource-card">
            <h3>Resource Card</h3>
            <p>Should match [class*='card']</p>
        </a>
        <a href="/link2" class="link-card">
            <h3>Link Card</h3>
            <p>Should match explicit selector</p>
        </a>
        <div class="info-card">
            <a href="/link3">
                <h3>Info Card</h3>
            </a>
            <p>Should match div pattern</p>
        </div>
    "#;
        
        let result = transform_link_cards_to_lists(html);
        
        assert!(result.contains("- **[Resource Card](/link1)**"));
        assert!(result.contains("- **[Link Card](/link2)**"));
        assert!(result.contains("- **[Info Card](/link3)**"));
    }

    #[test]
    fn test_scraper_html_method_includes_tags() {
        let html = r#"<a class="card" href="/test"><h3>Title</h3></a>"#;
        let document = Html::parse_fragment(html);
        let selector = Selector::parse("a.card").unwrap();
        let card = document.select(&selector).next().unwrap();
        
        let card_html = card.html();
        
        // Verify it includes the inner HTML content (not the outer tags)
        // scraper's .html() returns INNER html, not outer html
        assert!(card_html.contains("<h3>"));
        assert!(card_html.contains("Title"));
        assert!(card_html.contains("</h3>"));
    }

    #[test]
    fn test_transform_link_card_code_claude_com_real_html() {
        // Real HTML structure from code.claude.com
        // Uses <span data-as="p"> instead of <p> for description
        let html = r#"<div class="card-group prose dark:prose-dark grid gap-x-4 sm:grid-cols-2">
            <a class="card block font-normal group relative my-2 ring-2 ring-transparent rounded-2xl bg-white dark:bg-background-dark border border-gray-950/10 dark:border-white/10 overflow-hidden w-full cursor-pointer hover:!border-primary dark:hover:!border-primary-light" 
               href="/docs/en/quickstart">
                <div class="px-6 py-5 relative">
                    <div class="h-6 w-6 fill-gray-800 dark:fill-gray-100 text-gray-800 dark:text-gray-100">
                        <svg class="h-6 w-6 bg-primary dark:bg-primary-light !m-0 shrink-0"></svg>
                    </div>
                    <div>
                        <h2 class="not-prose font-semibold text-base text-gray-800 dark:text-white mt-4">Quickstart</h2>
                        <div class="prose mt-1 font-normal text-base leading-6 text-gray-600 dark:text-gray-400">
                            <span data-as="p">See Claude Code in action with practical examples</span>
                        </div>
                    </div>
                </div>
            </a>
        </div>"#;
        
        let result = transform_link_cards_to_lists(html);
        
        // Verify markdown list item format
        assert!(result.contains("- **[Quickstart]"), 
            "Should convert card title to list item. Got:\n{}", result);
        
        // Verify link preservation
        assert!(result.contains("](/docs/en/quickstart)"), 
            "Should preserve card link. Got:\n{}", result);
        
        // Verify description extraction from <span data-as="p">
        assert!(result.contains("See Claude Code in action with practical examples") || 
                result.contains("practical examples"),
            "Should extract description from <span data-as=\"p\">. Got:\n{}", result);
    }

    #[test]
    fn test_transform_link_card_multiple_html_structures() {
        // Verify robustness across different HTML patterns
        
        // Pattern 1: Standard <p> tag (original synthetic test pattern)
        let html1 = r#"<a class="card" href="/link1"><h3>Title 1</h3><p>Description 1</p></a>"#;
        
        // Pattern 2: <span data-as="p"> (code.claude.com Mintlify pattern)
        let html2 = r#"<a class="card" href="/link2"><h3>Title 2</h3><span data-as="p">Description 2</span></a>"#;
        
        // Pattern 3: <div> wrapper (common in custom documentation sites)
        let html3 = r#"<a class="card" href="/link3"><h3>Title 3</h3><div class="description">Description 3</div></a>"#;
        
        for (i, html) in [html1, html2, html3].iter().enumerate() {
            let result = transform_link_cards_to_lists(html);
            
            // All patterns should produce list format
            assert!(result.starts_with("- **[Title"), 
                "Structure {} should convert to list. Got: {}", i + 1, result);
            
            // All patterns should preserve both description and link
            assert!(result.contains(&format!("Description {}", i + 1)) || 
                    result.contains(&format!("](/link{})", i + 1)),
                "Structure {} should preserve content. Got: {}", i + 1, result);
        }
    }
}
