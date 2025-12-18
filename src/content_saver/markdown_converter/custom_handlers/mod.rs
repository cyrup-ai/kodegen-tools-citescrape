//! Custom handlers for htmd HTML-to-Markdown conversion
//!
//! This module provides custom element handlers for htmd that extend
//! the default behavior with language inference and improved link handling.

pub mod language_inference;
pub mod language_patterns;
pub mod list_processing;

use htmd::{
    element_handler::{HandlerResult, Handlers},
    Element, HtmlToMarkdown,
};

use language_inference::{
    extract_language_from_class, infer_language_from_content, validate_html_language,
};

/// Create an htmd converter with custom handlers for citescrape
///
/// Custom handlers:
/// - Block elements (`<p>`, `<h1>`-`<h6>`): Proper spacing to prevent text concatenation
/// - Code blocks (`<pre>`, `<code>`): Language inference from content when HTML lacks hints
/// - Links (`<a>`): Fallback text extraction from aria-label, title, or cleaned href
/// - Text formatting (`<strong>`, `<em>`): Preserve bold and italic formatting
/// - Generic inline elements: Ensure text is never lost from unsupported HTML elements
pub fn create_converter() -> HtmlToMarkdown {
    // Define all inline elements that should preserve text
    // Note: "span" has a dedicated handler for CSS style detection
    let inline_elements = vec![
        "div", "kbd", "samp", "var", "mark", "time",
        "abbr", "cite", "q", "dfn", "data", "s", "del", "ins", 
        "u", "ruby", "rt", "rp", "bdi", "bdo",
    ];
    
    HtmlToMarkdown::builder()
        // Block-level element handlers (MUST come first for priority)
        .add_handler(vec!["p"], p_handler)
        .add_handler(vec!["h1", "h2", "h3", "h4", "h5", "h6"], heading_handler)
        
        // Special formatting elements
        .add_handler(vec!["strong", "b"], strong_handler)
        .add_handler(vec!["em", "i"], em_handler)
        
        // Span handler for CSS inline styles (bold/italic detection)
        .add_handler(vec!["span"], span_handler)
        
        // Code and links
        .add_handler(vec!["pre"], pre_handler)
        .add_handler(vec!["code"], code_handler)
        .add_handler(vec!["a"], link_handler)
        
        // Custom video handler - convert to markdown link instead of fallback text
        .add_handler(vec!["video"], video_handler)
        // Custom audio handler - convert to markdown link instead of fallback text
        .add_handler(vec!["audio"], audio_handler)
        
        // Custom list handlers for proper nested content extraction
        .add_handler(vec!["ol"], ol_handler)
        .add_handler(vec!["ul"], ul_handler)
        .add_handler(vec!["li"], li_handler)
        
        // Table handlers - MUST be registered to override htmd defaults
        .add_handler(vec!["table"], table_handler)
        .add_handler(vec!["thead"], thead_handler)
        .add_handler(vec!["tbody"], tbody_handler)
        .add_handler(vec!["tr"], tr_handler)
        .add_handler(vec!["th"], th_handler)
        .add_handler(vec!["td"], td_handler)
        
        // Fallback for all other inline elements
        // This ensures text is NEVER lost
        .add_handler(inline_elements, inline_element_handler)
        .build()
}

/// Handle `<p>` elements - paragraphs with proper spacing
///
/// Paragraphs are block-level elements that require blank lines before and after
/// to prevent concatenation with adjacent content (headings, other paragraphs, lists).
fn p_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Extract paragraph content (handles nested inline elements)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    if content.is_empty() {
        // Skip empty paragraphs
        return None;
    }
    
    // Wrap with blank lines: "\n\n{content}\n\n"
    // This ensures separation from adjacent block elements
    Some(HandlerResult::from(format!("\n\n{}\n\n", content)))
}

/// Handle heading elements (`<h1>` through `<h6>`) with proper spacing
///
/// Headings are block-level elements requiring blank lines before and after.
/// Markdown headings use ATX style: `# H1`, `## H2`, etc.
fn heading_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    use markup5ever_rcdom::NodeData;
    
    // Determine heading level from tag name
    let level = if let NodeData::Element { name, .. } = &element.node.data {
        match &*name.local {
            "h1" => 1,
            "h2" => 2,
            "h3" => 3,
            "h4" => 4,
            "h5" => 5,
            "h6" => 6,
            _ => return None,
        }
    } else {
        return None;
    };
    
    // Extract heading content (handles nested inline elements)
    // Use SAME approach as p_handler - call walk_children on the element node
    // This allows htmd to properly combine adjacent text nodes
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    if content.is_empty() {
        // Skip empty headings
        return None;
    }
    
    // Generate ATX-style markdown heading
    let heading_prefix = "#".repeat(level);
    
    // Wrap with blank lines: "\n\n{## Heading}\n\n"
    // This ensures separation from adjacent block elements
    Some(HandlerResult::from(format!("\n\n{} {}\n\n", heading_prefix, content)))
}

/// Handle `<pre>` elements - code blocks with fences
///
/// Strategy:
/// - If `<pre>` contains a `<code>` child, let walk_children process it via code_handler
/// - If `<pre>` has no `<code>` child, extract raw text directly to preserve whitespace
///
/// This dual approach handles both common HTML structures:
/// - `<pre><code>...</code></pre>` (proper HTML, delegates to code_handler)
/// - `<pre>...</pre>` (bare pre, direct extraction to avoid whitespace collapse)
fn pre_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    use markup5ever_rcdom::NodeData;
    
    // Check if <pre> contains a <code> child element
    let has_code_child = element
        .node
        .children
        .borrow()
        .iter()
        .any(|child| {
            if let NodeData::Element { name, .. } = &child.data {
                &*name.local == "code"
            } else {
                false
            }
        });
    
    if has_code_child {
        // Case 1: <pre><code>...</code></pre>
        // Let code_handler process the <code> child (it uses extract_raw_text)
        let result = handlers.walk_children(element.node);
        let content = result.content.trim_matches('\n');
        
        // If code_handler already added fences, just wrap with newlines
        if content.starts_with("```") && content.ends_with("```") {
            return Some(HandlerResult::from(format!("\n\n{}\n\n", content)));
        }
        
        // Fallback: add fences (shouldn't normally reach here)
        let language = get_language_from_element(&element)
            .or_else(|| infer_language_from_content(content));
        
        let fence = match &language {
            Some(lang) => format!("```{}", lang),
            None => "```".to_string(),
        };
        
        Some(HandlerResult::from(format!(
            "\n\n{}\n{}\n```\n\n",
            fence, content
        )))
    } else {
        // Case 2: <pre>...</pre> (no <code> child)
        // Extract raw text directly to preserve whitespace
        // This is the FIX for the pip install bug
        let raw_content = extract_raw_text(element.node);
        let raw_content = raw_content.trim();
        
        // Infer language BEFORE normalization so we can pass it
        let language = get_language_from_element(&element)
            .or_else(|| infer_language_from_content(raw_content));
        
        // Use raw content directly - no whitespace manipulation
        let content = raw_content;
        
        if content.is_empty() {
            return None;
        }
        
        // Create fenced code block with the language
        let fence = match &language {
            Some(lang) => format!("```{}", lang),
            None => "```".to_string(),
        };
        
        Some(HandlerResult::from(format!(
            "\n\n{}\n{}\n```\n\n",
            fence, content
        )))
    }
}

