//! Incremental indexing service with async tokio messaging
//!
//! This module provides a high-performance, lock-free incremental indexing service
//! that processes document updates, additions, and deletions in the background.
//!
//! Optimized for:
//! - Zero allocation via message pooling and buffer reuse
//! - Blazing-fast performance via batching and lock-free operations
//! - Async concurrency via tokio mpsc channels and atomic coordination
//! - Elegant ergonomic API with seamless integration
//!
//! # Architecture
//!
//! The incremental indexing system is decomposed into logical modules:
//!
//! - `types` - Core message types, enums, and constants
//! - `stats` - Lock-free statistics tracking
//! - `sender` - Public API for sending indexing messages
//! - `service` - Background worker loop with batching and deduplication
//!
//! # Example
//!
//! ```ignore
//! use crate::search::engine::SearchEngine;
//! use crate::search::incremental::{IncrementalIndexingService, MessagePriority};
//!
//! let engine = SearchEngine::new(index_path).await?;
//! let sender = IncrementalIndexingService::start(engine).await?;
//!
//! // Add document to index
//! sender.add_or_update(
//!     url.into(),
//!     file_path,
//!     MessagePriority::Normal,
//!     |result| {
//!         // Handle completion
//!     }
//! ).await?;
//! ```

mod types;
mod stats;
mod sender;
mod service;

// Re-export public API
pub use types::{
    IndexingMessage, 
    MessagePriority, 
    CompletionCallback,
    MAX_PENDING_MESSAGES,
    DEFAULT_BATCH_SIZE,
    MAX_BATCH_WAIT_MS,
    MAX_RETRIES,
};
pub use stats::{IndexingStats, IndexingStatsSnapshot};
pub use sender::IndexingSender;
pub use service::IncrementalIndexingService;

// Tests module
#[cfg(test)]
mod tests;
