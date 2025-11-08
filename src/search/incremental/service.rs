//! Background worker service for incremental indexing
//!
//! This module implements the background worker loop that processes indexing messages
//! in batches with deduplication, retry logic, and error handling.

use anyhow::Result;
use imstr::ImString;
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::search::engine::SearchEngine;

use super::types::{
    CompletionCallback, IndexingMessage, MessagePriority, 
    DEFAULT_BATCH_SIZE, MAX_BATCH_WAIT_MS, MAX_RETRIES,
};
use super::stats::IndexingStats;
use super::sender::IndexingSender;

/// Incremental indexing service with lock-free coordination
pub struct IncrementalIndexingService {
    sender: mpsc::UnboundedSender<IndexingMessage>,
    completion_callbacks: Arc<Mutex<ahash::AHashMap<u64, CompletionCallback>>>,
    next_completion_id: Arc<AtomicUsize>,
    pending_operations: Arc<AtomicUsize>,
    is_running: Arc<AtomicBool>,
    stats: Arc<IndexingStats>,
}

impl IncrementalIndexingService {
    /// Create and start the incremental indexing service
    pub async fn start(engine: SearchEngine) -> Result<(IncrementalIndexingService, IndexingSender)> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let completion_callbacks = Arc::new(Mutex::new(ahash::AHashMap::with_capacity(1024)));
        let next_completion_id = Arc::new(AtomicUsize::new(1));
        let pending_operations = Arc::new(AtomicUsize::new(0));
        let is_running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(IndexingStats::new());

        // Start background worker task
        let worker_callbacks = completion_callbacks.clone();
        let worker_pending = pending_operations.clone();
        let worker_running = is_running.clone();
        let worker_stats = stats.clone();

        tokio::spawn(async move {
            Self::worker_loop(
                engine,
                receiver,
                worker_callbacks,
                worker_pending,
                worker_running,
                worker_stats,
            )
            .await;
        });

        let service = IncrementalIndexingService {
            sender: sender.clone(),
            completion_callbacks: completion_callbacks.clone(),
            next_completion_id: next_completion_id.clone(),
            pending_operations: pending_operations.clone(),
            is_running,
            stats: stats.clone(),
        };

        let sender = IndexingSender {
            sender,
            completion_callbacks,
            next_completion_id,
            pending_operations,
            stats,
        };