/// Handle `<code>` elements - inline code or code block content
fn code_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Check if inside a <pre> - if so, extract content with language detection
    let is_in_pre = is_inside_pre(element.node);

    if is_in_pre {
        // ✅ CORRECT: Use raw text extraction to preserve whitespace and angle brackets
        // DO NOT use handlers.walk_children() - it collapses whitespace!
        let raw_content = extract_raw_text(element.node);
        let raw_content = raw_content.trim();
        
        // Infer language BEFORE normalization so we can pass it
        let language = get_language_from_element(&element)
            .or_else(|| infer_language_from_content(raw_content));
        
        // Use raw content directly - no whitespace manipulation
        let content = raw_content;
        
        // Skip empty code blocks - prevents orphaned fence markers
        if content.is_empty() {
            return None;
        }
        
        // Re-validate HTML language hint against content (keep existing logic)
        let language = language.filter(|lang| validate_html_language(lang, content))
            .or_else(|| infer_language_from_content(content));

        let fence = match &language {
            Some(lang) => format!("```{}", lang),
            None => "```".to_string(),
        };

        // Return fenced code for pre_handler to wrap
        Some(HandlerResult::from(format!("{}\n{}\n```", fence, content)))
    } else {
        // ✅ CORRECT: Inline code also uses extract_raw_text() for whitespace preservation
        let raw_content = extract_raw_text(element.node);
        let raw_content = raw_content.trim();
        
        // Use raw content directly - no whitespace manipulation
        let content = raw_content;
        
        // Skip empty inline code - prevents empty backtick pairs
        if content.is_empty() {
            return None;
        }

        // Handle backticks in content (keep existing logic)
        if content.contains('`') {
            if content.starts_with('`') {
                Some(HandlerResult::from(format!("`` {} ``", content)))
            } else {
                Some(HandlerResult::from(format!("``{}``", content)))
            }
        } else {
            Some(HandlerResult::from(format!("`{}`", content)))
        }
    }
}

/// Handle `<a>` elements with fallback text extraction and validation
fn link_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Get href
    let href = get_attr(element.attrs, "href").unwrap_or_default();
    
    // Skip links with completely empty or meaningless hrefs
    if href.is_empty() || href == "#" || href == "javascript:void(0)" {
        // Return just the text content without the link wrapper
        // Use aggressive text extraction to ensure we get ALL text from nested elements
        let text = extract_raw_text(element.node);
        let text = text.trim();
        if !text.is_empty() {
            return Some(HandlerResult::from(text.to_string()));
        } else {
            return Some(HandlerResult::from(String::new()));
        }
    }

    // Extract link text with fallback hierarchy
    // Use aggressive text extraction instead of walk_children to ensure
    // we get text from nested inline elements like <span>, <kbd>, etc.
    let text = extract_raw_text(element.node);
    let text = text.trim();

    // DIAGNOSTIC: Log what we extracted for debugging missing link text issues
    tracing::debug!(
        "link_handler: href={:?}, extracted_text={:?}, is_meaningful={}",
        href,
        text,
        is_meaningful_link_text(text)
    );

    // Determine final link text with robust fallback chain
    let link_text = if is_meaningful_link_text(text) {
        // Text is meaningful, use it
        text.to_string()
    } else {
        // Text is empty or meaningless, try attribute fallbacks
        get_attr(element.attrs, "aria-label")
            .filter(|s| is_meaningful_link_text(s))
            .or_else(|| get_attr(element.attrs, "title").filter(|s| is_meaningful_link_text(s)))
            .or_else(|| get_attr(element.attrs, "alt").filter(|s| is_meaningful_link_text(s)))
            .or_else(|| {
                // Try cleaned URL for display
                let cleaned = clean_url_for_display(&href);
                if is_meaningful_link_text(&cleaned) {
                    Some(cleaned)
                } else {
                    // ✅ ULTIMATE FALLBACK: Use raw href as link text
                    // Better to have [/api/v1/users](/api/v1/users) than lose the link
                    None
                }
            })
            .unwrap_or_else(|| href.clone())  // ← Use href itself as last resort
    };

    // ✅ REMOVED: Final validation that returns empty string
    // Philosophy: ALWAYS emit a link structure for valid hrefs
    // It's better to have redundant [url](url) than silently lose links

    // Get optional title for markdown link (only if different from link text)
    let title = get_attr(element.attrs, "title");

    // Build markdown link
    let result = if let Some(title_text) = title {
        if !title_text.is_empty() && title_text != link_text {
            format!("[{}]({} \"{}\")", link_text, href, title_text)
        } else {
            format!("[{}]({})", link_text, href)
        }
    } else {
        format!("[{}]({})", link_text, href)
    };

    Some(HandlerResult::from(result))
}

/// Handle `<video>` elements - convert to markdown link with video indicator
fn video_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Try to get video source URL
    // Priority: 1. Direct src attribute, 2. First <source> child element
    let src = get_attr(element.attrs, "src")
        .or_else(|| find_source_url(element.node))
        .filter(|s| !s.is_empty())?;
    
    // Extract poster image if present
    let poster = get_attr(element.attrs, "poster");
    
    // Create display name from URL
    let filename = extract_filename_from_url(&src);
    
    // Build markdown output
    let mut result = format!("[Video: {}]({})", filename, src);
    
    // If there's a poster image, include it as well
    if let Some(poster_url) = poster
        && !poster_url.is_empty()
    {
        result.push_str(&format!("\n\n![Video thumbnail]({})", poster_url));
    }
    
    Some(HandlerResult::from(result))
}

/// Handle `<audio>` elements - convert to markdown link with audio indicator
fn audio_handler(_handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Try to get audio source URL
    // Priority: 1. Direct src attribute, 2. First <source> child element
    let src = get_attr(element.attrs, "src")
        .or_else(|| find_source_url(element.node))
        .filter(|s| !s.is_empty())?;
    
    // Create display name from URL
    let filename = extract_filename_from_url(&src);
    
    // Build markdown output
    let result = format!("[Audio: {}]({})", filename, src);
    
    Some(HandlerResult::from(result))
}

/// Handle generic inline elements by walking children to preserve nested formatting
/// This ensures text is never lost AND nested elements (like links) are processed
fn inline_element_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Use handler pipeline to properly process nested elements (including links)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    if content.is_empty() {
        return None;
    }
    
    // Return the content as-is (no markdown formatting)
    // The parent element's handler will apply formatting if needed
    Some(HandlerResult::from(content.to_string()))
}

