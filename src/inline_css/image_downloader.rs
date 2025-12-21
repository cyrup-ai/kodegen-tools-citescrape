//! Image resource downloading functionality

use super::downloaders::InlineConfig;
use super::types::{InliningError, ResourceType};
use anyhow::Result;
use futures::future::join_all;
use reqwest::Client;

/// Download all images concurrently
/// Returns tuple of (successes, failures) for error tracking
pub async fn download_all_images(
    images: Vec<(String, String)>,
    client: Client,
    config: &InlineConfig,
    max_inline_size_bytes: Option<usize>,
    rate_rps: Option<f64>,
) -> Result<(Vec<(String, String)>, Vec<InliningError>)> {
    // Create futures for concurrent execution
    let futures = images.into_iter().map(|(image_url, src)| {
        let client = client.clone();
        let config = config.clone();
        let image_url_for_error = image_url.clone();

        async move {
            // Apply rate limiting if configured
            if let Some(rate) = rate_rps {
                match crate::crawl_engine::rate_limiter::check_http_rate_limit(&image_url, rate)
                    .await
                {
                    crate::crawl_engine::rate_limiter::RateLimitDecision::Deny { .. } => {
                        let error_msg = format!("Rate limited: {image_url}");
                        log::debug!("{error_msg}");
                        return Err(InliningError {
                            url: image_url_for_error,
                            resource_type: ResourceType::Image,
                            error: super::domain_queue::DownloadError::RequestFailed(error_msg),
                        });
                    }
                    crate::crawl_engine::rate_limiter::RateLimitDecision::Allow => {
                        // Proceed with download
                    }
                }
            }

            match super::downloaders::download_and_encode_image_async(
                image_url,
                client,
                &config,
                max_inline_size_bytes,
            )
            .await
            {
                Ok(data_url) => Ok((src, data_url)),
                Err(e) => {
                    let error_msg = e.to_string();
                    log::warn!("Failed to download image from {image_url_for_error}: {error_msg}");
                    Err(InliningError {
                        url: image_url_for_error,
                        resource_type: ResourceType::Image,
                        error: super::domain_queue::DownloadError::RequestFailed(error_msg),
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
