//! Per-domain download queue for rate-limited resource downloads
//!
//! This module provides serialized download processing per domain to prevent
//! thundering herd problems when multiple resources from the same domain are
//! requested concurrently.

use dashmap::DashMap;
use reqwest::Client;
use reqwest::StatusCode;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::crawl_engine::rate_limiter::{check_http_rate_limit, extract_domain, RateLimitDecision};

/// Error type for download failures
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("Download failed: {0}")]
    RequestFailed(#[from] anyhow::Error),
    
    #[error("Invalid domain for URL: {0}")]
    InvalidDomain(String),
    
    #[error("Resource not found (404): {0}")]
    NotFound(String),
}

/// Request to download a resource
struct DownloadRequest {
    url: String,
    response_tx: oneshot::Sender<Result<Vec<u8>, DownloadError>>,
}

/// Per-domain download queue with worker task
pub struct DomainDownloadQueue {
    tx: mpsc::UnboundedSender<DownloadRequest>,
    #[allow(dead_code)]
    worker_handle: JoinHandle<()>,
}

impl DomainDownloadQueue {
    /// Create a new download queue for a domain
    pub fn new(
        domain: String,
        client: Client,
        rate_rps: Option<f64>,
        user_agent: String,
        not_found_cache: Arc<DashMap<String, ()>>,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        
        let worker_handle = tokio::spawn(async move {
            Self::worker_loop(domain, client, rate_rps, user_agent, not_found_cache, rx).await
        });
        
        Self { tx, worker_handle }
    }
    
    /// Submit a download request to the queue
    pub async fn submit_download(&self, url: String) -> Result<Vec<u8>, DownloadError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let request = DownloadRequest { url, response_tx };
        
        // Send request to worker
        self.tx.send(request).map_err(|_| {
            DownloadError::RequestFailed(anyhow::anyhow!("Worker task terminated"))
        })?;
        
        // Await response from worker
        response_rx
            .await
            .map_err(|_| DownloadError::RequestFailed(anyhow::anyhow!("Worker dropped response channel")))?
    }
    
    /// Worker loop that processes downloads serially
    async fn worker_loop(
        domain: String,
        client: Client,
        rate_rps: Option<f64>,
        user_agent: String,
        not_found_cache: Arc<DashMap<String, ()>>,
        mut rx: mpsc::UnboundedReceiver<DownloadRequest>,
    ) {
        log::debug!("Started download worker for domain: {domain}");
        
        while let Some(request) = rx.recv().await {
            let url = request.url.clone();
            
            // Check rate limit (single point of serialization)
            if let Some(rate) = rate_rps {
                loop {
                    match check_http_rate_limit(&url, rate).await {
                        RateLimitDecision::Allow => break,
                        RateLimitDecision::Deny { retry_after } => {
                            log::debug!(
                                "Rate limited: {url}, waiting {:?} (domain: {domain})",
                                retry_after
                            );
                            tokio::time::sleep(retry_after).await;
                        }
                    }
                }
            }
            
            // Download the resource
            log::debug!("Downloading: {url} (domain: {domain})");
            let result = Self::download_bytes(&client, &url, &user_agent, &not_found_cache).await;
            
            // Send result back to caller (ignore send errors - caller may have dropped)
            let _ = request.response_tx.send(result);
        }
        
        log::debug!("Download worker terminated for domain: {domain}");
    }
    
    /// Download bytes from URL using reqwest
    async fn download_bytes(
        client: &Client,
        url: &str,
        user_agent: &str,
        not_found_cache: &DashMap<String, ()>,
    ) -> Result<Vec<u8>, DownloadError> {
        let response = client
            .get(url)
            .header("User-Agent", user_agent)
            .send()
            .await
            .map_err(|e| DownloadError::RequestFailed(anyhow::anyhow!("Request failed: {e}")))?;
        
        // Check for 404 specifically and cache it
        if response.status() == StatusCode::NOT_FOUND {
            log::info!("Caching 404 for URL: {}", url);
            not_found_cache.insert(url.to_string(), ());
            return Err(DownloadError::NotFound(url.to_string()));
        }
        
        // Check for other non-success statuses
        if !response.status().is_success() {
            return Err(DownloadError::RequestFailed(anyhow::anyhow!(
                "HTTP error {}: {}",
                response.status(),
                url
            )));
        }
        
        let bytes = response
            .bytes()
            .await
            .map_err(|e| DownloadError::RequestFailed(anyhow::anyhow!("Failed to read bytes: {e}")))?;
        
        Ok(bytes.to_vec())
    }
}

/// Manages per-domain download queues
pub struct DomainQueueManager {
    queues: Arc<DashMap<String, Arc<DomainDownloadQueue>>>,
    client: Client,
    rate_rps: Option<f64>,
    user_agent: String,
    not_found_cache: Arc<DashMap<String, ()>>,
}

impl DomainQueueManager {
    /// Create a new domain queue manager
    pub fn new(client: Client, rate_rps: Option<f64>, user_agent: String) -> Self {
        Self {
            queues: Arc::new(DashMap::new()),
            client,
            rate_rps,
            user_agent,
            not_found_cache: Arc::new(DashMap::new()),
        }
    }
    
    /// Submit a download request (will route to appropriate domain queue)
    pub async fn submit_download(&self, url: String) -> Result<Vec<u8>, DownloadError> {
        // Check 404 cache first (before any expensive operations)
        if self.not_found_cache.contains_key(&url) {
            log::debug!("Skipping previously 404'd URL: {}", url);
            return Err(DownloadError::NotFound(url));
        }
        
        // Extract domain from URL
        let domain = extract_domain(&url)
            .ok_or_else(|| DownloadError::InvalidDomain(url.clone()))?;
        
        // Get or create queue for this domain
        let queue = self.queues
            .entry(domain.clone())
            .or_insert_with(|| {
                log::debug!("Creating new download queue for domain: {domain}");
                Arc::new(DomainDownloadQueue::new(
                    domain.clone(),
                    self.client.clone(),
                    self.rate_rps,
                    self.user_agent.clone(),
                    Arc::clone(&self.not_found_cache),
                ))
            })
            .clone();
        
        // Submit to queue
        queue.submit_download(url).await
    }
}

impl Clone for DomainQueueManager {
    fn clone(&self) -> Self {
        Self {
            queues: Arc::clone(&self.queues),
            client: self.client.clone(),
            rate_rps: self.rate_rps,
            user_agent: self.user_agent.clone(),
            not_found_cache: Arc::clone(&self.not_found_cache),
        }
    }
}
