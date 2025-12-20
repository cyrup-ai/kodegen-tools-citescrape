//! Recursive list processing with explicit depth tracking
//!
//! Handles HTML `<ul>` and `<ol>` elements by manually traversing the DOM
//! and tracking nesting depth. This bypasses htmd's stateless handler system
//! which cannot preserve list hierarchy.
//!
//! ## Key Design Decisions
//!
//! 1. **Bypass `walk_children()`** - We manually traverse `<li>` children to
//!    intercept nested `<ul>`/`<ol>` before htmd flattens them.
//!
//! 2. **Use `handlers.handle()` for non-list elements** - Inline elements
//!    (`<strong>`, `<code>`, `<a>`) and block elements (`<p>`, `<pre>`) are
//!    processed through htmd's registered handlers for proper formatting.
//!
//! 3. **Trim and normalize whitespace** - Text nodes from HTML contain extra
//!    whitespace from formatting. We trim individual nodes and join appropriately.
//!
//! 4. **Separate inline vs block content** - Inline content joined with space,
//!    block content (code blocks, paragraphs) joined with newlines.
//!
//! 5. **Depth-aware indentation** - 2 spaces per nesting level for list items,
//!    marker-aligned continuation for multi-line content.

use std::rc::Rc;
use markup5ever_rcdom::{Node, NodeData};
use super::Handlers;

// ============================================================================
// Data Structures
// ============================================================================

/// Represents a single list item with depth context
struct ListItem {
    /// The text content of the list item (may be multi-line for block content)
    content: String,
    /// Nesting depth (0 = root level)
    depth: usize,
    /// The list marker type
    marker: ListMarker,
}

/// List marker variants
enum ListMarker {
    /// Unordered list bullet: `-`
    Bullet,
    /// Ordered list number: `1.`, `2.`, etc.
    Number(usize),
}

impl ListMarker {
    /// Get the string representation of the marker
    fn as_str(&self) -> String {
        match self {
            ListMarker::Bullet => "-".to_string(),
            ListMarker::Number(n) => format!("{}.", n),
        }
    }
    
    /// Get the length of the marker including trailing space
    /// Used for continuation line alignment
    fn display_len(&self) -> usize {
        self.as_str().len() + 1  // +1 for the space after marker
    }
}

/// A nested list found within an `<li>` element
struct NestedList {
    /// Reference to the `<ul>` or `<ol>` node
    node: Rc<Node>,
    /// Whether this is an ordered list
    is_ordered: bool,
}

// ============================================================================
// Public API
// ============================================================================

/// Process a list element (`<ul>` or `<ol>`) into markdown
///
/// Entry point called from `ul_handler` and `ol_handler` in mod.rs.
/// Handles arbitrary nesting depth with proper 2-space indentation.
///
/// # Arguments
/// * `handlers` - htmd handlers for processing non-list inline content
/// * `list_node` - The `<ul>` or `<ol>` DOM node
/// * `is_ordered` - `true` for `<ol>`, `false` for `<ul>`
///
/// # Returns
/// Formatted markdown string with proper indentation
pub fn process_list(
    handlers: &dyn Handlers,
    list_node: &Rc<Node>,
    is_ordered: bool,
) -> String {
    let start_number = if is_ordered {
        get_list_start_number(list_node)
    } else {
        1
    };
    
    let items = process_list_recursive(handlers, list_node, 0, is_ordered, start_number);
    format_list_items(&items)
}

/// Extract the `start` attribute from an `<ol>` element
///
/// Returns 1 if attribute is missing or invalid.
fn get_list_start_number(list_node: &Rc<Node>) -> usize {
    if let NodeData::Element { attrs, .. } = &list_node.data {
        for attr in attrs.borrow().iter() {
            if &*attr.name.local == "start" {
                return attr.value.parse().unwrap_or(1);
            }
        }
    }
    1
}

// ============================================================================
// Core Recursive Processing
// ============================================================================

