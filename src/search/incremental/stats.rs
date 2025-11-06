//! Lock-free statistics tracking for incremental indexing
//!
//! This module provides atomic statistics tracking without locks for high-performance
//! monitoring of the indexing service.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use tokio::sync::Mutex;

/// Lock-free indexing statistics
#[derive(Debug)]
pub struct IndexingStats {
    pub total_processed: AtomicUsize,
    pub total_failed: AtomicUsize,
    pub pending_count: AtomicUsize,
    pub batch_count: AtomicUsize,
    pub last_optimization: Arc<Mutex<Option<Instant>>>,
}

impl IndexingStats {
    #[inline]
    pub fn new() -> Self {
        Self {
            total_processed: AtomicUsize::new(0),
            total_failed: AtomicUsize::new(0),
            pending_count: AtomicUsize::new(0),
            batch_count: AtomicUsize::new(0),
            last_optimization: Arc::new(Mutex::new(None)),
        }
    }

    /// Get snapshot of current statistics
    #[inline]
    pub async fn snapshot(&self) -> IndexingStatsSnapshot {
        IndexingStatsSnapshot {
            total_processed: self.total_processed.load(Ordering::Relaxed),
            total_failed: self.total_failed.load(Ordering::Relaxed),
            pending_count: self.pending_count.load(Ordering::Relaxed),
            batch_count: self.batch_count.load(Ordering::Relaxed),
            last_optimization: *self.last_optimization.lock().await,
        }
    }
}

impl Default for IndexingStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Immutable snapshot of indexing statistics
#[derive(Debug, Clone)]
pub struct IndexingStatsSnapshot {
    pub total_processed: usize,
    pub total_failed: usize,
    pub pending_count: usize,
    pub batch_count: usize,
    pub last_optimization: Option<Instant>,
}