        Ok((service, sender))
    }

    /// Background worker loop with batching and error handling
    async fn worker_loop(
        engine: SearchEngine,
        mut receiver: mpsc::UnboundedReceiver<IndexingMessage>,
        completion_callbacks: Arc<Mutex<ahash::AHashMap<u64, CompletionCallback>>>,
        pending_operations: Arc<AtomicUsize>,
        is_running: Arc<AtomicBool>,
        stats: Arc<IndexingStats>,
    ) {
        let mut message_batch: SmallVec<[IndexingMessage; DEFAULT_BATCH_SIZE]> = SmallVec::new();
        let mut last_batch_time = Instant::now();

        // Process messages with batching for optimal performance
        while is_running.load(Ordering::Relaxed) {
            let batch_timeout = Duration::from_millis(MAX_BATCH_WAIT_MS);
            let should_process_batch = message_batch.len() >= DEFAULT_BATCH_SIZE
                || (!message_batch.is_empty() && last_batch_time.elapsed() >= batch_timeout);

            if should_process_batch {
                // Process current batch
                Self::process_message_batch(
                    &engine,
                    &mut message_batch,
                    &completion_callbacks,
                    &pending_operations,
                    &stats,
                )
                .await;

                last_batch_time = Instant::now();
                continue;
            }

            // Try to receive messages with timeout
            match tokio::time::timeout(Duration::from_millis(10), receiver.recv()).await {
                Ok(Some(IndexingMessage::Shutdown)) => {
                    // Process final batch before shutdown
                    if !message_batch.is_empty() {
                        Self::process_message_batch(
                            &engine,
                            &mut message_batch,
                            &completion_callbacks,
                            &pending_operations,
                            &stats,
                        )
                        .await;
                    }
                    break;
                }
                Ok(Some(message)) => {
                    message_batch.push(message);
                }
                Ok(None) => {
                    // Channel closed (sender dropped)
                    if !message_batch.is_empty() {
                        Self::process_message_batch(
                            &engine,
                            &mut message_batch,
                            &completion_callbacks,
                            &pending_operations,
                            &stats,
                        )
                        .await;
                    }
                    break;
                }
                Err(_) => {
                    // Timeout - check if we should process partial batch due to timeout
                    continue;
                }
            }
        }

        is_running.store(false, Ordering::Relaxed);
    }

    /// Process a batch of messages with deduplication and error handling
    async fn process_message_batch(
        engine: &SearchEngine,
        message_batch: &mut SmallVec<[IndexingMessage; DEFAULT_BATCH_SIZE]>,
        completion_callbacks: &Arc<Mutex<ahash::AHashMap<u64, CompletionCallback>>>,
        pending_operations: &Arc<AtomicUsize>,
        stats: &Arc<IndexingStats>,
    ) {
        if message_batch.is_empty() {
            return;
        }

        // Sort messages by priority for optimal processing order
        message_batch.sort_unstable_by(|a, b| {
            let priority_a = match a {
                IndexingMessage::AddOrUpdate { priority, .. } => *priority,
                IndexingMessage::Delete { .. } => MessagePriority::High,
                IndexingMessage::Optimize { force, .. } => {
                    if *force {
                        MessagePriority::Critical
                    } else {
                        MessagePriority::Low
                    }
                }
                IndexingMessage::Shutdown => MessagePriority::Critical,
            };
            let priority_b = match b {
                IndexingMessage::AddOrUpdate { priority, .. } => *priority,
                IndexingMessage::Delete { .. } => MessagePriority::High,
                IndexingMessage::Optimize { force, .. } => {
                    if *force {
                        MessagePriority::Critical
                    } else {
                        MessagePriority::Low
                    }
                }
                IndexingMessage::Shutdown => MessagePriority::Critical,
            };
            priority_b.cmp(&priority_a) // Higher priority first
        });

        // Deduplicate URLs within batch (keep latest operation per URL)
        // Track the LAST index for each URL
        let mut url_last_index: ahash::AHashMap<ImString, usize> =
            ahash::AHashMap::with_capacity(message_batch.len());

        // Single forward pass to find last occurrence of each URL
        for (idx, message) in message_batch.iter().enumerate() {
            match message {
                IndexingMessage::AddOrUpdate { url, .. } | IndexingMessage::Delete { url, .. } => {
                    url_last_index.insert(url.clone(), idx);
                }
                _ => {}
            }
        }

        // Single pass filter keeping only last occurrences, maintaining order
        let mut duplicate_completions = Vec::new();
        let deduplicated_batch: SmallVec<[IndexingMessage; DEFAULT_BATCH_SIZE]> = message_batch
            .drain(..)
            .enumerate()
            .filter_map(|(idx, message)| {
                match &message {
                    IndexingMessage::AddOrUpdate { url, .. }
                    | IndexingMessage::Delete { url, .. } => {
                        // Keep if this is the last occurrence of this URL
                        if url_last_index.get(url) == Some(&idx) {
                            Some(message)
                        } else {
                            // Mark duplicate for completion with no-op
                            if let Some(id) = Self::extract_completion_id(&message) {
                                duplicate_completions.push(id);
                            }
                            None
                        }
                    }
                    IndexingMessage::Optimize { .. } | IndexingMessage::Shutdown => Some(message),
                }
            })
            .collect();

        // Complete all duplicate operations
        for completion_id in duplicate_completions {
            Self::complete_operation(
                completion_id,
                Ok(()),
                completion_callbacks,
                pending_operations,
            )
            .await;
        }

        // Process deduplicated batch
        let writer_opt = match engine.writer_with_retry(Some(64 * 1024 * 1024)).await {
            Ok(w) => Some(w),
            Err(e) => {
                // Complete all operations with error
                for message in &deduplicated_batch {
                    if let Some(completion_id) = Self::extract_completion_id(message) {
                        Self::complete_operation(
                            completion_id,
                            Err(anyhow::anyhow!("Failed to create index writer: {e}")),
                            completion_callbacks,
                            pending_operations,
                        )
                        .await;
                    }
                }
                stats
                    .total_failed
                    .fetch_add(deduplicated_batch.len(), Ordering::Relaxed);
                return;
            }
        };

        let mut successful_operations = 0;
        let mut failed_operations = 0;

        // Separate optimize messages from regular operations
        let (optimize_messages, regular_messages): (Vec<_>, Vec<_>) = deduplicated_batch
            .into_iter()
            .partition(|msg| matches!(msg, IndexingMessage::Optimize { .. }));

        // Extract writer from Option for regular operations
        let mut writer = match writer_opt {
            Some(w) => w,
            None => {
                // Writer creation failed earlier, all operations already completed with error
                return;
            }
        };

        // Process regular messages (Delete, AddOrUpdate) with retry logic
        for message in regular_messages {
            let completion_id = Self::extract_completion_id(&message);
            let mut retry_count = 0;
            let mut last_error = None;

            // Retry loop for transient failures
            loop {
                let result = match &message {
                    IndexingMessage::AddOrUpdate { url, file_path, .. } => {
                        // Use batch indexing with existing writer (avoids double writer acquisition)
                        super::super::indexer::index_single_file_sync(
                            engine,
                            &mut writer,
                            file_path.as_path(),
                            url,
                            &super::super::indexer::IndexingLimits::default(),
                        )
                        .map_err(|e| anyhow::anyhow!("{e}"))
                    }
                    IndexingMessage::Delete { url, .. } => engine
                        .delete_document(&mut writer, url.to_string())
                        .map_err(|e| anyhow::anyhow!("{e}")),
                    IndexingMessage::Shutdown => Ok(()), // No-op, handled by worker loop
                    IndexingMessage::Optimize { .. } => {
                        unreachable!("Optimize messages filtered out")
                    }
                };

                match result {
                    Ok(()) => {
                        successful_operations += 1;
                        if let Some(id) = completion_id {
                            Self::complete_operation(
                                id,
                                Ok(()),
                                completion_callbacks,
                                pending_operations,
                            )
                            .await;
                        }
                        break;
                    }
                    Err(e) if retry_count < MAX_RETRIES && Self::is_retryable_error(&e) => {
                        retry_count += 1;
                        last_error = Some(e);

                        // Exponential backoff: 10ms, 20ms, 40ms
                        let delay_ms = 10 * (1 << retry_count);
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                    Err(e) => {
                        failed_operations += 1;
                        let error = last_error.unwrap_or(e);
                        if let Some(id) = completion_id {
                            Self::complete_operation(
                                id,
                                Err(error),
                                completion_callbacks,
                                pending_operations,
                            )
                            .await;
                        }
                        break;
                    }
                }
            }
        }

        // Commit and optimize once after all operations
        match engine.commit_and_optimize(writer).await {
            Ok(_w) => {
                // Writer successfully committed and optimized
                // Update optimization timestamp if there were explicit optimize requests
                if !optimize_messages.is_empty() {
                    *stats.last_optimization.lock().await = Some(Instant::now());
                }

                // Complete all optimize operation callbacks
                for msg in optimize_messages {
                    if let Some(id) = Self::extract_completion_id(&msg) {
                        Self::complete_operation(
                            id,
                            Ok(()),
                            completion_callbacks,
                            pending_operations,
                        )
                        .await;
                        successful_operations += 1;
                    }
                }
            }
            Err(e) => {
                // Commit failed - complete optimize operations with error
                for msg in optimize_messages {
                    if let Some(id) = Self::extract_completion_id(&msg) {
                        Self::complete_operation(
                            id,
                            Err(anyhow::anyhow!("Failed to commit and optimize: {e}")),
                            completion_callbacks,
                            pending_operations,
                        )
                        .await;
                        failed_operations += 1;
                    }
                }
            }
        }

        // Update statistics
        stats
            .total_processed
            .fetch_add(successful_operations, Ordering::Relaxed);
        stats
            .total_failed
            .fetch_add(failed_operations, Ordering::Relaxed);
        stats.batch_count.fetch_add(1, Ordering::Relaxed);
        stats
            .pending_count
            .fetch_sub(successful_operations + failed_operations, Ordering::Relaxed);
    }

    /// Extract completion ID from message
    #[inline]
    fn extract_completion_id(message: &IndexingMessage) -> Option<u64> {
        match message {
            IndexingMessage::AddOrUpdate { completion_id, .. } => Some(*completion_id),
            IndexingMessage::Delete { completion_id, .. } => Some(*completion_id),
            IndexingMessage::Optimize { completion_id, .. } => Some(*completion_id),
            IndexingMessage::Shutdown => None,
        }
    }

    /// Complete an operation by calling its callback
    #[inline]
    async fn complete_operation(
        completion_id: u64,
        result: Result<()>,
        completion_callbacks: &Arc<Mutex<ahash::AHashMap<u64, CompletionCallback>>>,
        pending_operations: &Arc<AtomicUsize>,
    ) {
        if let Some(callback) = completion_callbacks.lock().await.remove(&completion_id) {
            callback(result);
            pending_operations.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Check if error is retryable (transient failure)
    #[inline]
    fn is_retryable_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("lock")
            || error_str.contains("busy")
            || error_str.contains("timeout")
            || error_str.contains("temporary")
            || error_str.contains("resource temporarily unavailable")
    }

    /// Check if the service is running
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    /// Get current count of pending operations
    pub fn pending_operations(&self) -> usize {
        self.pending_operations.load(Ordering::Relaxed)
    }

    /// Get indexing statistics
    pub fn stats(&self) -> Arc<IndexingStats> {
        Arc::clone(&self.stats)
    }

    /// Shutdown the service gracefully
    pub async fn shutdown(&self) -> Result<()> {
        self.sender
            .send(IndexingMessage::Shutdown)
            .map_err(|e| anyhow::anyhow!("Failed to send shutdown message: {e}"))?;
        Ok(())
    }

    /// Get count of pending completion callbacks
    pub async fn pending_callbacks(&self) -> usize {
        self.completion_callbacks.lock().await.len()
    }

    /// Get next completion ID that will be assigned
    pub fn next_completion_id(&self) -> usize {
        self.next_completion_id.load(Ordering::Relaxed)
    }
}
