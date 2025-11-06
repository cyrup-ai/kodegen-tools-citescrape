//! Metrics reporting for the CrawlEventBus

use crate::crawl_events::metrics::MetricsSnapshot;

use super::core::CrawlEventBus;

impl CrawlEventBus {
    /// Get detailed metrics report
    ///
    /// Uses a snapshot to ensure all metrics are consistent with each other.
    #[must_use]
    pub fn get_metrics_report(&self) -> String {
        if !self.config.enable_metrics {
            return "Metrics disabled".to_string();
        }

        let snapshot = self.metrics.snapshot();

        format!(
            "Event Bus Metrics:\n\
             - Events Published: {}\n\
             - Events Dropped: {}\n\
             - Events Failed: {}\n\
             - Active Subscribers: {}\n\
             - Peak Subscribers: {}\n\
             - Success Rate: {:.2}%",
            snapshot.events_published,
            snapshot.events_dropped,
            snapshot.events_failed,
            snapshot.active_subscribers,
            snapshot.peak_subscribers,
            self.calculate_success_rate_from_snapshot(&snapshot)
        )
    }

    pub(super) fn calculate_success_rate_from_snapshot(&self, snapshot: &MetricsSnapshot) -> f64 {
        let total = snapshot.events_published;
        if total == 0 {
            return 100.0;
        }
        let failed = snapshot.events_failed;
        ((total - failed) as f64 / total as f64) * 100.0
    }
}