/// Handle `<span>` elements - detect inline styles and apply markdown formatting
/// Uses walk_children to preserve nested elements like links
fn span_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Use handler pipeline to properly process nested elements (including links)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    // If no content, return nothing
    if content.is_empty() {
        return None;
    }
    
    // Check for style attribute
    let style = get_attr(element.attrs, "style");
    
    if let Some(style_str) = style {
        let (is_bold, is_italic) = parse_style_formatting(&style_str);
        
        // Apply markdown formatting based on detected styles
        let formatted = match (is_bold, is_italic) {
            (true, true) => {
                // Both bold and italic: ***text***
                format!("***{}***", content)
            }
            (true, false) => {
                // Bold only: **text**
                format!("**{}**", content)
            }
            (false, true) => {
                // Italic only: *text*
                format!("*{}*", content)
            }
            (false, false) => {
                // No recognized formatting, return plain content
                // This preserves color, background, and other non-markdown styles as plain text
                content.to_string()
            }
        };
        
        return Some(HandlerResult::from(formatted));
    }
    
    // No style attribute, return plain content
    Some(HandlerResult::from(content.to_string()))
}

/// Handle <strong> and <b> tags - bold text
/// Uses walk_children to preserve nested elements like links
fn strong_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Use handler pipeline to properly process nested elements (including links)
    let content = handlers.walk_children(element.node).content;
    
    if content.is_empty() {
        return None;
    }
    
    // Calculate whitespace bounds to preserve spacing outside markers
    // This matches htmd's emphasis_handler architecture
    let leading_space = content.len() - content.trim_start().len();
    let trailing_space = content.len() - content.trim_end().len();
    let trimmed = content.trim();
    
    if trimmed.is_empty() {
        return None;
    }
    
    // Reconstruct: leading_ws + ** + content + ** + trailing_ws
    // Result: ` **content** ` (spaces OUTSIDE markers, not inside)
    let result = format!(
        "{}**{}**{}",
        &content[..leading_space],
        trimmed,
        &content[content.len() - trailing_space..]
    );
    
    Some(HandlerResult::from(result))
}

/// Handle <em> and <i> tags - italic text
/// Uses walk_children to preserve nested elements like links
fn em_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Use handler pipeline to properly process nested elements (including links)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    if content.is_empty() {
        return None;
    }
    
    Some(HandlerResult::from(format!("*{}*", content)))
}

/// Handle `<li>` elements - list items with deep content extraction
///
/// Ensures all nested content (divs, spans, etc.) is properly extracted
/// by using walk_children to recursively traverse the entire element tree.
fn li_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Walk all children to extract nested content (divs, spans, etc.)
    let result = handlers.walk_children(element.node);
    let content = result.content.trim();
    
    // Return content with proper spacing
    // The list marker (number or bullet) will be added by the parent ol/ul handler
    if !content.is_empty() {
        Some(HandlerResult::from(format!("{}\n", content)))
    } else {
        // Empty list item - preserve it
        Some(HandlerResult::from("\n".to_string()))
    }
}

/// Indent nested blocks within list items for proper markdown rendering
///
/// Takes multi-line content and indents all lines after the first with 3 spaces.
/// This ensures code blocks, paragraphs, and other nested content render correctly
/// within numbered and bulleted list items.
#[allow(dead_code)]
fn indent_nested_blocks(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    
    if lines.len() <= 1 {
        return content.to_string();
    }
    
    let mut result = String::new();
    result.push_str(lines[0]);
    result.push('\n');
    
    for line in &lines[1..] {
        if !line.is_empty() {
            result.push_str("   "); // 3 spaces
        }
        result.push_str(line);
        result.push('\n');
    }
    
    result.trim_end().to_string()
}

/// Handle `<ol>` elements - ordered lists with nested depth tracking
///
/// Delegates to list_processing module which manually traverses DOM
/// to preserve nesting hierarchy that walk_children() would flatten.
fn ol_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let output = list_processing::process_list(handlers, element.node, true);
    if output.is_empty() {
        return None;
    }
    Some(HandlerResult::from(format!("\n\n{}\n\n", output)))
}

/// Handle `<ul>` elements - unordered lists with nested depth tracking
///
/// Delegates to list_processing module which manually traverses DOM
/// to preserve nesting hierarchy that walk_children() would flatten.
fn ul_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let output = list_processing::process_list(handlers, element.node, false);
    if output.is_empty() {
        return None;
    }
    Some(HandlerResult::from(format!("\n\n{}\n\n", output)))
}

// === Table Handlers ===

/// Handle `<table>` elements - construct proper markdown tables
///
/// This handler walks the table structure to extract:
/// - Header row from `<thead>` (if present)
/// - Data rows from `<tbody>` (or direct `<tr>` children)
/// - Generates proper markdown table format with separator row
fn table_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    use markup5ever_rcdom::NodeData;
    
    let mut header_row: Option<Vec<String>> = None;
    let mut data_rows: Vec<Vec<String>> = Vec::new();
    
    // Walk through table children to find thead and tbody
    for child in element.node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data {
            match &*name.local {
                "thead" => {
                    // Extract header row from thead
                    header_row = extract_table_header(handlers, child);
                }
                "tbody" => {
                    // Extract data rows from tbody
                    data_rows.extend(extract_table_rows(handlers, child));
                }
                "tr" => {
                    // Direct <tr> child (table without tbody)
                    if let Some(row) = extract_single_row(handlers, child) {
                        // If no header yet and this looks like a header row, use it
                        if header_row.is_none() && is_header_row(child) {
                            header_row = Some(row);
                        } else {
                            data_rows.push(row);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    
    // If we have no rows at all, skip the table
    if data_rows.is_empty() && header_row.is_none() {
        return None;
    }
    
    // Build markdown table
    let mut output = String::from("\n\n");
    
    // Write header row (or create a generic one if missing)
    let headers = header_row.unwrap_or_else(|| {
        // Generate generic headers based on column count
        let col_count = data_rows.first().map(|r| r.len()).unwrap_or(2);
        (1..=col_count).map(|i| format!("Column {}", i)).collect()
    });
    
    if !headers.is_empty() {
        output.push_str("| ");
        output.push_str(&headers.join(" | "));
        output.push_str(" |\n");
        
        // Write separator row
        output.push('|');
        for _ in 0..headers.len() {
            output.push_str("---|");
        }
        output.push('\n');
    }
    
    // Write data rows
    for row in data_rows {
        if !row.is_empty() {
            output.push_str("| ");
            output.push_str(&row.join(" | "));
            output.push_str(" |\n");
        }
    }
    
    output.push('\n');
    
    Some(HandlerResult::from(output))
}

/// Extract header row from `<thead>` element
fn extract_table_header(handlers: &dyn Handlers, thead_node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Option<Vec<String>> {
    use markup5ever_rcdom::NodeData;
    
    // Find the <tr> inside thead
    for child in thead_node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && &*name.local == "tr"
        {
            return extract_header_row_cells(handlers, child);
        }
    }
    None
}

/// Extract header cells from a `<tr>` containing `<th>` elements
fn extract_header_row_cells(handlers: &dyn Handlers, tr_node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Option<Vec<String>> {
    use markup5ever_rcdom::NodeData;
    
    let mut cells = Vec::new();
    
    for child in tr_node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && &*name.local == "th"
        {
            // Extract text from <th> using handlers to process nested elements
            let result = handlers.walk_children(child);
            let text = result.content.trim().to_string();
            cells.push(text);
        }
    }
    
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

/// Extract data rows from `<tbody>` element
fn extract_table_rows(handlers: &dyn Handlers, tbody_node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Vec<Vec<String>> {
    use markup5ever_rcdom::NodeData;
    
    let mut rows = Vec::new();
    
    for child in tbody_node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && &*name.local == "tr"
            && let Some(row) = extract_single_row(handlers, child)
        {
            rows.push(row);
        }
    }
    
    rows
}

/// Extract cells from a single `<tr>` element
fn extract_single_row(handlers: &dyn Handlers, tr_node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Option<Vec<String>> {
    use markup5ever_rcdom::NodeData;
    
    let mut cells = Vec::new();
    
    for child in tr_node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && (&*name.local == "td" || &*name.local == "th")
        {
            // Extract text from cell using handlers to process nested elements
            let result = handlers.walk_children(child);
            let text = result.content.trim().to_string();
            cells.push(text);
        }
    }
    
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

/// Check if a `<tr>` element contains `<th>` cells (making it a header row)
fn is_header_row(tr_node: &std::rc::Rc<markup5ever_rcdom::Node>) -> bool {
    use markup5ever_rcdom::NodeData;
    
    for child in tr_node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && &*name.local == "th"
        {
            return true;
        }
    }
    false
}

/// Handle `<thead>` elements - suppress default output since table_handler processes it
fn thead_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    // Return empty to suppress default handling
    // The table_handler will process thead content
    Some(HandlerResult::from(String::new()))
}

/// Handle `<tbody>` elements - suppress default output since table_handler processes it
fn tbody_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    // Return empty to suppress default handling
    // The table_handler will process tbody content
    Some(HandlerResult::from(String::new()))
}

/// Handle `<tr>` elements - suppress default output since table_handler processes it
fn tr_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    // Return empty to suppress default handling
    // The table_handler will process tr content
    Some(HandlerResult::from(String::new()))
}

/// Handle `<th>` elements - suppress default output since table_handler processes it
fn th_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    // Return empty to suppress default handling
    // The table_handler will process th content
    Some(HandlerResult::from(String::new()))
}

/// Handle `<td>` elements - suppress default output since table_handler processes it
fn td_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    // Return empty to suppress default handling
    // The table_handler will process td content
    Some(HandlerResult::from(String::new()))
}

