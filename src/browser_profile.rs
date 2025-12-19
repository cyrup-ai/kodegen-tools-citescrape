//! Centralized Chrome profile directory management
//!
//! Eliminates SingletonLock conflicts via UUID-based naming + stale lock detection.
//! All browser launch points MUST use this module for profile directory creation.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use uuid::Uuid;

// =============================================================================
// BrowserProfile - RAII wrapper for profile directory
// =============================================================================

/// RAII wrapper for Chrome profile directory
///
/// Automatically cleans up the profile directory on drop unless `into_path()` is called.
/// This ensures orphaned profile directories don't accumulate in temp.
#[derive(Debug)]
pub struct BrowserProfile {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl BrowserProfile {
    /// Create a new BrowserProfile with the given path
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleanup_on_drop: true,
        }
    }

    /// Get reference to the profile directory path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consume the BrowserProfile and return the path, disabling auto-cleanup
    ///
    /// Use this when transferring ownership to another cleanup mechanism
    /// (e.g., BrowserWrapper, PooledBrowserWrapper).
    pub fn into_path(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        std::mem::take(&mut self.path)
    }

    /// Disable auto-cleanup without consuming self
    #[allow(dead_code)]
    pub fn disable_cleanup(&mut self) {
        self.cleanup_on_drop = false;
    }
}

impl Drop for BrowserProfile {
    fn drop(&mut self) {
        if self.cleanup_on_drop && self.path.exists() {
            info!("BrowserProfile cleanup: removing {}", self.path.display());
            if let Err(e) = std::fs::remove_dir_all(&self.path) {
                warn!("Failed to cleanup profile directory {}: {}", self.path.display(), e);
            }
        }
    }
}

// =============================================================================
// Profile Creation - UUID-based unique directories
// =============================================================================

/// Create a unique Chrome profile directory using UUID v4
///
/// This is the PRIMARY method for profile creation. All browser launch code
/// should call this function instead of constructing paths manually.
///
/// # Returns
/// `BrowserProfile` with auto-cleanup enabled. Call `into_path()` to transfer
/// ownership to another cleanup mechanism.
///
/// # Example
/// ```
/// # use anyhow::Result;
/// # fn main() -> Result<()> {
/// use kodegen_tools_citescrape::browser_profile::create_unique_profile;
/// 
/// // Create a unique Chrome profile directory
/// let profile = create_unique_profile()?;
/// 
/// // Verify the profile directory was created
/// assert!(profile.path().exists());
/// assert!(profile.path().to_string_lossy().contains("kodegen_chrome_"));
/// 
/// // Profile is automatically cleaned up when dropped
/// # Ok(())
/// # }
/// ```
pub fn create_unique_profile() -> Result<BrowserProfile> {
    let uuid = Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("kodegen_chrome_{}", uuid));

    debug!("Creating unique Chrome profile: {}", path.display());

    // Use create_dir (not create_dir_all) for atomic creation
    // This fails if directory exists, providing defense against UUID collision
    std::fs::create_dir(&path)
        .with_context(|| format!("Failed to create profile directory: {}", path.display()))?;

    info!("Created Chrome profile directory: {}", path.display());
    Ok(BrowserProfile::new(path))
}

/// Create a unique profile with a custom prefix
///
/// Useful for distinguishing between different subsystems (pool, web_search, etc.)
pub fn create_unique_profile_with_prefix(prefix: &str) -> Result<BrowserProfile> {
    let uuid = Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid));

    debug!("Creating unique Chrome profile with prefix '{}': {}", prefix, path.display());

    std::fs::create_dir(&path)
        .with_context(|| format!("Failed to create profile directory: {}", path.display()))?;

    info!("Created Chrome profile directory: {}", path.display());
    Ok(BrowserProfile::new(path))
}

// =============================================================================
// Stale Lock Detection - Unix/macOS implementation
// =============================================================================

