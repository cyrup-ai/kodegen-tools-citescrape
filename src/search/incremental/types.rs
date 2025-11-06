//! Message types and constants for incremental indexing
//!
//! This module defines the core message types, priorities, and constants used
//! by the incremental indexing service.

use anyhow::Result;
use imstr::ImString;
use std::path::PathBuf;

/// Maximum number of pending indexing messages before backpressure
pub const MAX_PENDING_MESSAGES: usize = 10000;

/// Default batch size for processing messages
pub const DEFAULT_BATCH_SIZE: usize = 50;

/// Maximum wait time before processing partial batch
pub const MAX_BATCH_WAIT_MS: u64 = 100;

/// Maximum number of retries for failed operations
pub const MAX_RETRIES: usize = 3;

/// Indexing operation message with zero-allocation design
#[derive(Debug, Clone)]
pub enum IndexingMessage {
    /// Add or update a document in the index
    AddOrUpdate {
        url: ImString,
        file_path: PathBuf,
        priority: MessagePriority,
        completion_id: u64,
    },
    /// Delete a document from the index by URL
    Delete { url: ImString, completion_id: u64 },
    /// Optimize the index (periodic maintenance)
    Optimize { force: bool, completion_id: u64 },
    /// Shutdown the indexing service gracefully
    Shutdown,
}

/// Message priority for processing order
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Completion callback registry for tracking operation results
pub type CompletionCallback = Box<dyn FnOnce(Result<()>) + Send + 'static>;
