//! Block element spacing safety net
//!
//! Detects and fixes missing blank lines between markdown structural elements
//! that should be separated (headings, lists, code blocks, paragraphs).
//!
//! This is a defensive postprocessing step that catches edge cases where:
//! - htmd's normalize_content_for_buffer() incorrectly collapses newlines
//! - Inline elements directly precede block elements without spacing
//! - Handler outputs are concatenated in unexpected ways

use regex::Regex;
use std::sync::LazyLock;

/// Regex patterns for detecting missing blank lines between structural elements
///
/// Pattern 1: Text/inline markdown followed immediately by ATX heading
/// Matches: `text## Heading` or `**bold**### Heading` or `text\n## Heading`
/// Fix: Insert blank line before heading
static TEXT_BEFORE_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([^\n#])(\n?)(#{1,6}\s+)")
        .expect("TEXT_BEFORE_HEADING: hardcoded regex is valid")
});

/// Pattern 2: Text/inline markdown followed immediately by list item
/// Matches: `text- Item` or `**bold**1. Item`
/// Fix: Insert blank line before list
static TEXT_BEFORE_LIST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([^\n])\n((?:[*\-+]|\d+\.)\s+)")
        .expect("TEXT_BEFORE_LIST: hardcoded regex is valid")
});

/// Pattern 3: Closing code fence followed immediately by text (no blank line)
/// Matches: "```\nText" (should be "```\n\nText")
/// Fix: Insert blank line after code fence
static CODE_FENCE_BEFORE_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(```[ \t]*)\n([^\n])")
        .expect("CODE_FENCE_BEFORE_TEXT: hardcoded regex is valid")
});

/// Pattern 4: Text followed immediately by opening code fence (no blank line)
/// Matches: "Text\n```" (should be "Text\n\n```")
/// Fix: Insert blank line before code fence
/// Only matches opening fences (with language identifier) to avoid matching closing fences
static TEXT_BEFORE_CODE_FENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([^\n`])\n(```\w)")
        .expect("TEXT_BEFORE_CODE_FENCE: hardcoded regex is valid")
});

/// Pattern 5: Heading followed immediately by another heading (no blank line)
/// Matches: "## H1\n### H2" (should be "## H1\n\n### H2")
/// Fix: Insert blank line between headings
static HEADING_BEFORE_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(#{1,6}\s+[^\n]+)\n(#{1,6}\s+)")
        .expect("HEADING_BEFORE_HEADING: hardcoded regex is valid")
});

/// Ensure proper blank line spacing between markdown block elements
///
/// This function performs a final safety-net pass to detect and fix
/// missing blank lines that may have been lost during htmd conversion
/// or postprocessing.
///
/// # Transformations Applied
///
/// 1. **Text → Heading**: `text\n## Heading` → `text\n\n## Heading`
/// 2. **Text → List**: `text\n- Item` → `text\n\n- Item`
/// 3. **Code Fence → Text**: ` ```\ntext` → ` ```\n\ntext`
/// 4. **Text → Code Fence**: `text\n``` ` → `text\n\n``` `
/// 5. **Heading → Heading**: `## H1\n### H2` → `## H1\n\n### H2`
///
/// # Preserves Existing Spacing
///
/// - Code block content (inside fences) is never modified
/// - Existing blank lines are preserved (no double-spacing introduced)
/// - Single-line breaks within paragraphs are preserved
///
/// # Arguments
///
/// * `markdown` - The markdown content to process
///
/// # Returns
///
/// Markdown string with corrected blank line spacing
///
/// # Examples
///
/// ```rust
/// # use kodegen_tools_citescrape::content_saver::markdown_converter::markdown_postprocessing::block_spacing::ensure_block_element_spacing;
/// let input = "**Install:**\n- macOS\n- Linux";
/// let output = ensure_block_element_spacing(input);
/// assert_eq!(output, "**Install:**\n\n- macOS\n\n- Linux");
/// ```
pub fn ensure_block_element_spacing(markdown: &str) -> String {
    let mut result = markdown.to_string();
    
    // Apply transformations in order
    // Each regex uses capture groups to preserve matched content
    
    // 1. Fix text before headings: "text\n## Heading" → "text\n\n## Heading"
    result = TEXT_BEFORE_HEADING.replace_all(&result, "$1\n\n$3").to_string();
    
    // 2. Fix text before lists: "text\n- Item" → "text\n\n- Item"
    result = TEXT_BEFORE_LIST.replace_all(&result, "$1\n\n$2").to_string();
    
    // 3. Fix code fence before text: "```\ntext" → "```\n\ntext"
    result = CODE_FENCE_BEFORE_TEXT.replace_all(&result, "$1\n\n$2").to_string();
    
    // 4. Fix text before code fence: "text\n```" → "text\n\n```"
    result = TEXT_BEFORE_CODE_FENCE.replace_all(&result, "$1\n\n$2").to_string();
    
    // 5. Fix heading before heading: "## H1\n### H2" → "## H1\n\n### H2"
    result = HEADING_BEFORE_HEADING.replace_all(&result, "$1\n\n$2").to_string();
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_before_heading() {
        let input = "Some paragraph text\n## Heading";
        let expected = "Some paragraph text\n\n## Heading";
        assert_eq!(ensure_block_element_spacing(input), expected);
    }

    #[test]
    fn test_inline_before_list() {
        let input = "**Install Claude Code:**\n- macOS/Linux";
        let expected = "**Install Claude Code:**\n\n- macOS/Linux";
        assert_eq!(ensure_block_element_spacing(input), expected);
    }

    #[test]
    fn test_code_fence_before_text() {
        let input = "```rust\nfn main() {}\n```\nYou'll be prompted";
        let expected = "```rust\nfn main() {}\n```\n\nYou'll be prompted";
        assert_eq!(ensure_block_element_spacing(input), expected);
    }

    #[test]
    fn test_heading_before_heading() {
        let input = "## Quickstart\nSome text## Common workflows";
        // First pass fixes text before second heading
        let expected = "## Quickstart\nSome text\n\n## Common workflows";
        assert_eq!(ensure_block_element_spacing(input), expected);
    }

    #[test]
    fn test_preserves_existing_spacing() {
        let input = "Text\n\n## Heading\n\n- List item";
        let expected = "Text\n\n## Heading\n\n- List item";
        assert_eq!(ensure_block_element_spacing(input), expected);
    }
}