// === Helper Functions ===

/// Extract raw text content from a node tree, preserving all whitespace
/// and adding intelligent spacing between inline elements
fn extract_raw_text(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> String {
    use markup5ever_rcdom::NodeData;

    let mut text = String::new();

    match &node.data {
        NodeData::Text { contents } => {
            let content = contents.borrow();
            // ✅ ADD DEBUG LOGGING
            if !content.is_empty() {
                tracing::trace!("extract_raw_text: Found text node: {:?}", &content[..content.len().min(50)]);
            }
            // Preserve text exactly as-is (including angle brackets from decoded entities)
            text.push_str(&content);
        }
        NodeData::Element { name, .. } => {
            // ✅ ADD DEBUG LOGGING for element traversal
            tracing::trace!("extract_raw_text: Processing element: {}", &name.local);
            
            // Recursively process all children with intelligent spacing
            for (i, child) in node.children.borrow().iter().enumerate() {
                let child_text = extract_raw_text(child);
                
                // Add space between sibling inline elements if needed
                if i > 0 && !child_text.is_empty() && !text.is_empty() {
                    // Check if we need to add space between elements
                    let last_char = text.chars().last();
                    let first_char = child_text.chars().next();
                    
                    // Add space if:
                    // - Previous text doesn't end with whitespace
                    // - Current text doesn't start with whitespace
                    // - NOT transitioning between adjacent punctuation
                    if let (Some(last), Some(first)) = (last_char, first_char) {
                        let needs_space = 
                            !last.is_whitespace() 
                            && !first.is_whitespace()
                            && !is_punctuation_pair(last, first);
                        
                        if needs_space {
                            text.push(' ');
                        }
                    }
                }
                
                text.push_str(&child_text);
            }
        }
        NodeData::Document | NodeData::Doctype { .. } => {
            // Recursively process all children with intelligent spacing
            for (i, child) in node.children.borrow().iter().enumerate() {
                let child_text = extract_raw_text(child);
                
                // Add space between sibling inline elements if needed
                if i > 0 && !child_text.is_empty() && !text.is_empty() {
                    // Check if we need to add space between elements
                    let last_char = text.chars().last();
                    let first_char = child_text.chars().next();
                    
                    // Add space if:
                    // - Previous text doesn't end with whitespace
                    // - Current text doesn't start with whitespace
                    // - NOT transitioning between adjacent punctuation
                    if let (Some(last), Some(first)) = (last_char, first_char) {
                        let needs_space = 
                            !last.is_whitespace() 
                            && !first.is_whitespace()
                            && !is_punctuation_pair(last, first);
                        
                        if needs_space {
                            text.push(' ');
                        }
                    }
                }
                
                text.push_str(&child_text);
            }
        }
        NodeData::Comment { .. } | NodeData::ProcessingInstruction { .. } => {
            // Skip comments and processing instructions
        }
    }

    text
}

/// Check if two characters form a punctuation pair that shouldn't have space between them
#[inline]
fn is_punctuation_pair(prev: char, next: char) -> bool {
    // Don't add space in these cases:
    match (prev, next) {
        // Path separators
        ('/', _) | (_, '/') => true,
        // (':', _) | (_, ':') => true,  // ❌ REMOVED - Breaks YAML/JSON/TOML spacing
        // Brackets and parens
        ('(', _) | (_, ')') => true,
        ('[', _) | (_, ']') => true,
        ('{', _) | (_, '}') => true,
        // Quotes
        ('"', _) | (_, '"') => true,
        ('\'', _) | (_, '\'') => true,
        // Operators that should be adjacent
        ('=', _) | (_, '=') => true,
        ('.', _) | (_, '.') => true,
        (',', _) => true,  // Comma followed by anything
        // Pipe operator (but we want space around it in shell commands)
        // ('|', _) | (_, '|') => true,  // ❌ DON'T exclude pipes
        // Dash with alphanumeric or dash (handles -fsSL, --release, build-prod, -5)
        ('-', c) | (c, '-') if c.is_alphanumeric() || c == '-' => true,
        _ => false,
    }
}

/// Check if a node is inside a <pre> element
fn is_inside_pre(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> bool {
    use markup5ever_rcdom::NodeData;

    // Safe non-destructive read: take() + replace()
    let mut current = node.parent.take();
    node.parent.set(current.clone());

    while let Some(weak_parent) = current {
        if let Some(parent) = weak_parent.upgrade() {
            if let NodeData::Element { name, .. } = &parent.data
                && &*name.local == "pre"
            {
                return true;
            }
            // Safe non-destructive read: take() + replace()
            current = parent.parent.take();
            parent.parent.set(current.clone());
        } else {
            break;
        }
    }
    false
}

/// Find the first `<source>` element's src attribute in child nodes
fn find_source_url(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Option<String> {
    use markup5ever_rcdom::NodeData;

    // Check direct children only (not recursive - source elements are direct children)
    for child in node.children.borrow().iter() {
        if let NodeData::Element { name, attrs, .. } = &child.data {
            // Check if this is a <source> element
            if &*name.local == "source" {
                // Extract src attribute
                for attr in attrs.borrow().iter() {
                    if &*attr.name.local == "src" {
                        let src = attr.value.to_string().trim().to_string();
                        if !src.is_empty() {
                            return Some(src);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract a human-readable filename from a URL for display
fn extract_filename_from_url(url: &str) -> String {
    // Split by / and get the last segment
    let path = url.split('/').next_back().unwrap_or(url);
    
    // Remove query parameters and fragments
    let filename = path
        .split('?').next().unwrap_or(path)
        .split('#').next().unwrap_or(path);
    
    // URL decode if needed (e.g., %20 -> space)
    let decoded = urlencoding::decode(filename).unwrap_or(std::borrow::Cow::Borrowed(filename));
    
    // If we got a meaningful filename, use it
    if !decoded.is_empty() && decoded != "/" {
        decoded.to_string()
    } else {
        "media file".to_string()
    }
}

/// Get language from element attributes (class or data-language)
/// Also checks parent element for language class (common HTML pattern)
fn get_language_from_element(element: &Element) -> Option<String> {
    // Try data-language attribute first (highest priority)
    if let Some(lang) = get_attr(element.attrs, "data-language")
        && !lang.is_empty()
    {
        return Some(lang);
    }

    // Try class attribute on this element
    if let Some(class) = get_attr(element.attrs, "class")
        && let Some(lang) = extract_language_from_class(&class)
    {
        return Some(lang);
    }
    
    // Check parent element (for <pre class="language-X"><code>)
    if let Some(lang) = get_language_from_parent(element.node) {
        return Some(lang);
    }

    None
}

/// Extract language from parent element's class attribute
/// Handles common pattern: <pre class="language-python"><code>...</code></pre>
fn get_language_from_parent(node: &std::rc::Rc<markup5ever_rcdom::Node>) -> Option<String> {
    use markup5ever_rcdom::NodeData;
    
    // Safe non-destructive read: take() + replace()
    let weak_parent = node.parent.take();
    node.parent.set(weak_parent.clone());
    let weak_parent = weak_parent?;
    let parent_rc = weak_parent.upgrade()?;
    
    if let NodeData::Element { attrs, .. } = &parent_rc.data {
        // Check parent's class attribute
        for attr in attrs.borrow().iter() {
            if &*attr.name.local == "class" {
                let class_value = attr.value.to_string();
                if let Some(lang) = extract_language_from_class(&class_value) {
                    return Some(lang);
                }
            }
        }
        
        // Check parent's data-language attribute
        for attr in attrs.borrow().iter() {
            if &*attr.name.local == "data-language" {
                let lang = attr.value.to_string().trim().to_string();
                if !lang.is_empty() {
                    return Some(lang);
                }
            }
        }
    }
    
    None
}

/// Get attribute value from element
fn get_attr(attrs: &[html5ever::Attribute], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| a.value.to_string())
        .filter(|v| !v.trim().is_empty())
}

/// Parse inline CSS style attribute and detect bold/italic formatting
/// 
/// Returns (is_bold, is_italic) tuple based on CSS properties:
/// - font-weight: bold, bolder, or numeric >= 600
/// - font-style: italic or oblique
fn parse_style_formatting(style: &str) -> (bool, bool) {
    let mut is_bold = false;
    let mut is_italic = false;
    
    // Parse semicolon-separated CSS properties
    for property in style.split(';') {
        let parts: Vec<&str> = property.split(':').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            continue;
        }
        
        let (key, value) = (parts[0].to_lowercase(), parts[1].to_lowercase());
        
        match key.as_str() {
            "font-weight" => {
                // Check for bold keyword values
                if matches!(value.as_str(), "bold" | "bolder") {
                    is_bold = true;
                } else if let Ok(weight) = value.parse::<u16>() {
                    // Numeric font-weight: 600+ is bold
                    is_bold = weight >= 600;
                }
            }
            "font-style" => {
                // Check for italic values
                is_italic = matches!(value.as_str(), "italic" | "oblique");
            }
            _ => {}
        }
    }
    
    (is_bold, is_italic)
}

/// Check if text is meaningful for a link (not just whitespace/punctuation)
fn is_meaningful_link_text(text: &str) -> bool {
    let trimmed = text.trim();
    
    // Empty is not meaningful
    if trimmed.is_empty() {
        return false;
    }
    
    // Single character that's only punctuation is not meaningful
    if trimmed.len() == 1
        && let Some(ch) = trimmed.chars().next()
        && ch.is_ascii_punctuation()
    {
        return false;
    }
    
    // Check if it contains at least one alphanumeric character
    // This filters out pure punctuation like ".", "...", "---", etc.
    trimmed.chars().any(|c| c.is_alphanumeric())
}

/// Clean URL for display when used as link text fallback
fn clean_url_for_display(url: &str) -> String {
    // Remove leading slash for relative URLs
    let cleaned = url.trim_start_matches('/');

    // Remove query parameters and fragments
    let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
    let cleaned = cleaned.split('#').next().unwrap_or(cleaned);

    // Remove file extensions for cleaner display
    let cleaned = cleaned.trim_end_matches(".html");
    let cleaned = cleaned.trim_end_matches(".htm");
    let cleaned = cleaned.trim_end_matches("/index");

    // Replace hyphens/underscores with spaces
    let cleaned = cleaned.replace(['-', '_'], " ");

    // If cleaned is empty or just whitespace, return a sensible fallback
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return "link".to_string();  // Generic but meaningful fallback
    }

    // Title case the first word
    if let Some(first_char) = cleaned.chars().next() {
        format!(
            "{}{}",
            first_char.to_uppercase(),
            &cleaned[first_char.len_utf8()..]
        )
    } else {
        "link".to_string()  // Should never reach here, but safety fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_url_for_display() {
        assert_eq!(clean_url_for_display("/guide"), "Guide");
        assert_eq!(
            clean_url_for_display("/installation-guide"),
            "Installation guide"
        );
        assert_eq!(
            clean_url_for_display("/docs/api.html"),
            "Docs/api"
        );
    }

    #[test]
    fn test_code_block_basic() {
        let converter = create_converter();
        let html = r#"<pre><code>fn main() {
    println!("Hello");
}</code></pre>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("```rust"), "Should detect Rust language");
        assert!(md.contains("fn main()"), "Should preserve code content");
        assert!(md.contains("```"), "Should have closing fence");
    }

    #[test]
    fn test_code_block_with_language_class() {
        let converter = create_converter();
        let html = r#"<pre class="language-python"><code>def hello():
    print("world")</code></pre>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("```python"), "Should use HTML language hint");
    }

    #[test]
    fn test_inline_code() {
        let converter = create_converter();
        let html = r#"<p>Use the <code>println!</code> macro</p>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("`println!`"), "Should wrap inline code with backticks");
    }

    #[test]
    fn test_link_with_text() {
        let converter = create_converter();
        let html = r#"<a href="/guide">Installation Guide</a>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_with_aria_label_fallback() {
        let converter = create_converter();
        let html = r#"<a href="/guide" aria-label="Installation Guide"></a>"#;
        let md = converter.convert(html).unwrap();

        assert!(md.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_href_fallback() {
        let converter = create_converter();
        let html = r#"<a href="/installation-guide"></a>"#;
        let md = converter.convert(html).unwrap();

        // Should clean up href for display
        assert!(md.contains("[Installation guide](/installation-guide)"));
    }

    #[test]
    fn test_code_block_with_angle_brackets() {
        let converter = create_converter();
        // Test HTML-escaped angle brackets that should be preserved in code
        let html = r#"<pre><code>"&lt;Left&gt;".blue().bold()</code></pre>"#;
        let md = converter.convert(html).unwrap();

        // Should preserve <Left> after HTML entity decoding
        assert!(
            md.contains("<Left>"),
            "Should preserve angle brackets in code. Got: {}",
            md
        );
    }

    #[test]
    fn test_inline_code_with_angle_brackets() {
        let converter = create_converter();
        let html = r#"<p>Press <code>&lt;Left&gt;</code> to go back</p>"#;
        let md = converter.convert(html).unwrap();

        // Should preserve <Left> in inline code
        assert!(
            md.contains("`<Left>`"),
            "Should preserve angle brackets in inline code. Got: {}",
            md
        );
    }

    #[test]
    fn test_list_with_bold() {
        let converter = create_converter();
        let html = r#"<ul><li><strong>Homebrew (macOS, Linux):</strong> Install via brew</li></ul>"#;
        let md = converter.convert(html).unwrap();

        // Should have proper bold formatting in list
        assert!(
            md.contains("**Homebrew (macOS, Linux):**"),
            "Should have proper bold in list. Got: {}",
            md
        );
    }

    #[test]
    fn test_span_with_bold() {
        let converter = create_converter();
        // This is the pattern from Claude Code docs: <span data-as="p"><strong>text:</strong></span>
        let html = r#"<span data-as="p"><strong>Homebrew (macOS, Linux):</strong></span>"#;
        let md = converter.convert(html).unwrap();

        // Should have proper bold formatting
        assert!(
            md.contains("**Homebrew (macOS, Linux):**"),
            "Should have proper bold. Got: '{}'",
            md
        );
    }

    #[test]
    fn test_link_with_empty_text() {
        let converter = create_converter();
        let html = r#"<a href="/guide"></a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should use href fallback
        assert!(md.contains("[Guide](/guide)"), "Empty link should use href fallback. Got: {}", md);
    }

    #[test]
    fn test_link_with_only_period() {
        let converter = create_converter();
        let html = r#"<a href="/troubleshooting">.</a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should NOT output [.](url), should use fallback
        assert!(!md.contains("[.]"), "Should not output period as link text");
        assert!(md.contains("[Troubleshooting](/troubleshooting)"), "Should use href fallback. Got: {}", md);
    }

    #[test]
    fn test_link_with_only_whitespace() {
        let converter = create_converter();
        let html = r#"<a href="/docs">   </a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should use href fallback, not empty text
        assert!(md.contains("[Docs](/docs)"), "Whitespace-only link should use href fallback. Got: {}", md);
    }

    #[test]
    fn test_link_with_empty_href() {
        let converter = create_converter();
        let html = r#"<a href="">Click here</a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should return just the text, not a link
        assert!(!md.contains("[Click here]"), "Empty href should not create link");
        assert!(md.contains("Click here"), "Should preserve text content");
    }

    #[test]
    fn test_link_with_hash_only() {
        let converter = create_converter();
        let html = r##"<a href="#">Section link</a>"##;
        let md = converter.convert(html).unwrap();
        
        // Should return just the text for # links (page anchors without context)
        assert!(md.contains("Section link"), "Should preserve text");
    }

    #[test]
    fn test_link_with_nested_span() {
        let converter = create_converter();
        let html = r#"<a href="/guide"><span>Installation Guide</span></a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should extract text from nested span
        assert!(md.contains("[Installation Guide](/guide)"), "Should extract nested span text. Got: {}", md);
    }

    #[test]
    fn test_link_with_aria_label_meaningful() {
        let converter = create_converter();
        let html = r#"<a href="/help" aria-label="Get Help">.</a>"#;
        let md = converter.convert(html).unwrap();
        
        // Should use aria-label instead of period
        assert!(md.contains("[Get Help](/help)"), "Should use aria-label over meaningless text. Got: {}", md);
    }

    #[test]
    fn test_is_meaningful_link_text_helper() {
        // Meaningful
        assert!(is_meaningful_link_text("Guide"));
        assert!(is_meaningful_link_text("Installation guide"));
        assert!(is_meaningful_link_text("API"));
        assert!(is_meaningful_link_text("v1.0"));
        
        // Not meaningful
        assert!(!is_meaningful_link_text(""));
        assert!(!is_meaningful_link_text("   "));
        assert!(!is_meaningful_link_text("."));
        assert!(!is_meaningful_link_text(","));
        assert!(!is_meaningful_link_text("!"));
        assert!(!is_meaningful_link_text("..."));
        assert!(!is_meaningful_link_text("---"));
        
        // Edge cases
        assert!(is_meaningful_link_text("1"));  // Single digit is meaningful
        assert!(is_meaningful_link_text("a"));  // Single letter is meaningful
        assert!(is_meaningful_link_text("v.1.0"));  // Has alphanumeric
    }

    #[test]
    fn test_inline_span_preserves_text() {
        let converter = create_converter();
        let html = r#"<p>There was <span>a breaking change</span> in the code</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("a breaking change"),
            "Span text should be preserved. Got: {}",
            md
        );
    }

    #[test]
    fn test_nested_inline_elements() {
        let converter = create_converter();
        let html = r#"<p>Click <a href="/docs"><span class="icon">here</span></a> to continue</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("[here](/docs)"),
            "Nested span in link should preserve text. Got: {}",
            md
        );
    }

    #[test]
    fn test_kbd_element() {
        let converter = create_converter();
        let html = r#"<p>Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("Ctrl") && md.contains("C"),
            "Kbd elements should preserve text. Got: {}",
            md
        );
    }

    #[test]
    fn test_multiple_inline_elements() {
        let converter = create_converter();
        let html = r#"<p><strong>Software</strong>: <span>Node.js 18+</span> (only required for npm installation)</p>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("**Software**") && md.contains("Node.js 18+"),
            "Multiple inline elements should preserve all text. Got: {}",
            md
        );
    }

    #[test]
    fn test_empty_inline_elements() {
        let converter = create_converter();
        let html = r#"<p>Text <span></span> more text</p>"#;
        let md = converter.convert(html).unwrap();
        
        // Should not crash, empty span should be skipped
        assert!(md.contains("Text") && md.contains("more text"));
    }

    #[test]
    fn test_real_world_missing_words_patterns() {
        let converter = create_converter();
        
        // Pattern 1: Link with styled text
        let html1 = r#"<p>There was <a href="/change"><span>a breaking change</span></a> in version 0.1.14</p>"#;
        let md1 = converter.convert(html1).unwrap();
        assert!(md1.contains("a breaking change"));
        
        // Pattern 2: Bold label with inline value
        let html2 = r#"<p><strong>Software</strong>: <code>Node.js 18+</code> (only required...)</p>"#;
        let md2 = converter.convert(html2).unwrap();
        assert!(md2.contains("Node.js 18+"));
        
        // Pattern 3: Text with inline code
        let html3 = r#"<p>If you have <kbd>nvm</kbd> installed:</p>"#;
        let md3 = converter.convert(html3).unwrap();
        assert!(md3.contains("nvm"));
    }

    // ===== Link Preservation Tests =====
    // These tests verify that links inside formatting elements are preserved
    // after fixing handlers to use walk_children() instead of extract_raw_text()

    #[test]
    fn test_link_inside_div_preserved() {
        let converter = create_converter();
        let html = r#"<div><a href="/tokio">tokio</a></div>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("[tokio](/tokio)"),
            "Link inside div should be preserved as markdown link. Got: {}",
            md
        );
    }

    #[test]
    fn test_link_inside_span_preserved() {
        let converter = create_converter();
        let html = r#"<span><a href="/tokio">tokio</a></span>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("[tokio](/tokio)"),
            "Link inside span should be preserved as markdown link. Got: {}",
            md
        );
    }

    #[test]
    fn test_link_inside_strong_preserved() {
        let converter = create_converter();
        let html = r#"<strong><a href="/tokio">tokio</a></strong>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("[tokio](/tokio)"),
            "Link inside strong should be preserved as markdown link. Got: {}",
            md
        );
        // Bold wrapping is expected
        assert!(
            md.contains("**"),
            "Strong element should add bold markers. Got: {}",
            md
        );
    }

    #[test]
    fn test_link_inside_em_preserved() {
        let converter = create_converter();
        let html = r#"<em><a href="/tokio">tokio</a></em>"#;
        let md = converter.convert(html).unwrap();
        
        assert!(
            md.contains("[tokio](/tokio)"),
            "Link inside em should be preserved as markdown link. Got: {}",
            md
        );
        // Italic wrapping is expected
        assert!(
            md.contains("*"),
            "Em element should add italic markers. Got: {}",
            md
        );
    }

    #[test]
    fn test_github_trending_structure() {
        let converter = create_converter();
        // Simulated GitHub trending repo structure with nested links
        let html = r#"<article>
            <h2><a href="/rust-lang/rust">rust-lang / rust</a></h2>
            <p>Empowering everyone to build reliable software.</p>
            <div class="topics">
                <a href="/topics/rust">rust</a>
                <a href="/topics/compiler">compiler</a>
            </div>
            <span class="stars"><a href="/rust-lang/rust/stargazers">98,000</a> stars</span>
        </article>"#;
        let md = converter.convert(html).unwrap();
        
        // All links should be preserved
        assert!(
            md.contains("[rust-lang / rust](/rust-lang/rust)"),
            "Main repo link should be preserved. Got: {}",
            md
        );
        assert!(
            md.contains("[rust](/topics/rust)"),
            "Topic link 'rust' should be preserved. Got: {}",
            md
        );
        assert!(
            md.contains("[compiler](/topics/compiler)"),
            "Topic link 'compiler' should be preserved. Got: {}",
            md
        );
        assert!(
            md.contains("[98,000](/rust-lang/rust/stargazers)"),
            "Stars link should be preserved. Got: {}",
            md
        );
    }
}
#[test]
fn test_paragraph_heading_separation() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    
    // Test from task: Paragraph -> Heading -> Paragraph
    let html = r#"<p>rust TUIs.</p><h2>Why Ratatui?</h2><p>Ratatui is designed for developers...</p>"#;
    let md = converter.convert(html).unwrap();
    
    // Should have the heading marker
    assert!(md.contains("## Why Ratatui?"), "Should have heading with ##. Got: {}", md);
    
    // Should NOT have concatenated text (the original bug)
    assert!(!md.contains("TUIs.Why Ratatui?"), "Should not concatenate paragraph and heading. Got: {}", md);
    
    // Should have blank lines (multiple newlines)
    assert!(md.contains("\n\n"), "Should have blank lines between elements. Got: {}", md);
}

