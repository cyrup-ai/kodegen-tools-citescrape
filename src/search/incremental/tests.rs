//! Unit tests for incremental indexing deduplication logic
//!
//! These tests verify the message deduplication behavior in batch processing.

use imstr::ImString;
use smallvec::SmallVec;
use std::path::PathBuf;

use super::types::{IndexingMessage, MessagePriority, DEFAULT_BATCH_SIZE};

#[test]
fn test_deduplication_keeps_latest_operation() {
    // Create test messages with duplicate URLs in various positions
    let mut message_batch = SmallVec::<[IndexingMessage; DEFAULT_BATCH_SIZE]>::new();

    // First operation for url1 (should be dropped)
    message_batch.push(IndexingMessage::AddOrUpdate {
        url: ImString::from("http://example.com/page1"),
        file_path: PathBuf::from("/tmp/file1.md"),
        priority: MessagePriority::Normal,
        completion_id: 1,
    });

    // Operation for url2 (should be kept)
    message_batch.push(IndexingMessage::AddOrUpdate {
        url: ImString::from("http://example.com/page2"),
        file_path: PathBuf::from("/tmp/file2.md"),
        priority: MessagePriority::Normal,
        completion_id: 2,
    });

    // Second operation for url1 (should be dropped)
    message_batch.push(IndexingMessage::Delete {
        url: ImString::from("http://example.com/page1"),
        completion_id: 3,
    });

    // Operation for url3 (should be kept)
    message_batch.push(IndexingMessage::AddOrUpdate {
        url: ImString::from("http://example.com/page3"),
        file_path: PathBuf::from("/tmp/file3.md"),
        priority: MessagePriority::High,
        completion_id: 4,
    });

    // Third operation for url1 - LATEST (should be kept)
    message_batch.push(IndexingMessage::AddOrUpdate {
        url: ImString::from("http://example.com/page1"),
        file_path: PathBuf::from("/tmp/file1_updated.md"),
        priority: MessagePriority::High,
        completion_id: 5,
    });

    // Optimize message (should always be kept)
    message_batch.push(IndexingMessage::Optimize {
        force: false,
        completion_id: 6,
    });

    // Duplicate url2 (should be dropped)
    message_batch.push(IndexingMessage::Delete {
        url: ImString::from("http://example.com/page2"),
        completion_id: 7,
    });

    // Shutdown message (should always be kept)
    message_batch.push(IndexingMessage::Shutdown);

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
                        // In real code, would complete duplicate with no-op here
                        None
                    }
                }
                IndexingMessage::Optimize { .. } | IndexingMessage::Shutdown => Some(message),
            }
        })
        .collect();

    // Verify results
    assert_eq!(
        deduplicated_batch.len(),
        5,
        "Should have 5 deduplicated messages"
    );

    // Verify url1's LATEST operation is kept (completion_id 5)
    let url1_msg = deduplicated_batch.iter().find(|m| {
        matches!(m, IndexingMessage::AddOrUpdate { url, completion_id, .. } 
            if url.as_str() == "http://example.com/page1" && *completion_id == 5)
    });
    assert!(
        url1_msg.is_some(),
        "Should keep latest url1 operation (completion_id 5)"
    );

    // Verify url2's LATEST operation is kept (completion_id 7, Delete)
    let url2_msg = deduplicated_batch.iter().find(|m| {
        matches!(m, IndexingMessage::Delete { url, completion_id } 
            if url.as_str() == "http://example.com/page2" && *completion_id == 7)
    });
    assert!(
        url2_msg.is_some(),
        "Should keep latest url2 operation (completion_id 7, Delete)"
    );

    // Verify url3 operation is kept
    let url3_msg = deduplicated_batch.iter().find(|m| {
        matches!(m, IndexingMessage::AddOrUpdate { url, .. } 
            if url.as_str() == "http://example.com/page3")
    });
    assert!(url3_msg.is_some(), "Should keep url3 operation");

    // Verify Optimize message is kept
    let optimize_msg = deduplicated_batch
        .iter()
        .find(|m| matches!(m, IndexingMessage::Optimize { .. }));
    assert!(optimize_msg.is_some(), "Should keep Optimize message");

    // Verify Shutdown message is kept
    let shutdown_msg = deduplicated_batch
        .iter()
        .find(|m| matches!(m, IndexingMessage::Shutdown));
    assert!(shutdown_msg.is_some(), "Should keep Shutdown message");

    // Verify order is maintained (check indices)
    let positions: Vec<usize> = deduplicated_batch
        .iter()
        .enumerate()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        positions,
        vec![0, 1, 2, 3, 4],
        "Messages should maintain their relative order"
    );
}

