//! HTML preprocessing functionality for markdown conversion.
//!
//! This module provides two main functions:
//! 1. `extract_main_content` - Intelligently extracts the primary content from HTML
//! 2. `clean_html_content` - Removes scripts, styles, ads, and other non-content elements
//!
//! These functions prepare HTML for optimal markdown conversion.

// Submodules
mod main_content_extraction;
mod html_cleaning;

// Re-export public API
pub use main_content_extraction::extract_main_content;
pub use html_cleaning::clean_html_content;
