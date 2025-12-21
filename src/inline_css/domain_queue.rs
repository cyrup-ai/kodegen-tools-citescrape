//! Per-domain download queue for rate-limited resource downloads
//!
//! This module provides serialized download processing per domain to prevent
//! thundering herd problems when multiple resources from the same domain are
//! requested concurrently.

use dashmap::DashMap;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::crawl_engine::rate_limiter::{check_http_rate_limit, extract_domain, RateLimitDecision};

/// Normalize URL for consistent cache key generation
/// 
/// This function ensures that semantically identical URLs produce the same cache key,
/// preventing cache misses due to minor string variations (trailing slashes, port numbers,
/// URL encoding differences, etc.).
/// 
/// # Normalization steps:
/// - Parse URL and serialize back to canonical form
/// - Remove default ports (80 for http, 443 for https)
/// - Remove trailing slashes from paths (except root "/")
/// - Ensure consistent percent-encoding
fn normalize_url_for_cache(url: &str) -> String {
    // If parsing fails, return original URL (defensive programming)
    let Ok(mut parsed) = url::Url::parse(url) else {
        log::warn!("Failed to parse URL for normalization: {}", url);
        return url.to_string();
    };
    
    // Remove default ports to ensure "http://example.com" == "http://example.com:80"
    if let Some(port) = parsed.port() {
        let scheme = parsed.scheme();
        if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
            let _ = parsed.set_port(None);
        }
    }
    
    // Remove trailing slash from path (except for root path "/")
    let path = parsed.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        parsed.set_path(path.trim_end_matches('/'));
    }
    
    // url::Url's to_string() already ensures consistent percent-encoding
    parsed.to_string()
}

/// Cached response type - stores either successful content or error status
#[derive(Debug, Clone)]
pub enum CachedResponse {
    /// Successful download with content bytes
    Success(Vec<u8>),
    /// HTTP error with status code
    Error(u16),
}

/// Error type for download failures
#[derive(Debug, Clone, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum DownloadError {
    #[error("Download failed: {0}")]
    RequestFailed(String),
    
    #[error("Invalid domain for URL: {0}")]
    InvalidDomain(String),
    
    #[error("Resource not found (404): {0}")]
    NotFound(String),
    
    #[error("HTTP error {status}: {url}")]
    HttpError {
        url: String,
        status: u16,
        #[serde(default)]
        from_cache: bool,
    },
}

impl From<anyhow::Error> for DownloadError {
    fn from(e: anyhow::Error) -> Self {
        DownloadError::RequestFailed(e.to_string())
    }
}

impl DownloadError {
    /// Returns true if this error came from cache (no HTTP request was made)
    pub fn is_from_cache(&self) -> bool {
        match self {
            DownloadError::HttpError { from_cache, .. } => *from_cache,
            _ => false,
        }
    }
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
        http_error_cache: Arc<DashMap<String, CachedResponse>>,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        
        let worker_handle = tokio::spawn(async move {
            Self::worker_loop(domain, client, rate_rps, user_agent, http_error_cache, rx).await
        });
        
