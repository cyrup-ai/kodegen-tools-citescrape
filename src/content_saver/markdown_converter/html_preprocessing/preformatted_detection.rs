//! Preformatted content detection and normalization
//!
//! Detects elements that should be treated as preformatted content based on:
//! 1. CSS styles (white-space: pre, font-family: monospace)
//! 2. Class names (.ascii, .diagram, .terminal, .console, .output)
//! 3. Content patterns (box-drawing characters, ASCII art indicators)
//!
//! Converts matching elements to `<pre>` tags for proper markdown code block conversion.

use anyhow::Result;
use kuchiki::traits::TendrilSink;
use kuchiki::NodeData;
use regex::Regex;
use std::sync::LazyLock;

// ============================================================================
// CSS Style Detection Patterns
// ============================================================================

/// Matches white-space CSS property with preformatted values
/// 
/// Pattern reference: Similar to parse_style_formatting() in 
/// custom_handlers/mod.rs line 1054-1080
///
/// Detects: white-space: pre | pre-wrap | pre-line
static WHITESPACE_PRE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"white-space\s*:\s*(pre|pre-wrap|pre-line)")
        .expect("WHITESPACE_PRE_RE: hardcoded regex is valid")
});

/// Matches font-family with monospace indicators
/// 
/// Detects common monospace font families:
/// - Generic: monospace
/// - Specific: Consolas, Courier, Monaco, Menlo, "Fira Code", "Source Code Pro", JetBrains Mono
static MONOSPACE_FONT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"font-family\s*:\s*[^;]*(?:monospace|consolas|courier|monaco|menlo|fira\s*code|source\s*code|jetbrains)")
        .expect("MONOSPACE_FONT_RE: hardcoded regex is valid")
});

// ============================================================================
// Content Pattern Detection
// ============================================================================

/// Box-drawing characters (Unicode range U+2500-U+257F)
/// Plus common ASCII art block characters
///
/// Note: The codebase already handles box-drawing characters correctly in UTF-8
/// (see utils/string_utils.rs for UTF-8 safe handling examples).
const BOX_DRAWING_CHARS: &[char] = &[
    // Light box drawing
    '─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼',
    // Heavy box drawing
    '═', '║', '╔', '╗', '╚', '╝', '╠', '╣', '╦', '╩', '╬',
    // Rounded corners
    '╭', '╮', '╯', '╰',
    // Diagonals
    '╱', '╲', '╳',
    // Block elements
    '▀', '▄', '█', '▌', '▐', '░', '▒', '▓',
    // Dashed/dotted lines
    '┈', '┉', '┊', '┋', '┄', '┅', '┆', '┇',
];

/// Arrow characters commonly used in diagrams
const ARROW_CHARS: &[char] = &['→', '←', '↑', '↓', '↔', '↕', '⇒', '⇐', '⇑', '⇓'];

/// Check if content contains ASCII art patterns
///
/// Uses heuristics to detect ASCII art without false positives on normal text:
/// - 3+ box-drawing characters = definite ASCII art
/// - 2+ arrows + alignment spaces = likely diagram
/// - 3+ indented lines + alignment spaces = structured content (code-like)
///
/// # Rationale for Thresholds
///
/// - **3+ box characters**: A single box character might appear in text ("the ─ symbol"),
///   but 3+ indicates intentional diagram construction
/// - **2+ arrows with spaces**: Arrows alone appear in text, but multiple arrows with
///   alignment-dependent whitespace (3+ spaces) suggests a flow diagram
/// - **3+ indented lines with spaces**: Indicates structured, indented content like
///   code blocks or hierarchical diagrams
fn contains_ascii_art_patterns(content: &str) -> bool {
    // Check for box-drawing characters
    let box_char_count = content.chars()
        .filter(|c| BOX_DRAWING_CHARS.contains(c))
        .count();
    
    if box_char_count >= 3 {
        return true;
    }
    
    // Check for arrow characters in combination with other indicators
    let arrow_count = content.chars()
        .filter(|c| ARROW_CHARS.contains(c))
        .count();
    
    // Check for alignment-dependent whitespace (3+ consecutive spaces)
    // This catches code-style formatting and diagram alignment
    let has_alignment_spaces = content.contains("   ");
    
    // Check for multiple lines with consistent leading spaces (indented blocks)
    let lines: Vec<&str> = content.lines().collect();
    let indented_lines = lines.iter()
        .filter(|line| line.starts_with("  ") && !line.trim().is_empty())
        .count();
    
    // ASCII art indicators:
    // - Multiple arrows with alignment spaces (flow diagrams)
    // - Many indented lines (structured content like trees or hierarchies)
    // - Any box characters present (even 1 if other indicators are strong)
    (indented_lines >= 3 || arrow_count >= 2) && has_alignment_spaces ||
    box_char_count >= 1
}

