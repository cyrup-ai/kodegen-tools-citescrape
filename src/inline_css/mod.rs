//! Resource inlining functionality
//!
//! This module provides functionality for inlining external resources (CSS, images, SVGs)
//! into HTML content, converting them to embedded content for self-contained documents.

// Sub-modules
pub mod css_downloader;
pub mod domain_queue;
pub mod downloaders;
pub mod image_downloader;
pub mod orchestrator;
pub mod processors;
pub mod svg_downloader;
pub mod types;
pub mod utils;

#[cfg(test)]
mod kuchiki_test;

// Re-exports for public API
pub use orchestrator::{inline_all_resources, inline_resources_from_info};
pub use types::{InliningError, InliningResult, ResourceType};
pub use downloaders::InlineConfig;
pub use processors::{process_css_links, process_images, process_svgs};
pub use utils::resolve_url;
