//! Type definitions for resource inlining

/// Resource type for error tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Css,
    Image,
    Svg,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Css => write!(f, "CSS"),
            ResourceType::Image => write!(f, "Image"),
            ResourceType::Svg => write!(f, "SVG"),
        }
    }
}

/// Type alias for resource download future
pub type ResourceFuture = std::pin::Pin<
    Box<
        dyn std::future::Future<Output = Result<(String, String, ResourceType), InliningError>>
            + Send,
    >,
>;

/// Error information for a failed resource download
#[derive(Debug, Clone)]
pub struct InliningError {
    pub url: String,
    pub resource_type: ResourceType,
    pub error: String,
}

/// Result of resource inlining with success and failure tracking
#[derive(Debug, Clone)]
pub struct InliningResult {
    pub html: String,
    pub successes: usize,
    pub failures: Vec<InliningError>,
}

impl InliningResult {
    /// Total number of resources processed
    #[must_use]
    pub fn total(&self) -> usize {
        self.successes + self.failures.len()
    }

    /// Check if any failures occurred
    #[must_use]
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    /// Get failure rate as a ratio between 0.0 and 1.0
    #[must_use]
    pub fn failure_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.failures.len() as f64 / total as f64
        }
    }
}
