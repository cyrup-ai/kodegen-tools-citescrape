//! MCP manager implementations for session tracking, search caching, and manifest persistence
//!
//! This module provides the infrastructure for managing active crawl sessions,
//! caching search engines, and persisting crawl metadata. It has been decomposed
//! into logical sub-modules for maintainability:
//!
//! - `timestamp_utils`: Lock-free timestamp conversion for cache entries
//! - `session_manager`: Active crawl session tracking with cleanup
//! - `search_cache`: Search engine caching with LRU eviction
//! - `manifest_manager`: Atomic manifest file persistence
//! - `path_utils`: URL to filesystem path conversion

// Module declarations
mod timestamp_utils;
mod session_manager;
mod search_cache;
mod manifest_manager;
mod path_utils;

// Re-export types from parent module for internal use
use super::types;

// Re-export all public APIs
pub use session_manager::CrawlSessionManager;
pub use search_cache::{SearchEngineCache, SearchEngineCacheEntry};
pub use manifest_manager::ManifestManager;
pub use path_utils::url_to_output_dir;