/// Recursively process a list with explicit depth tracking
fn process_list_recursive(
    handlers: &dyn Handlers,
    list_node: &Rc<Node>,
    depth: usize,
    is_ordered: bool,
    start_number: usize,
) -> Vec<ListItem> {
    let mut items = Vec::new();
    let mut counter = start_number;
    
    for child in list_node.children.borrow().iter() {
        // Only process <li> elements
        if !is_li_element(child) {
            continue;
        }
        
        // Separate inline/block content from nested lists
        let (content, nested_lists) = process_li_children(handlers, child);
        
        // Create the list item marker
        let marker = if is_ordered {
            let m = ListMarker::Number(counter);
            counter += 1;
            m
        } else {
            ListMarker::Bullet
        };
        
        // Add the item if it has content
        let trimmed_content = content.trim().to_string();
        if !trimmed_content.is_empty() {
            items.push(ListItem {
                content: trimmed_content,
                depth,
                marker,
            });
        }
        
        // Process nested lists recursively with increased depth
        for nested in nested_lists {
            let nested_start = if nested.is_ordered {
                get_list_start_number(&nested.node)
            } else {
                1
            };
            
            let nested_items = process_list_recursive(
                handlers,
                &nested.node,
                depth + 1,
                nested.is_ordered,
                nested_start,
            );
            items.extend(nested_items);
        }
    }
    
    items
}