#[test]
fn test_multiple_paragraphs_separation() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    let html = r#"<p>First paragraph.</p><p>Second paragraph.</p>"#;
    let md = converter.convert(html).unwrap();
    
    // Should have both paragraphs
    assert!(md.contains("First paragraph"), "Should have first paragraph");
    assert!(md.contains("Second paragraph"), "Should have second paragraph");
    
    // Should have blank lines
    assert!(md.contains("\n\n"), "Should have blank lines between paragraphs. Got: {}", md);
}

#[test]
fn test_heading_with_nested_formatting() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    let html = r#"<h1>Main <strong>Title</strong> with Link to <a href="/guide">Guide</a></h1>"#;
    let md = converter.convert(html).unwrap();
    
    // Should be H1
    assert!(md.contains("# Main"), "Should have H1 marker. Got: {}", md);
    
    // Should preserve inline formatting
    assert!(md.contains("**Title**"), "Should preserve bold. Got: {}", md);
    assert!(md.contains("[Guide](/guide)"), "Should preserve link. Got: {}", md);
}

#[test]
fn test_empty_paragraphs_skipped() {
    use crate::content_saver::markdown_converter::custom_handlers::create_converter;
    
    let converter = create_converter();
    let html = r#"<p></p><h2>Valid Heading</h2><p>   </p><p>Valid paragraph</p>"#;
    let md = converter.convert(html).unwrap();
    
    // Should have valid content
    assert!(md.contains("## Valid Heading"), "Should have heading");
    assert!(md.contains("Valid paragraph"), "Should have paragraph");
    
    // Empty paragraphs should be skipped (no excessive blank lines)
    let newline_count = md.matches('\n').count();
    // Should have some newlines but not excessive
    assert!(newline_count < 15, "Should not have excessive newlines from empty elements. Got: {}", md);
}

