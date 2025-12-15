use anyhow::Result;
use std::time::Duration;
use tokio::time::timeout;

use crate::utils::{ensure_domain_gitignore, get_mirror_path};

use super::compression::save_compressed_file;

/// Timeout for blocking JSON serialization
/// Prevents hangs on pathological data structures
const BLOCKING_SERIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Save JSON data
pub async fn save_json_data(
    data: serde_json::Value,
    url: String,
    output_dir: std::path::PathBuf,
    compression_threshold: usize,
) -> Result<()> {
    let path = get_mirror_path(&url, &output_dir, "index.json").await?;

    // Ensure .gitignore exists in domain directory
    ensure_domain_gitignore(&path, &output_dir).await?;

    // JSON serialization (keep spawn_blocking - CPU intensive)
    let blocking_task = tokio::task::spawn_blocking(move || serde_json::to_string_pretty(&data));

    let json_str = match timeout(BLOCKING_SERIALIZATION_TIMEOUT, blocking_task).await {
        Ok(Ok(result)) => result?,
        Ok(Err(e)) => return Err(anyhow::anyhow!("JSON serialization task panicked: {}", e)),
        Err(_) => {
            log::warn!("JSON serialization timeout (timeout: {:?})", BLOCKING_SERIALIZATION_TIMEOUT);
            return Err(anyhow::anyhow!(
                "JSON serialization timed out after {:?} - data structure may be pathological",
                BLOCKING_SERIALIZATION_TIMEOUT
            ));
        }
    };

    // Create directory
    tokio::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Path has no parent directory"))?,
    )
    .await?;

    // save_compressed_file is now async
    let (_saved_path, _metadata) = save_compressed_file(
        json_str.into_bytes(),
        &path,
        "application/json",
        false,
        compression_threshold,
    )
    .await?;

    Ok(())
}

/// Save page data as JSON
pub async fn save_page_data(
    page_data: crate::page_extractor::schema::PageData,
    url: String,
    output_dir: std::path::PathBuf,
    compression_threshold: usize,
) -> Result<()> {
    let path = get_mirror_path(&url, &output_dir, "index.json").await?;

    // Ensure .gitignore exists in domain directory
    ensure_domain_gitignore(&path, &output_dir).await?;

    // Wrap in Arc for spawn_blocking (internal implementation detail)
    let page_data_arc = std::sync::Arc::new(page_data);

    // PageData serialization (keep spawn_blocking - CPU intensive)
    let blocking_task = 
        tokio::task::spawn_blocking(move || serde_json::to_string_pretty(&*page_data_arc));

    let json_content = match timeout(BLOCKING_SERIALIZATION_TIMEOUT, blocking_task).await {
        Ok(Ok(result)) => result?,
        Ok(Err(e)) => return Err(anyhow::anyhow!("PageData serialization task panicked: {}", e)),
        Err(_) => {
            log::warn!("PageData serialization timeout (timeout: {:?})", BLOCKING_SERIALIZATION_TIMEOUT);
            return Err(anyhow::anyhow!(
                "PageData serialization timed out after {:?} - data structure may be pathological",
                BLOCKING_SERIALIZATION_TIMEOUT
            ));
        }
    };

    // Create parent directory
    tokio::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Path has no parent directory"))?,
    )
    .await?;

    // save_compressed_file is now async
    let (_saved_path, _metadata) = save_compressed_file(
        json_content.into_bytes(),
        &path,
        "application/json",
        false,
        compression_threshold,
    )
    .await?;

    Ok(())
}
