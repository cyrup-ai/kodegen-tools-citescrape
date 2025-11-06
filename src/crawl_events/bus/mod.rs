//! Event bus implementation for publishing and subscribing to crawl events
//!
//! This module provides the core event bus functionality with support for
//! metrics, batching, and filtered subscriptions.

// Core struct and constructors
mod core;

// Functionality implementations
mod impls;
mod metrics_reporting;
mod publishing;
mod shutdown;
mod subscription;

// Re-export the main type
pub use core::CrawlEventBus;