/// Process children of an `<li>` element
///
/// Separates inline/text content from nested `<ul>`/`<ol>` elements.
/// Uses `handlers.handle()` for formatting inline elements like `<strong>`, `<code>`, etc.
///
/// # Whitespace Handling
///
/// Text nodes from HTML contain extra whitespace from source formatting:
/// ```html
/// <li>
///   Text content here
/// </li>
/// ```
/// Results in text node: `"\n  Text content here\n"` - we trim this.
///
/// # Block vs Inline Elements
///
/// - **Inline** (`<strong>`, `<code>`, `<a>`, `<em>`): Joined with spaces
/// - **Block** (`<p>`, `<pre>`, `<div>`): Joined with newlines
fn process_li_children(
    handlers: &dyn Handlers,
    li_node: &Rc<Node>,
) -> (String, Vec<NestedList>) {
    let mut inline_parts: Vec<String> = Vec::new();
    let mut block_parts: Vec<String> = Vec::new();
    let mut nested_lists = Vec::new();
    
    for child in li_node.children.borrow().iter() {
        match &child.data {
            NodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                // Trim whitespace from HTML formatting, preserve internal whitespace
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    inline_parts.push(trimmed.to_string());
                }
            }
            NodeData::Element { name, .. } => {
                let tag = &*name.local;
                
                if tag == "ul" {
                    nested_lists.push(NestedList {
                        node: Rc::clone(child),
                        is_ordered: false,
                    });
                } else if tag == "ol" {
                    nested_lists.push(NestedList {
                        node: Rc::clone(child),
                        is_ordered: true,
                    });
                } else if is_block_element(tag) {
                    // Block elements (p, pre, div) - process and add to block_parts
                    // htmd returns "\n\nContent\n\n" for blocks - we trim this
                    if let Some(result) = handlers.handle(child) {
                        let content = result.content.trim();
                        if !content.is_empty() {
                            block_parts.push(content.to_string());
                        }
                    }
                } else {
                    // Inline elements (strong, code, a, em, span, etc.)
                    // Process through htmd handlers for proper formatting
                    if let Some(result) = handlers.handle(child) {
                        let content = result.content.trim();
                        if !content.is_empty() {
                            inline_parts.push(content.to_string());
                        }
                    } else {
                        // FALLBACK: If handler returns None, extract raw text
                        // This handles cases where emphasis_handler fails due to empty walk_children()
                        let raw_text = extract_raw_text_from_node(child);
                        let trimmed = raw_text.trim();
                        if !trimmed.is_empty() {
                            // Apply markdown formatting based on tag
                            let formatted = match &*name.local {
                                "strong" | "b" => format!("**{}**", trimmed),
                                "em" | "i" => format!("*{}*", trimmed),
                                "code" => format!("`{}`", trimmed),
                                _ => trimmed.to_string(),
                            };
                            inline_parts.push(formatted);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    
    // Join inline parts with space (they flow together in prose)
    let inline_content = inline_parts.join(" ");
    
    // Combine inline and block content
    let mut all_parts = Vec::new();
    if !inline_content.is_empty() {
        all_parts.push(inline_content);
    }
    all_parts.extend(block_parts);
    
    // Join all parts with newline (for multi-block content like text + code block)
    let text_content = all_parts.join("\n");
    
    (text_content, nested_lists)
}

// ============================================================================
// Output Formatting
// ============================================================================

/// Format collected list items into markdown string
///
/// # Indentation Rules (CommonMark Standard)
///
/// 1. **Depth indent**: 2 spaces per nesting level
///    - Depth 0: no prefix
///    - Depth 1: "  " (2 spaces)
///    - Depth 2: "    " (4 spaces)
///
/// 2. **Continuation indent**: Aligns with content after marker
///    - Bullet (`- `): 2 chars → 2-space continuation
///    - Number (`1. `): 3 chars → 3-space continuation
///    - Number (`10. `): 4 chars → 4-space continuation
///    - Combined: depth_indent + marker_len spaces
///
/// # Example Output
///
/// ```markdown
/// - Top level item
///   - Nested item (2-space depth indent)
///     - Deeply nested (4-space depth indent)
/// 1. Ordered item
///    Code block here (3-space continuation)
/// ```
fn format_list_items(items: &[ListItem]) -> String {
    let mut output = String::new();
    
    for (i, item) in items.iter().enumerate() {
        // Calculate indentation: 2 spaces per depth level
        let depth_indent = "  ".repeat(item.depth);
        
        // Format the marker
        let marker_str = item.marker.as_str();
        
        // Handle multi-line content (indent continuation lines)
        let lines: Vec<&str> = item.content.lines().collect();
        if lines.is_empty() {
            continue;
        }
        
        // First line: depth_indent + marker + space + content
        output.push_str(&depth_indent);
        output.push_str(&marker_str);
        output.push(' ');
        output.push_str(lines[0]);
        output.push('\n');
        
        // Continuation lines: depth_indent + marker-width spaces
        // This aligns continuation with the content start after the marker
        if lines.len() > 1 {
            let continuation_indent = format!(
                "{}{}",
                depth_indent,
                " ".repeat(item.marker.display_len())
            );
            
            for line in &lines[1..] {
                if line.is_empty() {
                    // Preserve blank lines in multi-paragraph content
                    output.push('\n');
                } else {
                    output.push_str(&continuation_indent);
                    output.push_str(line);
                    output.push('\n');
                }
            }
        }
        
        // Add blank line between items with block content (like the existing ol_handler)
        // Only if this item has multi-line content AND there's a next item
        if lines.len() > 1 && i < items.len() - 1 {
            output.push('\n');
        }
    }
    
    // Remove trailing newline for clean output
    output.trim_end().to_string()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if a node is an `<li>` element
fn is_li_element(node: &Rc<Node>) -> bool {
    if let NodeData::Element { name, .. } = &node.data {
        return &*name.local == "li";
    }
    false
}

/// Check if a tag is a block-level element
///
/// Block elements produce `\n\n...\n\n` output from htmd and should be
/// joined with newlines rather than spaces.
fn is_block_element(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "pre" | "div" | "blockquote" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" |
        "address" | "article" | "aside" | "details" | "dialog" | "fieldset" |
        "figcaption" | "figure" | "footer" | "form" | "header" | "main" | "nav" |
        "section" | "table"
    )
}

/// Extract raw text from a node when handlers fail
///
/// This bypasses the htmd handler system and directly extracts text content.
/// Used as a fallback when emphasis_handler returns None for elements with valid content.
fn extract_raw_text_from_node(node: &Rc<Node>) -> String {
    match &node.data {
        NodeData::Text { contents } => contents.borrow().to_string(),
        NodeData::Element { .. } => {
            let mut text = String::new();
            for child in node.children.borrow().iter() {
                text.push_str(&extract_raw_text_from_node(child));
            }
            text
        }
        _ => String::new(),
    }
}