#[test]
fn test_github_trending_exact_html() {
    let converter = create_converter();
    // Exact structure from GitHub trending page
    let html = r#"
    <article class="Box-row">
      <h2 class="h3 lh-condensed">
        <a href="/rustdesk/rustdesk" class="Link">
          <span class="text-normal">rustdesk /</span>
          rustdesk
        </a>
      </h2>
      <p class="col-9 color-fg-muted my-1 pr-4">
        An open-source remote desktop application
      </p>
    </article>
    "#;
    let md = converter.convert(html).unwrap();
    
    eprintln!("OUTPUT: {}", md);
    
    // The link MUST be preserved
    assert!(
        md.contains("[rustdesk /rustdesk](/rustdesk/rustdesk)") || 
        md.contains("[rustdesk / rustdesk](/rustdesk/rustdesk)") ||
        md.contains("(/rustdesk/rustdesk)"),
        "Link href must be preserved! Got: '{}'",
        md
    );
}

#[test]
fn test_numbered_list_with_code_blocks() {
    let converter = create_converter();
    
    let html = r#"<ol>
  <li>
    <p>For example:</p>
    <pre><code>claude mcp add --transport http sentry https://mcp.sentry.dev/mcp</code></pre>
  </li>
  <li>
    <p>In Claude Code, use the command:</p>
    <pre><code>> /mcp</code></pre>
  </li>
</ol>"#;
    
    let md = converter.convert(html).unwrap();
    
    // Should have proper numbered list markers
    assert!(md.contains("1. For example:"), "Should have '1. For example:'. Got: {}", md);
    assert!(md.contains("2. In Claude Code"), "Should have '2. In Claude Code'. Got: {}", md);
    
    // Code blocks should be indented with 3 spaces
    assert!(md.contains("   ```"), "Code blocks should be indented with 3 spaces. Got: {}", md);
    
    // Should NOT have orphaned numbers (numbers merged with text)
    assert!(!md.contains("1For"), "Should not have '1For'. Got: {}", md);
    assert!(!md.contains("2In"), "Should not have '2In'. Got: {}", md);
}