/// Check if a SingletonLock file is stale (Chrome process no longer running)
///
/// SingletonLock is a symlink with target `{hostname}-{PID}`.
/// We parse the PID and check if that process still exists.
///
/// # Arguments
/// * `profile_dir` - Path to Chrome profile directory
///
/// # Returns
/// * `true` if lock is stale (safe to reuse/delete)
/// * `false` if lock is active or cannot be determined
#[cfg(unix)]
pub fn is_singleton_lock_stale(profile_dir: &Path) -> bool {
    let lock_path = profile_dir.join("SingletonLock");

    // If lock doesn't exist, directory is available
    if !lock_path.exists() && !lock_path.is_symlink() {
        return true;
    }

    match std::fs::read_link(&lock_path) {
        Ok(target) => {
            let target_str = target.to_string_lossy();
            debug!("SingletonLock target: {}", target_str);

            // Parse PID from "hostname-PID" format
            if let Some(pid_str) = target_str.rsplit('-').next()
                && let Ok(pid) = pid_str.parse::<i32>()
            {
                // Check if process exists using kill(pid, 0)
                // Returns 0 if process exists (and we have permission to signal it)
                // Returns -1 with ESRCH if process doesn't exist
                let exists = unsafe { libc::kill(pid, 0) == 0 };
                if !exists {
                    info!("SingletonLock is stale: PID {} no longer exists", pid);
                    return true;
                }
                debug!("SingletonLock is active: PID {} is running", pid);
                return false;
            }
            // Couldn't parse PID, assume lock is active to be safe
            warn!("Could not parse PID from SingletonLock target: {}", target_str);
            false
        }
        Err(e) => {
            // Not a symlink or read error - might be corrupted
            debug!("Could not read SingletonLock as symlink: {}", e);
            // If it's not a symlink but exists, it might be corrupted - treat as stale
            lock_path.is_file()
        }
    }
}

/// Non-Unix fallback - always returns true (assumes stale)
#[cfg(not(unix))]
pub fn is_singleton_lock_stale(_profile_dir: &Path) -> bool {
    // On non-Unix platforms, we don't have reliable process checking
    // UUID-based naming should prevent conflicts anyway
    true
}

/// Remove stale SingletonLock file from a profile directory
///
/// # Safety
/// Only call this after `is_singleton_lock_stale()` returns true!
#[allow(dead_code)]
pub fn cleanup_stale_lock(profile_dir: &Path) -> Result<()> {
    let lock_path = profile_dir.join("SingletonLock");

    // Check both exists() and is_symlink() - broken symlinks return false for exists()
    if lock_path.exists() || lock_path.is_symlink() {
        info!("Removing stale SingletonLock: {}", lock_path.display());
        
        // std::fs::remove_file works for both symlinks and regular files
        std::fs::remove_file(&lock_path)
            .with_context(|| format!("Failed to remove SingletonLock: {}", lock_path.display()))?;
    }

    Ok(())
}

/// Clean up all stale kodegen Chrome profiles in temp directory
///
/// Maintenance function that can be called at startup to clean
/// orphaned profile directories from previous crashes.
#[allow(dead_code)]
pub fn cleanup_stale_profiles() -> Result<usize> {
    let temp_dir = std::env::temp_dir();
    let mut cleaned = 0;

    let entries = std::fs::read_dir(&temp_dir)
        .with_context(|| format!("Failed to read temp directory: {}", temp_dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        
        // Only process kodegen Chrome directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.starts_with("kodegen_chrome_")
            && path.is_dir()
            && is_singleton_lock_stale(&path)
        {
            info!("Cleaning stale profile: {}", path.display());
            if let Err(e) = std::fs::remove_dir_all(&path) {
                warn!("Failed to remove stale profile {}: {}", path.display(), e);
            } else {
                cleaned += 1;
            }
        }
    }

    if cleaned > 0 {
        info!("Cleaned {} stale Chrome profile directories", cleaned);
    }

    Ok(cleaned)
}