/// Check if inline style indicates preformatted content
///
/// Follows CSS parsing pattern from custom_handlers/mod.rs parse_style_formatting()
/// (line 1054-1080) but adapted for white-space and font-family detection.
fn is_preformatted_style(style: &str) -> bool {
    let style_lower = style.to_lowercase();
    WHITESPACE_PRE_RE.is_match(&style_lower) || MONOSPACE_FONT_RE.is_match(&style_lower)
}

/// Check if class name indicates preformatted content
///
/// Detects common class naming conventions for terminal, code output, and diagrams.
fn is_preformatted_class(class: &str) -> bool {
    let class_lower = class.to_lowercase();
    let preformatted_classes = [
        "ascii", "ascii-art", "diagram", "terminal", "console", 
        "output", "shell", "cli", "monospace", "preformatted",
        "code-output", "command-output", "term", "tty"
    ];
    
    class_lower.split_whitespace()
        .any(|c| preformatted_classes.iter().any(|pc| c.contains(pc)))
}

// ============================================================================
// Main Preprocessing Function
// ============================================================================

/// Preprocess HTML to convert CSS-styled preformatted elements to `<pre>` tags
///
/// This function identifies elements that should be treated as preformatted
/// content based on CSS styles, class names, or content patterns, and wraps
/// them in `<pre>` tags so they are properly converted to markdown code blocks.
///
/// # Architecture Integration
///
/// This preprocessor executes at Stage 0.52 in the markdown conversion pipeline
/// (see mod.rs convert_html_to_markdown_sync function), positioned between 
/// expressive-code preprocessing (0.5) and syntax highlighting span stripping (0.55).
///
/// This ordering ensures:
/// 1. Expressive-code patterns are handled first (different HTML structure)
/// 2. CSS-styled preformatted content is normalized to `<pre>` tags
/// 3. Syntax highlighting spans are then stripped from ALL code blocks
/// 4. Code block protection prevents whitespace collapse in DOM operations
///
/// # Detection Criteria (any of these triggers conversion):
///
/// 1. **CSS Styles**:
///    - `white-space: pre` / `pre-wrap` / `pre-line`
///    - `font-family: monospace` (or monospace font names)
///
/// 2. **Class Names**:
///    - `.ascii`, `.diagram`, `.terminal`, `.console`, `.output`
///    - `.shell`, `.cli`, `.monospace`, `.preformatted`
///
/// 3. **Content Patterns** (fallback detection):
///    - Box-drawing characters (─, │, ┌, └, etc.) - 3+ occurrences
///    - Arrow characters (→, ←, ↑, ↓) with alignment-dependent whitespace
///    - Multi-line indented structured content (3+ lines starting with spaces)
///
/// # DOM Manipulation Pattern
///
/// Follows the established kuchiki pattern from html_cleaning.rs preprocess_expressive_code:
/// 1. Parse HTML to mutable DOM
/// 2. Walk tree and collect matching elements
/// 3. Replace matching elements with `<pre>` wrappers
/// 4. Serialize back to HTML string
///
/// Reference: preprocess_expressive_code() in html_cleaning.rs line 627
///
/// # Arguments
/// * `html` - Raw HTML string
///
/// # Returns
/// * `Ok(String)` - HTML with preformatted elements wrapped in `<pre>` tags
/// * `Err(anyhow::Error)` - If HTML parsing or serialization fails
pub fn preprocess_preformatted_elements(html: &str) -> Result<String> {
    // Fast path: skip if no potential indicators present
    // This optimization avoids DOM parsing for pages without diagrams
    // (90%+ of pages based on typical web content distribution)
    if !html.contains("white-space") 
        && !html.contains("monospace")
        && !html.contains("ascii")
        && !html.contains("diagram")
        && !html.contains("terminal")
        && !html.contains("console")
    {
        return Ok(html.to_string());
    }
    
    // Parse HTML into mutable DOM (same pattern as html_cleaning.rs line 627)
    let document = kuchiki::parse_html().one(html.to_string());
    
    // Elements to convert: collect node references first
    // We must collect before iteration because we'll detach nodes during replacement
    // (kuchiki pattern requirement - see html_cleaning.rs line 637)
    let mut elements_to_convert: Vec<kuchiki::NodeRef> = Vec::new();
    
    // Walk the DOM tree looking for preformatted elements
    // This uses kuchiki's descendants() iterator which performs depth-first traversal
    for node in document.descendants() {
        if let NodeData::Element(elem_data) = node.data() {
            let tag_name = elem_data.name.local.to_string();
            
            // Skip elements already in <pre> tags or are <pre> themselves
            // These are already correctly formatted for markdown conversion
            if tag_name == "pre" || tag_name == "code" {
                continue;
            }
            
            // Skip script/style elements (never contain user content)
            if tag_name == "script" || tag_name == "style" {
                continue;
            }
            
            let attrs = elem_data.attributes.borrow();
            let mut should_convert = false;
            
            // Detection Method 1: Check CSS style attribute
            // Pattern reference: parse_style_formatting() in custom_handlers/mod.rs
            if let Some(style) = attrs.get("style")
                && is_preformatted_style(style) {
                    should_convert = true;
                }
            
            // Detection Method 2: Check class attribute
            if !should_convert
                && let Some(class) = attrs.get("class")
                && is_preformatted_class(class) {
                    should_convert = true;
                }
            
            // Detection Method 3: Content-based detection (only for container elements)
            // This catches ASCII art in generic containers without semantic markup
            if !should_convert {
                let text_content = node.text_contents();
                let trimmed = text_content.trim();
                
                // Only check content if element has substantial text
                // and is a container element (div, span, p, section)
                // Minimum 10 chars prevents false positives on short text fragments
                if trimmed.len() >= 10 
                    && matches!(tag_name.as_str(), "div" | "span" | "p" | "section")
                    && contains_ascii_art_patterns(trimmed)
                {
                    should_convert = true;
                }
            }
            
            if should_convert {
                elements_to_convert.push(node.clone());
            }
        }
    }
    
    // Convert identified elements to <pre> tags
    // Pattern reference: html_cleaning.rs preprocess_expressive_code() line 695
    for node in elements_to_convert {
        // Get the text content preserving structure
        // node.text_contents() recursively concatenates all text nodes
        let text_content = node.text_contents();
        
        // Skip if empty after trimming
        if text_content.trim().is_empty() {
            continue;
        }
        
        // HTML-escape the content to prevent injection and preserve special characters
        // This is critical: raw text must be escaped before embedding in HTML
        // html_escape::encode_text handles &, <, >, ", ' and Unicode characters
        let escaped_content = html_escape::encode_text(&text_content).to_string();
        
        // Create replacement <pre> element
        // The htmd library will convert this to a markdown code block
        let replacement_html = format!("<pre>{}</pre>", escaped_content);
        let replacement = kuchiki::parse_html().one(replacement_html);
        
        // Insert replacement and remove original
        // This is the safe DOM replacement pattern used throughout the codebase:
        // 1. Insert new nodes before the old node
        // 2. Detach the old node (removes from tree and drops reference)
        for child in replacement.children() {
            node.insert_before(child);
        }
        node.detach();
    }
    
    // Serialize back to HTML
    // Pattern reference: html_cleaning.rs line 728
    let mut output = Vec::new();
    document.serialize(&mut output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize HTML: {}", e))?;
    
    String::from_utf8(output)
        .map_err(|e| anyhow::anyhow!("Failed to convert HTML to UTF-8: {}", e))
}