        Self { tx, worker_handle }
    }
    
    /// Submit a download request to the queue
    pub async fn submit_download(&self, url: String) -> Result<Vec<u8>, DownloadError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let request = DownloadRequest { url, response_tx };
        
        // Send request to worker
        self.tx.send(request).map_err(|_| {
            DownloadError::RequestFailed("Worker task terminated".to_string())
        })?;
        
        // Await response from worker
        response_rx
            .await
            .map_err(|_| DownloadError::RequestFailed("Worker dropped response channel".to_string()))?
    }
    
    /// Worker loop that processes downloads serially
    async fn worker_loop(
        domain: String,
        client: Client,
        rate_rps: Option<f64>,
        user_agent: String,
        http_error_cache: Arc<DashMap<String, CachedResponse>>,
        mut rx: mpsc::UnboundedReceiver<DownloadRequest>,
    ) {
        log::debug!("Started download worker for domain: {domain}");
        
        while let Some(request) = rx.recv().await {
            let url = request.url.clone();
            let cache_key = normalize_url_for_cache(&url);
            
            // DIAGNOSTIC: Log cache state and URL being checked
            log::debug!("[CACHE-DIAG] worker_loop checking URL: {} | Normalized: {} | Cache size: {} | Cache contains: {}", 
                url, cache_key, http_error_cache.len(), http_error_cache.contains_key(&cache_key));
            
            // Check cache HERE - this is the only serialization point where the check works
            // All requests are queued before any processing starts, so the cache check in
            // submit_download() always sees an empty cache. Only here can we catch duplicates.
            if let Some(cached) = http_error_cache.get(&cache_key) {
                match cached.value() {
                    CachedResponse::Success(bytes) => {
                        log::debug!("[CACHE-HIT] worker_loop Success: {} ({} bytes)", url, bytes.len());
                        let _ = request.response_tx.send(Ok(bytes.clone()));
                        continue;
                    }
                    CachedResponse::Error(status) => {
                        log::debug!("[CACHE-HIT] worker_loop Error (HTTP {}): {}", status, url);
                        let _ = request.response_tx.send(Err(DownloadError::HttpError {
                            url,
                            status: *status,
                            from_cache: true,
                        }));
                        continue;
                    }
                }
            }
            
            log::debug!("[CACHE-MISS] worker_loop will download: {}", url);
            
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
            let result = Self::download_bytes(&client, &url, &user_agent, &http_error_cache).await;
            
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
        http_error_cache: &DashMap<String, CachedResponse>,
    ) -> Result<Vec<u8>, DownloadError> {
        let response = client
            .get(url)
            .header("User-Agent", user_agent)
            .send()
            .await
            .map_err(|e| DownloadError::RequestFailed(format!("Request failed: {e}")))?;
        
        let status = response.status();
        
        // Cache all 4XX and 5XX errors
        if status.is_client_error() || status.is_server_error() {
            let url_string = url.to_string();
            let cache_key = normalize_url_for_cache(&url_string);
            log::warn!("HTTP {} error: {}", status.as_u16(), url_string);
            http_error_cache.insert(cache_key, CachedResponse::Error(status.as_u16()));
            
            return Err(DownloadError::HttpError {
                url: url_string,
                status: status.as_u16(),
                from_cache: false,
            });
        }
        
        // Check for other non-success statuses (3XX redirects, etc.)
        if !status.is_success() {
            return Err(DownloadError::RequestFailed(format!(
                "HTTP error {}: {}",
                status,
                url
            )));
        }
        
        let bytes = response
            .bytes()
            .await
            .map_err(|e| DownloadError::RequestFailed(format!("Failed to read bytes: {e}")))?;
        
        // NEW: Cache successful downloads
        let cache_key = normalize_url_for_cache(url);
        let bytes_vec = bytes.to_vec();
        log::debug!("[CACHE-INSERT] Success: {} ({} bytes)", url, bytes_vec.len());
        http_error_cache.insert(cache_key, CachedResponse::Success(bytes_vec.clone()));
        
        Ok(bytes_vec)
    }
}

/// In-flight download result type for sharing between concurrent callers
type InFlightResult = Option<Result<Vec<u8>, DownloadError>>;

/// Manages per-domain download queues
pub struct DomainQueueManager {
    queues: Arc<DashMap<String, Arc<DomainDownloadQueue>>>,
    client: Client,
    rate_rps: Option<f64>,
    user_agent: String,
    http_error_cache: Arc<DashMap<String, CachedResponse>>,
    /// Track URLs currently being downloaded to coalesce duplicate requests
    in_flight: Arc<DashMap<String, watch::Sender<InFlightResult>>>,
}

