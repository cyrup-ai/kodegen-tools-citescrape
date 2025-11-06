//! SVG resource downloading functionality

use super::downloaders::InlineConfig;
use super::types::{InliningError, ResourceType};
use anyhow::Result;
use futures::future::join_all;
use reqwest::Client;

/// Download all SVGs concurrently
/// Returns tuple of (successes, failures) for error tracking
pub async fn download_all_svgs(
    svgs: Vec<(String, String)>,
    client: Client,
    config: &InlineConfig,
    rate_rps: Option<f64>,
) -> Result<(Vec<(String, String)>, Vec<InliningError>)> {
    // Create futures for concurrent execution
    let futures = svgs.into_iter().map(|(svg_url, src)| {
        let client = client.clone();
        let config = config.clone();
        let svg_url_for_error = svg_url.clone();

        async move {
            // Apply rate limiting if configured
            if let Some(rate) = rate_rps {
                match crate::crawl_engine::rate_limiter::check_http_rate_limit(&svg_url, rate).await
                {
                    crate::crawl_engine::rate_limiter::RateLimitDecision::Deny { .. } => {
                        let error_msg = format!("Rate limited: {svg_url}");
                        log::debug!("{error_msg}");
                        return Err(InliningError {
                            url: svg_url_for_error,
                            resource_type: ResourceType::Svg,
                            error: error_msg,
                        });
                    }
                    crate::crawl_engine::rate_limiter::RateLimitDecision::Allow => {
                        // Proceed with download
                    }
                }
            }

            match super::downloaders::download_svg_async(svg_url, client, &config).await {
                Ok(svg_content) => Ok((src, svg_content)),
                Err(e) => {
                    let error_msg = e.to_string();
                    log::warn!("Failed to download SVG from {svg_url_for_error}: {error_msg}");
                    Err(InliningError {
                        url: svg_url_for_error,
                        resource_type: ResourceType::Svg,
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
            Ok((src, content)) => results.push((src, content)),
            Err(error) => failures.push(error),
        }
    }

    Ok((results, failures))
}
