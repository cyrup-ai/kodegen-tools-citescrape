//! HTML preprocessing utilities for markdown conversion.
//!
//! This module provides essential HTML preprocessing before htmd conversion:
//! - Main content extraction via CSS selectors
//! - HTML cleaning (remove scripts, styles, ads, tracking)

pub mod main_content_extraction;
pub mod html_cleaning;

pub use main_content_extraction::extract_main_content;
pub use html_cleaning::{clean_html_content, normalize_html_structure};
