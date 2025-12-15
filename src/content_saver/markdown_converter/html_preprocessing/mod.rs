//! HTML preprocessing functionality for markdown conversion.
//!
//! This module provides three main functions:
//! 1. `extract_main_content` - Intelligently extracts the primary content from HTML
//! 2. `clean_html_content` - Removes scripts, styles, ads, and other non-content elements
//! 3. `preprocess_tables` - Normalizes tables by expanding colspan/rowspan and detecting layout tables
//!
//! These functions prepare HTML for optimal markdown conversion.

// Submodules
mod main_content_extraction;
mod html_cleaning;
mod table_preprocessing;
mod expressive_code;
mod code_block_protection;

// Re-export public API
pub use main_content_extraction::extract_main_content;
pub use main_content_extraction::transform_tabs_to_sections;
pub use main_content_extraction::transform_callouts_to_blockquotes;
pub use main_content_extraction::transform_card_grids_to_tables;
pub use main_content_extraction::transform_link_cards_to_lists;
pub use main_content_extraction::transform_mcp_server_cards;
pub use html_cleaning::clean_html_content;
pub use html_cleaning::normalize_html_structure;
pub use table_preprocessing::preprocess_tables;
pub use expressive_code::{preprocess_expressive_code, convert_br_to_newlines_in_code};
pub use code_block_protection::CodeBlockProtector;
