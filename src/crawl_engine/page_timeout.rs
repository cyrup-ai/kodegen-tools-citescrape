//! Timeout utilities for page operations
//!
//! Provides async timeout wrappers to prevent indefinite hangs during
//! page navigation, loading, and other browser operations.

use anyhow::Result;
use std::future::Future;
use std::time::Duration;

/// Helper function to wrap async page operations with explicit timeout
///
/// Prevents indefinite hangs on page operations by applying `tokio::time::timeout`.
/// Returns proper error messages distinguishing between timeout and operation failures.
///
/// # Arguments
/// * `operation` - The async Future to execute with a timeout
/// * `timeout_secs` - Timeout duration in seconds
/// * `operation_name` - Human-readable name for error messages
///
/// # Returns
/// * `Ok(T)` - Operation completed successfully
/// * `Err` - Either the operation failed or the timeout was reached
pub async fn with_page_timeout<F, T>(
    operation: F,
    timeout_secs: u64,
    operation_name: &str,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match tokio::time::timeout(Duration::from_secs(timeout_secs), operation).await {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!(
            "{operation_name} timeout after {timeout_secs} seconds"
        )),
    }
}
