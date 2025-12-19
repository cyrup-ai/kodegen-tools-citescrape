//! HTML preprocessing functionality for markdown conversion.
//!
//! This module provides three main functions:
//! 1. `extract_main_content` - Intelligently extracts the primary content from HTML
//! 2. `clean_html_content` - Removes scripts, styles, ads, and other non-content elements
//! 3. `preprocess_tables` - Normalizes tables by expanding colspan/rowspan and detecting layout tables
//!
//! These functions prepare HTML for optimal markdown conversion.

// Submodules
pub mod main_content_extraction;
pub mod html_cleaning;
mod table_preprocessing;
mod expressive_code;
pub mod code_block_protection;
mod preformatted_detection;

// Re-export public API
pub use main_content_extraction::extract_main_content;
// Callout transformation removed - it was site-specific
// Link card transformation removed - it was site-specific (assumed "card" in class names)
pub use html_cleaning::clean_html_content;
pub use html_cleaning::normalize_html_structure;
pub use html_cleaning::strip_syntax_highlighting_spans;
pub use table_preprocessing::{preprocess_tables, fix_pre_table_text, inject_preceding_headers};
pub use expressive_code::{preprocess_expressive_code, convert_br_to_newlines_in_code};
pub use code_block_protection::CodeBlockProtector;
pub use preformatted_detection::preprocess_preformatted_elements;