#[test]
fn test_nested_list_indentation() {
    let converter = create_converter();
    let html = r#"
        <div>
            <p>Tips:</p>
            <ul>
                <li>Use the <code>--scope</code> flag to specify where the configuration is stored:
                    <ul>
                        <li><code>local</code> (default): Available only to you in the current project</li>
                        <li><code>project</code>: Shared with everyone in the project</li>
                        <li><code>user</code>: Available to you across all projects</li>
                    </ul>
                </li>
                <li>Set environment variables with <code>--env</code> flags</li>
            </ul>
        </div>
    "#;
    
    let result = converter.convert(html).unwrap();
    
    println!("=== Nested List Output ===");
    for (i, line) in result.lines().enumerate() {
        println!("Line {}: {:?}", i, line);
    }
    println!("=== End ===");
    
    // Verify nested items have 2-space indentation
    let lines: Vec<&str> = result.lines().collect();
    
    // Parent item should have no indentation
    assert!(
        lines.iter().any(|l| l.starts_with("- Use the `--scope`")), 
        "Expected parent list item without indentation. Got:\n{}", result
    );
    
    // Nested items should have exactly 2-space indentation
    assert!(
        lines.iter().any(|l| l.starts_with("  - `local`")), 
        "Expected nested list item '  - `local`' with 2-space indent. Got:\n{}", result
    );
    assert!(
        lines.iter().any(|l| l.starts_with("  - `project`")), 
        "Expected nested list item '  - `project`' with 2-space indent. Got:\n{}", result
    );
    assert!(
        lines.iter().any(|l| l.starts_with("  - `user`")), 
        "Expected nested list item '  - `user`' with 2-space indent. Got:\n{}", result
    );
    
    // Second parent item should have no indentation
    assert!(
        lines.iter().any(|l| l.starts_with("- Set environment variables")), 
        "Expected second parent list item without indentation. Got:\n{}", result
    );
}

