//! Markdown processing and heading normalization functionality.
//!
//! Processes markdown content by normalizing headings and handling various markdown formats.

mod code_fence_detection;
mod heading_extraction;
mod processor;
mod whitespace_normalization;
mod code_block_cleaning;

#[cfg(test)]
mod tests;

// Re-export public API
pub use heading_extraction::{extract_heading_level, normalize_heading_level};
pub use processor::process_markdown_headings;
pub use whitespace_normalization::normalize_whitespace;
pub use code_block_cleaning::filter_collapsed_lines;