impl DomainQueueManager {
    /// Create a new domain queue manager
    /// 
    /// # Arguments
    /// * `client` - HTTP client for making requests
    /// * `rate_rps` - Optional rate limit in requests per second
    /// * `user_agent` - User-Agent string for HTTP requests
    /// * `http_error_cache` - Shared cache for HTTP error responses (enables cross-page caching)
    /// * `queues` - Shared domain queues (enables cross-page worker sharing)
    pub fn new(
        client: Client,
        rate_rps: Option<f64>,
        user_agent: String,
        http_error_cache: Arc<DashMap<String, CachedResponse>>,
        queues: Arc<DashMap<String, Arc<DomainDownloadQueue>>>,
    ) -> Self {
        Self {
            queues,
            client,
            rate_rps,
            user_agent,
            http_error_cache,
            in_flight: Arc::new(DashMap::new()),
        }
    }
    
    /// Submit a download request (will route to appropriate domain queue)
    /// 
    /// This method coalesces duplicate requests - if the same URL is already being
    /// downloaded, subsequent callers will wait for and share the result instead
    /// of making a new HTTP request.
    pub async fn submit_download(&self, url: String) -> Result<Vec<u8>, DownloadError> {
        let cache_key = normalize_url_for_cache(&url);
        
        // DIAGNOSTIC: Log cache check at manager level
        log::debug!("[CACHE-DIAG] DomainQueueManager checking URL: {} | Normalized: {} | Cache size: {} | Contains: {}", 
            url, cache_key, self.http_error_cache.len(), self.http_error_cache.contains_key(&cache_key));
        
        // Check HTTP error cache first (before any expensive operations)
        if let Some(cached) = self.http_error_cache.get(&cache_key) {
            match cached.value() {
                CachedResponse::Success(bytes) => {
                    log::debug!("[CACHE-HIT] DomainQueueManager Success: {} ({} bytes)", url, bytes.len());
                    return Ok(bytes.clone());
                }
                CachedResponse::Error(status) => {
                    log::debug!("[CACHE-HIT] DomainQueueManager Error (HTTP {}): {}", status, url);
                    return Err(DownloadError::HttpError {
                        url: url.clone(),
                        status: *status,
                        from_cache: true,
                    });
                }
            }
        }
        
        log::debug!("[CACHE-MISS] DomainQueueManager will queue: {}", url);
        
        // Check if this URL is already being downloaded by another caller
        // If so, subscribe to the result instead of making a new request
        if let Some(sender) = self.in_flight.get(&cache_key) {
            log::debug!("URL already in-flight, waiting for result: {url}");
            let mut rx = sender.subscribe();
            drop(sender); // Release DashMap lock before awaiting
            
            // Wait for the result to be available
            loop {
                if let Some(result) = rx.borrow_and_update().clone() {
                    return result;
                }
                // Wait for the value to change
                if rx.changed().await.is_err() {
                    // Sender was dropped without sending result
                    return Err(DownloadError::RequestFailed(
                        "In-flight request dropped without result".to_string()
                    ));
                }
            }
        }
        
        // Register this URL as in-flight BEFORE starting the download
        // Use a watch channel so multiple receivers can get the result
        let (tx, _rx) = watch::channel(None);
        self.in_flight.insert(cache_key.clone(), tx.clone());
        
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
                    Arc::clone(&self.http_error_cache),
                ))
            })
            .clone();
        
        // Submit to queue and get result
        let result = queue.submit_download(url.clone()).await;
        
        // Broadcast result to all waiters and remove from in-flight
        let _ = tx.send(Some(result.clone()));
        self.in_flight.remove(&cache_key);
        
        result
    }
}

impl Clone for DomainQueueManager {
    fn clone(&self) -> Self {
        Self {
            queues: Arc::clone(&self.queues),
            client: self.client.clone(),
            rate_rps: self.rate_rps,
            user_agent: self.user_agent.clone(),
            http_error_cache: Arc::clone(&self.http_error_cache),
            in_flight: Arc::clone(&self.in_flight),
        }
    }
}
