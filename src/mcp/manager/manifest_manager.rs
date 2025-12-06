//! Manifest persistence with atomic writes for crawl metadata
//!
//! Provides safe file operations using write-to-temp-then-rename pattern
//! to prevent corruption from crashes or interrupted writes.

use super::types::CrawlManifest;
use kodegen_mcp_schema::McpError;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Manager for crawl manifest persistence
///
/// Provides atomic file writes to prevent corruption.
/// Uses write-to-temp-then-rename pattern for atomicity.
pub struct ManifestManager;

impl ManifestManager {
    const MANIFEST_FILENAME: &'static str = "manifest.json";

    /// Save manifest atomically to {`output_dir}/manifest.json`
    ///
    /// Uses atomic write pattern: write to temp file, sync, rename
    pub async fn save(manifest: &CrawlManifest) -> Result<(), McpError> {
        let manifest_path = manifest.output_dir.join(Self::MANIFEST_FILENAME);

        // Ensure output directory exists
        if let Some(parent) = manifest_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                McpError::Manifest(format!("Failed to create manifest directory: {e}"))
            })?;
        }

        // Serialize to JSON with pretty formatting
        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| McpError::Manifest(format!("Failed to serialize manifest: {e}")))?;

        // Atomic write pattern: temp file + rename
        let temp_path = manifest_path.with_extension("json.tmp");

        let mut file = fs::File::create(&temp_path)
            .await
            .map_err(|e| McpError::Manifest(format!("Failed to create temp manifest file: {e}")))?;

        file.write_all(json.as_bytes())
            .await
            .map_err(|e| McpError::Manifest(format!("Failed to write manifest: {e}")))?;

        // Sync to disk before rename (ensures durability)
        file.sync_all()
            .await
            .map_err(|e| McpError::Manifest(format!("Failed to sync manifest to disk: {e}")))?;

        // Atomic rename (overwrites existing file)
        fs::rename(&temp_path, &manifest_path)
            .await
            .map_err(|e| McpError::Manifest(format!("Failed to rename manifest file: {e}")))?;

        Ok(())
    }

    /// Load manifest from {`output_dir}/manifest.json`
    pub async fn load(output_dir: &Path) -> Result<CrawlManifest, McpError> {
        let manifest_path = output_dir.join(Self::MANIFEST_FILENAME);

        if !manifest_path.exists() {
            return Err(McpError::Manifest(format!(
                "Manifest not found at {manifest_path:?}"
            )));
        }

        let contents = fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| McpError::Manifest(format!("Failed to read manifest: {e}")))?;

        let manifest: CrawlManifest = serde_json::from_str(&contents)
            .map_err(|e| McpError::Manifest(format!("Failed to parse manifest JSON: {e}")))?;

        Ok(manifest)
    }

    /// Check if manifest exists for `output_dir`
    pub async fn exists(output_dir: &Path) -> bool {
        let manifest_path = output_dir.join(Self::MANIFEST_FILENAME);
        manifest_path.exists()
    }
}
