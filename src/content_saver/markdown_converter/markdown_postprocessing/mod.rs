//! Markdown processing and heading normalization functionality.
//!
//! Processes markdown content by normalizing headings and handling various markdown formats.

mod code_fence_detection;
mod heading_extraction;
mod processor;
mod whitespace_normalization;
mod code_block_cleaning;
mod block_spacing;
mod shell_syntax_repair;

#[cfg(test)]
mod tests;

/// Ensure markdown starts with H1 heading
///
/// Checks if markdown already starts with an H1 (`# `). If not, prepends
/// an H1 using either:
/// 1. The first extracted H1 heading from the document (if level == 1)
/// 2. The first heading of any level (if no H1 exists)
/// 3. The document title as fallback (if no headings extracted)
///
/// This fixes the issue where H1 headings inside `<header>` elements
/// are removed during HTML preprocessing, leaving markdown without a
/// top-level heading.
///
/// # Arguments
///
/// * `markdown` - The markdown content to process
/// * `headings` - Extracted heading elements from the page
/// * `title` - Document title as final fallback
///
/// # Returns
///
/// Markdown string with H1 at the start
///
/// # Examples
///
/// ```rust
/// let headings = vec![
///     HeadingElement { level: 1, text: "Main Title".to_string(), id: None, ordinal: vec![1] },
/// ];
/// let markdown = "## Subtitle\n\nContent";
/// let result = ensure_h1_at_start(markdown, &headings, "Fallback Title");
/// // Returns: "# Main Title\n\n## Subtitle\n\nContent"
/// ```
///
/// ```rust
/// let markdown = "# Already Has H1\n\nContent";
/// let result = ensure_h1_at_start(markdown, &[], "Title");
/// // Returns unchanged: "# Already Has H1\n\nContent"
/// ```
pub fn ensure_h1_at_start(
    markdown: &str,
    headings: &[crate::page_extractor::schema::HeadingElement],
    title: &str,
) -> String {
    // Check if markdown already starts with H1
    // trim_start() handles leading whitespace/newlines
    if markdown.trim_start().starts_with("# ") {
        return markdown.to_string();
    }

    // Determine H1 text to use:
    // 1. First extracted H1 (level == 1)
    // 2. First heading of any level (fallback if no H1)
    // 3. Document title (final fallback)
    let h1_text = if let Some(first_heading) = headings.first() {
        if first_heading.level == 1 {
            // Use the first H1
            &first_heading.text
        } else {
            // No H1 extracted, use document title
            // (Don't use lower-level headings as they may be contextual)
            title
        }
    } else {
        // No headings at all, use title
        title
    };

    // Prepend H1 with proper spacing
    // Format: "# {text}\n\n{original_markdown}"
    // The double newline ensures proper separation from content
    format!("# {}\n\n{}", h1_text, markdown)
}

// Re-export public API
pub use heading_extraction::{extract_heading_level, normalize_heading_level};
pub use processor::process_markdown_headings;
pub use processor::fix_merged_code_fences;
pub use whitespace_normalization::normalize_whitespace;
pub use whitespace_normalization::normalize_inline_formatting_spacing;
pub use whitespace_normalization::fix_bold_internal_spacing;
pub use code_block_cleaning::filter_collapsed_lines;
pub use code_block_cleaning::strip_bold_from_code_fences;
pub use code_block_cleaning::normalize_code_fences;
pub use code_block_cleaning::strip_trailing_asterisks_after_code_fences;
pub use code_block_cleaning::strip_residual_html_tags;
pub use code_block_cleaning::remove_duplicate_code_blocks;
pub use code_block_cleaning::fix_shebang_lines;
pub use block_spacing::ensure_block_element_spacing;
pub use shell_syntax_repair::repair_shell_syntax;
