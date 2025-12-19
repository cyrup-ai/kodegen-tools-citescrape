//! HTML preprocessing utilities for markdown conversion.
//!
//! This module provides essential HTML preprocessing before htmd conversion:
//! - Main content extraction via CSS selectors
//!
//! Note: HTML cleaning (widget filtering, script/style removal) is now handled
//! by htmd element handlers during DOM traversal. See:
//! - htmd/element_handler/div.rs (widget filtering, expressive-code extraction)
//! - htmd/element_handler/section.rs (widget filtering)
//! - htmd/element_handler/aside.rs (widget filtering)
//! - htmd/element_handler/element_util.rs (is_widget_element patterns)

pub mod main_content_extraction;

pub use main_content_extraction::extract_main_content;
