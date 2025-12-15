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

// Re-export public API
pub use heading_extraction::{extract_heading_level, normalize_heading_level};
pub use processor::process_markdown_headings;
pub use processor::fix_merged_code_fences;
pub use whitespace_normalization::normalize_whitespace;
pub use whitespace_normalization::normalize_inline_formatting_spacing;
pub use code_block_cleaning::filter_collapsed_lines;
pub use code_block_cleaning::strip_bold_from_code_fences;
pub use code_block_cleaning::strip_trailing_asterisks_after_code_fences;
pub use code_block_cleaning::strip_residual_html_tags;
pub use code_block_cleaning::remove_duplicate_code_blocks;
pub use code_block_cleaning::fix_shebang_lines;
pub use block_spacing::ensure_block_element_spacing;
pub use shell_syntax_repair::repair_shell_syntax;