#[test]
fn test_link_with_no_extractable_text() {
    let converter = create_converter();
    
    // Case 1: Link with nested empty spans
    let html = r#"<a href="/guide"><span></span></a>"#;
    let md = converter.convert(html).unwrap();
    
    // Should use href as fallback, NOT return empty string
    assert!(md.contains("["), "Link structure must be preserved");
    assert!(md.contains("](/guide)"), "Link URL must be preserved");
    assert!(!md.trim().is_empty(), "Must not return empty string for valid links");
    
    // Case 2: Link with only whitespace
    let html = r#"<a href="/api">   </a>"#;
    let md = converter.convert(html).unwrap();
    
    assert!(md.contains("["), "Link structure must be preserved");
    assert!(md.contains("](/api)"), "Link URL must be preserved");
    
    // Case 3: Link with nested structure but no text
    let html = r#"<a href="/console"><span><span></span></span></a>"#;
    let md = converter.convert(html).unwrap();
    
    assert!(md.contains("["), "Link structure must be preserved");
    assert!(md.contains("](/console)"), "Link URL must be preserved");
}

#[test]
fn test_link_always_produces_markdown_structure() {
    let converter = create_converter();
    
    // Every valid href should produce [text](url) format
    let test_cases = vec![
        (r#"<a href="/guide">Guide</a>"#, "[Guide](/guide)"),
        (r#"<a href="/guide"></a>"#, "(/guide)"),  // Will contain href in some form
        (r#"<a href="/guide">   </a>"#, "(/guide)"),  // Will contain href in some form
        (r#"<a href="/api/v1/users"></a>"#, "(/api/v1/users)"),  // Will contain href
    ];
    
    for (html, expected_contains) in test_cases {
        let md = converter.convert(html).unwrap();
        assert!(
            md.contains(expected_contains),
            "HTML '{}' should produce markdown containing '{}', got: '{}'",
            html, expected_contains, md
        );
        assert!(!md.trim().is_empty(), "Must never return empty for valid href");
    }
}

#[test]
fn test_table_with_thead_proper_headers() {
    let converter = create_converter();
    
    // Test the exact structure from code.claude.com that was failing
    let html = r#"<table>
  <thead>
    <tr>
      <th>File</th>
      <th>Purpose</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><code>~/.claude/settings.json</code></td>
      <td>User settings (permissions, hooks, model overrides)</td>
    </tr>
    <tr>
      <td><code>.claude/settings.json</code></td>
      <td>Project settings (checked into source control)</td>
    </tr>
  </tbody>
</table>"#;
    
    let md = converter.convert(html).unwrap();
    
    // MUST have proper table header row
    assert!(
        md.contains("| File | Purpose |"),
        "Table must have proper header row '| File | Purpose |'. Got: {}",
        md
    );
    
    // MUST have separator row
    assert!(
        md.contains("|---|---|"),
        "Table must have separator row '|---|---|'. Got: {}",
        md
    );
    
    // MUST NOT have header text as plain text above table
    assert!(
        !md.contains("File Purpose\n\n|") && !md.contains("FilePurpose"),
        "Table headers must NOT appear as plain text above the table. Got: {}",
        md
    );
    
    // MUST preserve inline code in cells
    assert!(
        md.contains("`~/.claude/settings.json`"),
        "Table must preserve inline code formatting. Got: {}",
        md
    );
    
    // MUST have data rows
    assert!(
        md.contains("User settings"),
        "Table must contain data rows. Got: {}",
        md
    );
}

#[test]
fn test_table_without_thead() {
    let converter = create_converter();
    
    // Test table without explicit thead - should still work
    let html = r#"<table>
  <tr>
    <th>Column 1</th>
    <th>Column 2</th>
  </tr>
  <tr>
    <td>Data 1</td>
    <td>Data 2</td>
  </tr>
</table>"#;
    
    let md = converter.convert(html).unwrap();
    
    // Should detect th elements and use as header
    assert!(
        md.contains("| Column 1 | Column 2 |"),
        "Should detect <th> elements as headers. Got: {}",
        md
    );
    
    assert!(
        md.contains("|---|---|"),
        "Should have separator row. Got: {}",
        md
    );
}

#[test]
fn test_empty_table() {
    let converter = create_converter();
    
    let html = r#"<table></table>"#;
    let md = converter.convert(html).unwrap();
    
    // Empty table should produce minimal output
    assert!(
        md.trim().is_empty() || md.trim().len() < 10,
        "Empty table should produce minimal/no output. Got: '{}'",
        md
    );
}
