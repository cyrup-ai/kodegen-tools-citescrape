//! Progress reporting abstraction for crawl operations
//!
//! Defines the `ProgressReporter` trait for lifecycle event reporting
//! and provides a no-op implementation for simple use cases.

/// Trait for reporting crawl progress at key lifecycle events
///
/// Implementations can send updates to channels, log to console, update UI, etc.
/// This abstraction allows the same core crawl logic to support both simple
/// and progress-reporting APIs.
pub trait ProgressReporter: Send + Sync {
    /// Report that browser initialization has started
    fn report_initializing(&self);

    /// Report that the browser has launched successfully
    fn report_browser_launched(&self);

    /// Report that navigation to a URL has started
    fn report_navigation_started(&self, url: &str);

    /// Report that a page has loaded successfully
    fn report_page_loaded(&self, url: &str);

    /// Report that page data extraction has started
    fn report_extracting_data(&self);

    /// Report that a screenshot is being captured
    fn report_taking_screenshot(&self);

    /// Report that cleanup has started
    fn report_cleanup_started(&self);

    /// Report that the crawl has completed successfully
    fn report_completed(&self);

    /// Report an error that occurred during crawling
    fn report_error(&self, error: &str);
}

/// Progress reporter that does nothing
///
/// Used by the simple `crawl_impl()` API that doesn't need progress updates.
/// All methods are no-ops and will be inlined away by the compiler.
#[derive(Debug, Clone, Copy)]
pub struct NoOpProgress;

impl ProgressReporter for NoOpProgress {
    #[inline(always)]
    fn report_initializing(&self) {}

    #[inline(always)]
    fn report_browser_launched(&self) {}

    #[inline(always)]
    fn report_navigation_started(&self, _url: &str) {}

    #[inline(always)]
    fn report_page_loaded(&self, _url: &str) {}

    #[inline(always)]
    fn report_extracting_data(&self) {}

    #[inline(always)]
    fn report_taking_screenshot(&self) {}

    #[inline(always)]
    fn report_cleanup_started(&self) {}

    #[inline(always)]
    fn report_completed(&self) {}

    #[inline(always)]
    fn report_error(&self, _error: &str) {}
}