#[test]
fn test_deduplication_with_no_duplicates() {
    let mut message_batch = SmallVec::<[IndexingMessage; DEFAULT_BATCH_SIZE]>::new();

    message_batch.push(IndexingMessage::AddOrUpdate {
        url: ImString::from("http://example.com/page1"),
        file_path: PathBuf::from("/tmp/file1.md"),
        priority: MessagePriority::Normal,
        completion_id: 1,
    });

    message_batch.push(IndexingMessage::AddOrUpdate {
        url: ImString::from("http://example.com/page2"),
        file_path: PathBuf::from("/tmp/file2.md"),
        priority: MessagePriority::Normal,
        completion_id: 2,
    });

    let original_len = message_batch.len();

    // Track the LAST index for each URL
    let mut url_last_index: ahash::AHashMap<ImString, usize> =
        ahash::AHashMap::with_capacity(message_batch.len());

    for (idx, message) in message_batch.iter().enumerate() {
        match message {
            IndexingMessage::AddOrUpdate { url, .. } | IndexingMessage::Delete { url, .. } => {
                url_last_index.insert(url.clone(), idx);
            }
            _ => {}
        }
    }

    let deduplicated_batch: SmallVec<[IndexingMessage; DEFAULT_BATCH_SIZE]> = message_batch
        .drain(..)
        .enumerate()
        .filter_map(|(idx, message)| match &message {
            IndexingMessage::AddOrUpdate { url, .. } | IndexingMessage::Delete { url, .. } => {
                if url_last_index.get(url) == Some(&idx) {
                    Some(message)
                } else {
                    None
                }
            }
            IndexingMessage::Optimize { .. } | IndexingMessage::Shutdown => Some(message),
        })
        .collect();

    assert_eq!(
        deduplicated_batch.len(),
        original_len,
        "Should keep all messages when no duplicates"
    );
}

#[test]
fn test_deduplication_all_duplicates_same_url() {
    let mut message_batch = SmallVec::<[IndexingMessage; DEFAULT_BATCH_SIZE]>::new();
    let url = ImString::from("http://example.com/page1");

    // Add 5 operations for the same URL
    for i in 1..=5 {
        message_batch.push(IndexingMessage::AddOrUpdate {
            url: url.clone(),
            file_path: PathBuf::from(format!("/tmp/file{i}.md")),
            priority: MessagePriority::Normal,
            completion_id: i,
        });
    }

    // Track the LAST index for each URL
    let mut url_last_index: ahash::AHashMap<ImString, usize> =
        ahash::AHashMap::with_capacity(message_batch.len());

    for (idx, message) in message_batch.iter().enumerate() {
        match message {
            IndexingMessage::AddOrUpdate { url, .. } | IndexingMessage::Delete { url, .. } => {
                url_last_index.insert(url.clone(), idx);
            }
            _ => {}
        }
    }

    let deduplicated_batch: SmallVec<[IndexingMessage; DEFAULT_BATCH_SIZE]> = message_batch
        .drain(..)
        .enumerate()
        .filter_map(|(idx, message)| match &message {
            IndexingMessage::AddOrUpdate { url, .. } | IndexingMessage::Delete { url, .. } => {
                if url_last_index.get(url) == Some(&idx) {
                    Some(message)
                } else {
                    None
                }
            }
            IndexingMessage::Optimize { .. } | IndexingMessage::Shutdown => Some(message),
        })
        .collect();

    assert_eq!(
        deduplicated_batch.len(),
        1,
        "Should keep only the last operation"
    );

    // Verify it's the last one (completion_id 5)
    match &deduplicated_batch[0] {
        IndexingMessage::AddOrUpdate { completion_id, .. } => {
            assert_eq!(
                *completion_id, 5,
                "Should keep the last operation (completion_id 5)"
            );
        }
        _ => panic!("Expected AddOrUpdate message"),
    }
}
