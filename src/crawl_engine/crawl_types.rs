//! Core types and traits for web crawling operations.
//!
//! This module contains the fundamental types used throughout the crawler including
//! error types, progress indicators, and the main Crawler trait.

use anyhow::Result;
use std::fmt;

/// Custom error type for crawl operations
#[derive(Debug, Clone)]
pub enum CrawlError {
    /// Configuration error
    ConfigError(String),
    /// Browser error
    BrowserError(String),
    /// Network error
    NetworkError(String),
    /// Operation cancelled
    Cancelled,
    /// Other errors
    Other(String),
}

impl fmt::Display for CrawlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigError(msg) => write!(f, "Configuration error: {msg}"),
            Self::BrowserError(msg) => write!(f, "Browser error: {msg}"),
            Self::NetworkError(msg) => write!(f, "Network error: {msg}"),
            Self::Cancelled => write!(f, "Crawl operation was cancelled"),
            Self::Other(msg) => write!(f, "Crawl error: {msg}"),
        }
    }
}

impl std::error::Error for CrawlError {}

impl From<anyhow::Error> for CrawlError {
    fn from(err: anyhow::Error) -> Self {
        // Use {:#} to preserve full error chain with context
        Self::Other(format!("{err:#}"))
    }
}

/// Convenience alias for Result with `CrawlError`
pub type CrawlResult<T> = Result<T, CrawlError>;

/// Represents a crawl progress update
#[derive(Debug, Clone)]
pub enum CrawlProgress {
    /// Initializing the browser
    Initializing,
    /// Browser launched successfully
    BrowserLaunched,
    /// Page navigation started
    NavigationStarted(String),
    /// Page loaded successfully
    PageLoaded(String),
    /// Extracting page data
    ExtractingData,
    /// Saving assets
    SavingAssets,
    /// Taking screenshot
    TakingScreenshot,
    /// Cleanup started
    CleanupStarted,
    /// Crawl completed successfully
    Completed,
    /// Error occurred
    Error(String),
}

use crate::CrawlRequest;
use serde::{Deserialize, Serialize};

/// A trait defining the interface for web crawlers.
pub trait Crawler {
    /// Create a new crawler with the given configuration.
    fn new(config: crate::config::CrawlConfig) -> Self;

    /// Crawl the target URL and store the results.
    /// Returns a `CrawlRequest` that can be awaited for the final result.
    fn crawl(&self) -> CrawlRequest;
}

/// Represents an item in the crawl queue with URL and depth tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlQueue {
    pub url: String,
    pub depth: u8,
    /// Number of retry attempts for this URL (0 = first attempt)
    #[serde(default)]
    pub retry_count: u8,
}

/// Categorizes page failures for intelligent retry decisions
///
/// Different failure types have different retry characteristics:
/// - Network errors are usually transient → high retry value
/// - Browser errors may recover with backoff → medium retry value  
/// - Content errors are usually permanent → low/no retry value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Network-level failure (timeout, DNS, connection refused)
    /// High retry value - usually transient
    Network,
    /// Browser/page failure (crash, resource exhaustion, CDP error)
    /// Medium retry value - may recover with backoff
    Browser,
    /// Content extraction failure (invalid HTML, missing elements)
    /// Low retry value - unlikely to recover
    ContentExtraction,
    /// Rate limiting detected (HTTP 429)
    /// Special handling - use longer backoff
    RateLimited,
    /// Unknown/unclassified error
    Unknown,
}

impl FailureKind {
    /// Classify an error into a failure kind based on error message patterns
    #[must_use]
    pub fn classify(error: &anyhow::Error) -> Self {
        let msg = error.to_string().to_lowercase();
        
        // Rate limiting (highest priority check)
        if msg.contains("429") || msg.contains("too many requests") || msg.contains("rate limit") {
            return Self::RateLimited;
        }
        
        // Network errors (high retry value)
        if msg.contains("timeout") || msg.contains("timed out") ||
           msg.contains("connection refused") || msg.contains("connection reset") ||
           msg.contains("dns") || msg.contains("network") ||
           msg.contains("unreachable") || msg.contains("eof") {
            return Self::Network;
        }
        
        // Browser/CDP errors (medium retry value)
        if msg.contains("browser") || msg.contains("page") || 
           msg.contains("chrome") || msg.contains("cdp") ||
           msg.contains("target") || msg.contains("session") {
            return Self::Browser;
        }
        
        // Content errors (low retry value)
        if msg.contains("extract") || msg.contains("validation") ||
           msg.contains("content") || msg.contains("html") ||
           msg.contains("parse") || msg.contains("selector") {
            return Self::ContentExtraction;
        }
        
        Self::Unknown
    }
    
    /// Whether this failure kind should be retried by default
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        match self {
            Self::Network | Self::Browser | Self::RateLimited | Self::Unknown => true,
            Self::ContentExtraction => false,  // Usually permanent
        }
    }
    
    /// Base delay multiplier for this failure kind
    #[must_use]
    pub const fn delay_multiplier(&self) -> f64 {
        match self {
            Self::Network => 1.0,
            Self::Browser => 1.5,
            Self::RateLimited => 3.0,  // Longer backoff for rate limits
            Self::ContentExtraction | Self::Unknown => 1.0,
        }
    }
}
