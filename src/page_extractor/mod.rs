//! Page data extraction functions.
//!
//! This module provides functions for extracting various types of data from web pages
//! including metadata, timing information, security details, and links.

// Sub-modules
pub mod extractors;
pub mod js_scripts;
pub mod link_rewriter;
pub mod page_data;
pub mod schema;

// Re-exports for public API
pub use extractors::{capture_screenshot, scroll_to_bottom, wait_for_page_load};
pub use page_data::extract_page_data;
