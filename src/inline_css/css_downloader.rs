//! CSS resource downloading functionality

use super::downloaders::InlineConfig;
use super::types::{InliningError, ResourceType};
use anyhow::Result;
use futures::future::join_all;
use reqwest::Client;

/// Download all CSS files concurrently
/// Returns tuple of (successes, failures) for error tracking
pub async fn download_all_css(
    css_links: Vec<(String, String)>,
    client: Client,
    config: &InlineConfig,
    rate_rps: Option<f64>,
) -> Result<(Vec<(String, String)>, Vec<InliningError>)> {
    // Create futures for concurrent execution
    let futures = css_links.into_iter().map(|(css_url, href)| {
        let client = client.clone();
        let config = config.clone();
        let css_url_for_error = css_url.clone();

        async move {
            // Apply rate limiting if configured
            if let Some(rate) = rate_rps {
                match crate::crawl_engine::rate_limiter::check_http_rate_limit(&css_url, rate).await
                {
                    crate::crawl_engine::rate_limiter::RateLimitDecision::Deny { .. } => {
                        let error_msg = format!("Rate limited: {css_url}");
                        log::debug!("{error_msg}");
                        return Err(InliningError {
                            url: css_url_for_error,
                            resource_type: ResourceType::Css,
                            error: error_msg,
                        });
                    }
                    crate::crawl_engine::rate_limiter::RateLimitDecision::Allow => {
                        // Proceed with download
                    }
                }
            }

            match super::downloaders::download_css_async(css_url, client, &config).await {
                Ok(css_content) => Ok((href, css_content)),
                Err(e) => {
                    let error_msg = e.to_string();
                    log::warn!("Failed to download CSS from {css_url_for_error}: {error_msg}");
                    Err(InliningError {
                        url: css_url_for_error,
                        resource_type: ResourceType::Css,
                        error: error_msg,
                    })
                }
            }
        }
    });

    // Execute all downloads concurrently
    let download_results = join_all(futures).await;

    // Partition into successes and failures
    let mut results = Vec::new();
    let mut failures = Vec::new();

    for result in download_results {
        match result {
            Ok((href, content)) => results.push((href, content)),
            Err(error) => failures.push(error),
        }
    }

    Ok((results, failures))
}
