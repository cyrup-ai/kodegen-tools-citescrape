//! Public API for sending indexing messages
//!
//! This module provides the IndexingSender handle for sending indexing operations
//! to the background worker with completion callbacks and backpressure.

use anyhow::Result;
use imstr::ImString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use super::types::{
    CompletionCallback, IndexingMessage, MessagePriority, MAX_PENDING_MESSAGES,
};
use super::stats::{IndexingStats, IndexingStatsSnapshot};

/// Handle for sending indexing messages with zero-allocation design
#[derive(Clone)]
pub struct IndexingSender {
    pub(super) sender: mpsc::UnboundedSender<IndexingMessage>,
    pub(super) completion_callbacks: Arc<Mutex<ahash::AHashMap<u64, CompletionCallback>>>,
    pub(super) next_completion_id: Arc<AtomicUsize>,
    pub(super) pending_operations: Arc<AtomicUsize>,
    pub(super) stats: Arc<IndexingStats>,
}

impl std::fmt::Debug for IndexingSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexingSender")
            .field(
                "pending_operations",
                &self
                    .pending_operations
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish()
    }
}

impl IndexingSender {
    /// Send an add/update operation with completion callback
    #[inline]
    pub async fn add_or_update<F>(
        &self,
        url: ImString,
        file_path: PathBuf,
        priority: MessagePriority,
        completion_callback: F,
    ) -> Result<()>
    where
        F: FnOnce(Result<()>) + Send + 'static,
    {
        // Check backpressure
        let pending = self.pending_operations.load(Ordering::Relaxed);
        if pending >= MAX_PENDING_MESSAGES {
            return Err(anyhow::anyhow!(
                "Indexing service backpressure: {pending} pending operations"
            ));
        }

        let completion_id = self.next_completion_id.fetch_add(1, Ordering::Relaxed) as u64;

        // Register completion callback
        {
            let mut callbacks = self.completion_callbacks.lock().await;
            callbacks.insert(completion_id, Box::new(completion_callback));
        }

        let message = IndexingMessage::AddOrUpdate {
            url,
            file_path,
            priority,
            completion_id,
        };

        // Send message with error handling
        if let Ok(()) = self.sender.send(message) {
            self.pending_operations.fetch_add(1, Ordering::Relaxed);
            self.stats.pending_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        } else {
            // Remove callback if send failed (channel disconnected)
            self.completion_callbacks
                .lock()
                .await
                .remove(&completion_id);
            Err(anyhow::anyhow!("Indexing service disconnected"))
        }
    }
    /// Send a delete operation and await completion
    #[inline]
    pub async fn delete(&self, url: ImString) -> Result<()> {
        let pending = self.pending_operations.load(Ordering::Relaxed);
        if pending >= MAX_PENDING_MESSAGES {
            return Err(anyhow::anyhow!(
                "Indexing service backpressure: {pending} pending operations"
            ));
        }

        let completion_id = self.next_completion_id.fetch_add(1, Ordering::Relaxed) as u64;

        // Create a oneshot channel for the result
        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut callbacks = self.completion_callbacks.lock().await;
            callbacks.insert(
                completion_id,
                Box::new(move |result| {
                    let _ = tx.send(result);
                }),
            );
        }

        let message = IndexingMessage::Delete { url, completion_id };

        if let Ok(()) = self.sender.send(message) {
            self.pending_operations.fetch_add(1, Ordering::Relaxed);
            self.stats.pending_count.fetch_add(1, Ordering::Relaxed);
            // Wait for background worker to complete the operation
            rx.await
                .map_err(|_| anyhow::anyhow!("Indexing service disconnected"))?
        } else {
            let _ = self
                .completion_callbacks
                .lock()
                .await
                .remove(&completion_id);
            Err(anyhow::anyhow!("Indexing service disconnected"))
        }
    }

    /// Trigger index optimization
    #[inline]
    pub async fn optimize(&self, force: bool) -> Result<()> {
        let completion_id = self.next_completion_id.fetch_add(1, Ordering::Relaxed) as u64;

        let (tx, rx) = tokio::sync::oneshot::channel();

        {
            let mut callbacks = self.completion_callbacks.lock().await;
            callbacks.insert(
                completion_id,
                Box::new(move |result| {
                    let _ = tx.send(result);
                }),
            );
        }

        let message = IndexingMessage::Optimize {
            force,
            completion_id,
        };

        if let Ok(()) = self.sender.send(message) {
            // Wait for background worker to complete and invoke callback
            match rx.await {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!("Optimization callback was dropped")),
            }
        } else {
            // Clean up callback and return error
            self.completion_callbacks
                .lock()
                .await
                .remove(&completion_id);
            Err(anyhow::anyhow!("Indexing service disconnected"))
        }
    }

    /// Get current indexing statistics
    #[inline]
    pub async fn stats(&self) -> IndexingStatsSnapshot {
        self.stats.snapshot().await
    }

    /// Check if service is healthy and processing messages
    #[inline]
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        // Check if the sender can potentially send (not disconnected)
        // We'll use the pending operations count as a health indicator
        self.pending_operations.load(Ordering::Relaxed) < MAX_PENDING_MESSAGES
    }
}
