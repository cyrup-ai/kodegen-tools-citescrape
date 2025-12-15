//! Persistent link index using SQLite for event-driven link rewriting.
//!
//! This module provides a database layer that tracks:
//! - All saved pages (URL → local path mapping)
//! - Link graph edges (which pages link to which)
//!
//! This enables efficient queries like:
//! - "Does this URL have a local copy?" (O(log n) indexed lookup)
//! - "Which pages link to this URL?" (for retroactive rewriting)

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use tokio::sync::RwLock;
use url::Url;

/// SQL schema for link index database
const SCHEMA_SQL: &str = r#"
-- Saved pages: maps URLs to local file paths
CREATE TABLE IF NOT EXISTS pages (
    url TEXT PRIMARY KEY,
    local_path TEXT NOT NULL,
    domain TEXT NOT NULL,
    saved_at INTEGER NOT NULL
);

-- Index for domain-scoped queries (find all pages from example.com)
CREATE INDEX IF NOT EXISTS idx_pages_domain ON pages(domain);

-- Link graph edges: tracks which pages link to which
CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_url TEXT NOT NULL,
    target_url TEXT NOT NULL,
    UNIQUE(source_url, target_url)
);

-- Index for outbound queries (what does page X link to?)
CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_url);

-- Index for inbound queries (who links to page X?) - critical for retroactive rewriting
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_url);
"#;

/// Persistent index of crawled pages and their link relationships.
///
/// Uses SQLite with WAL mode for:
/// - Concurrent reads during writes
/// - ACID transactions for consistency
/// - O(log n) indexed queries
/// - Handles millions of pages easily
#[derive(Clone)]
pub struct LinkIndex {
    pool: SqlitePool,
    output_dir: PathBuf,
    /// Cache of recently queried URLs for fast repeated lookups
    path_cache: Arc<RwLock<lru::LruCache<String, Option<PathBuf>>>>,
}

impl LinkIndex {
    /// Open existing index or create new one.
    ///
    /// The database is stored at `{output_dir}/.citescrape/link_index.sqlite`
    pub async fn open(output_dir: &Path) -> Result<Self> {
        let db_dir = output_dir.join(".citescrape");
        tokio::fs::create_dir_all(&db_dir)
            .await
            .context("Failed to create .citescrape directory")?;

        let db_path = db_dir.join("link_index.sqlite");

        // Configure SQLite for optimal concurrent performance
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal) // WAL mode for concurrent reads
            .synchronous(SqliteSynchronous::Normal) // Good balance of safety/speed
            .busy_timeout(std::time::Duration::from_secs(30));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .context("Failed to open SQLite database")?;

        // Run schema migrations (idempotent - CREATE IF NOT EXISTS)
        sqlx::query(SCHEMA_SQL)
            .execute(&pool)
            .await
            .context("Failed to initialize database schema")?;

        // LRU cache for path lookups (1000 entries should cover most cases)
        let path_cache = Arc::new(RwLock::new(lru::LruCache::new(
            std::num::NonZeroUsize::new(1000).unwrap(),
        )));

        Ok(Self {
            pool,
            output_dir: output_dir.to_path_buf(),
            path_cache,
        })
    }

    /// Get local path for URL if it exists in index.
    ///
    /// Returns `None` if the URL has not been saved locally.
    pub async fn get_local_path(&self, url: &str) -> Result<Option<PathBuf>> {
        let normalized = normalize_url(url);

        // Check cache first
        {
            let cache = self.path_cache.read().await;
            if let Some(cached) = cache.peek(&normalized) {
                return Ok(cached.clone());
            }
        }

        // Query database
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT local_path FROM pages WHERE url = ?"
        )
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to query local path")?;

        let path = result.map(|(p,)| PathBuf::from(p));

        // Update cache
        {
            let mut cache = self.path_cache.write().await;
            cache.put(normalized, path.clone());
        }

        Ok(path)
    }

    /// Get all pages that link TO a given target URL.
    ///
    /// Returns Vec of (source_url, source_local_path) for retroactive rewriting.
    pub async fn get_inbound_links(&self, target_url: &str) -> Result<Vec<(String, PathBuf)>> {
        let normalized = normalize_url(target_url);

        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT l.source_url, p.local_path
            FROM links l
            JOIN pages p ON l.source_url = p.url
            WHERE l.target_url = ?
            "#
        )
        .bind(&normalized)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query inbound links")?;

        Ok(rows
            .into_iter()
            .map(|(url, path)| (url, PathBuf::from(path)))
            .collect())
    }

    /// Get all URLs that a given source page links TO.
    ///
    /// Returns Vec of target URLs (outbound links from the page).
    pub async fn get_outbound_links(&self, source_url: &str) -> Result<Vec<String>> {
        let normalized = normalize_url(source_url);

        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT target_url FROM links WHERE source_url = ?"
        )
        .bind(&normalized)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query outbound links")?;

        Ok(rows.into_iter().map(|(url,)| url).collect())
    }

    /// Atomically register a page and its outbound links.
    ///
    /// This is called AFTER a page is saved to disk. It:
    /// 1. Upserts the page record (URL → local path)
    /// 2. Clears old outbound links for this page
    /// 3. Inserts new outbound links
    ///
    /// All operations are in a single transaction for consistency.
    pub async fn register_page(
        &self,
        url: &str,
        local_path: &Path,
        outbound_links: &[String],
    ) -> Result<()> {
        let normalized_url = normalize_url(url);
        let domain = extract_domain(url);
        let local_path_str = local_path.to_string_lossy().to_string();
        let timestamp = chrono::Utc::now().timestamp();

        // Normalize all outbound links
        let normalized_outbound: Vec<String> = outbound_links
            .iter()
            .map(|u| normalize_url(u))
            .collect();

        let mut tx = self.pool.begin().await.context("Failed to begin transaction")?;

        // Upsert page record
        sqlx::query(
            r#"
            INSERT INTO pages (url, local_path, domain, saved_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(url) DO UPDATE SET
                local_path = excluded.local_path,
                saved_at = excluded.saved_at
            "#
        )
        .bind(&normalized_url)
        .bind(&local_path_str)
        .bind(&domain)
        .bind(timestamp)
        .execute(&mut *tx)
        .await
        .context("Failed to upsert page")?;

        // Clear old outbound links
        sqlx::query("DELETE FROM links WHERE source_url = ?")
            .bind(&normalized_url)
            .execute(&mut *tx)
            .await
            .context("Failed to delete old links")?;

        // Insert new outbound links
        for target in &normalized_outbound {
            sqlx::query(
                "INSERT OR IGNORE INTO links (source_url, target_url) VALUES (?, ?)"
            )
            .bind(&normalized_url)
            .bind(target)
            .execute(&mut *tx)
            .await
            .context("Failed to insert link")?;
        }

        tx.commit().await.context("Failed to commit transaction")?;

        // Invalidate cache entry for this URL
        {
            let mut cache = self.path_cache.write().await;
            cache.put(normalized_url, Some(local_path.to_path_buf()));
        }

        Ok(())
    }

    /// Batch check which URLs from a list exist in the index.
    ///
    /// Returns the subset of URLs that have local copies saved.
    /// Uses efficient IN clause query for batch performance.
    pub async fn filter_existing(&self, urls: &[String]) -> Result<HashSet<String>> {
        if urls.is_empty() {
            return Ok(HashSet::new());
        }

        // Normalize all URLs
        let normalized: Vec<String> = urls.iter().map(|u| normalize_url(u)).collect();

        // Build parameterized IN clause
        // SQLite has a limit of ~999 variables, so batch if needed
        let mut existing = HashSet::new();

        for chunk in normalized.chunks(500) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let query_str = format!(
                "SELECT url FROM pages WHERE url IN ({})",
                placeholders.join(", ")
            );

            let mut query = sqlx::query(&query_str);
            for url in chunk {
                query = query.bind(url);
            }

            let rows = query.fetch_all(&self.pool).await.context("Failed to filter existing URLs")?;

            for row in rows {
                let url: String = row.get("url");
                existing.insert(url);
            }
        }

        Ok(existing)
    }

    /// Get all pages from a specific domain.
    ///
    /// Useful for domain-scoped operations.
    pub async fn get_pages_by_domain(&self, domain: &str) -> Result<Vec<(String, PathBuf)>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT url, local_path FROM pages WHERE domain = ?"
        )
        .bind(domain)
        .fetch_all(&self.pool)
        .await
        .context("Failed to query pages by domain")?;

        Ok(rows
            .into_iter()
            .map(|(url, path)| (url, PathBuf::from(path)))
            .collect())
    }

    /// Get total number of indexed pages.
    pub async fn page_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pages")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count pages")?;
        Ok(row.0)
    }

    /// Get total number of indexed links.
    pub async fn link_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count links")?;
        Ok(row.0)
    }

    /// Get the output directory this index is associated with.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Close the database connection pool.
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Normalize URL for consistent matching across different representations.
///
/// Handles:
/// - Lowercase scheme and host
/// - Remove default ports (80, 443)
/// - Remove trailing slash from path (unless root)
/// - Remove fragment
/// - Decode unnecessary percent-encoding (but keep encoded chars that need it)
pub fn normalize_url(url: &str) -> String {
    // Try to parse as URL
    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => {
            // If it's a relative URL or malformed, just return lowercased
            return url.to_lowercase();
        }
    };

    let mut normalized = String::with_capacity(url.len());

    // Scheme (lowercase)
    normalized.push_str(parsed.scheme());
    normalized.push_str("://");

    // Host (lowercase, already done by Url::parse)
    if let Some(host) = parsed.host_str() {
        normalized.push_str(host);
    }

    // Port (omit default ports)
    if let Some(port) = parsed.port() {
        let default_port = match parsed.scheme() {
            "http" => 80,
            "https" => 443,
            _ => 0,
        };
        if port != default_port {
            normalized.push(':');
            normalized.push_str(&port.to_string());
        }
    }

    // Path (remove trailing slash unless root)
    let path = parsed.path();
    if path.len() > 1 && path.ends_with('/') {
        normalized.push_str(&path[..path.len() - 1]);
    } else if path.is_empty() {
        normalized.push('/');
    } else {
        normalized.push_str(path);
    }

    // Query string (keep as-is, it's significant)
    if let Some(query) = parsed.query() {
        normalized.push('?');
        normalized.push_str(query);
    }

    // Fragment is omitted (not significant for page identity)

    normalized
}

/// Extract domain from URL for domain-scoped queries.
pub fn extract_domain(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_normalize_url() {
        assert_eq!(
            normalize_url("https://Example.Com/Path/"),
            "https://example.com/Path"
        );
        assert_eq!(
            normalize_url("http://example.com:80/path"),
            "http://example.com/path"
        );
        assert_eq!(
            normalize_url("https://example.com:443/"),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url("https://example.com:8080/path"),
            "https://example.com:8080/path"
        );
        assert_eq!(
            normalize_url("https://example.com/path#fragment"),
            "https://example.com/path"
        );
        assert_eq!(
            normalize_url("https://example.com/path?a=1&b=2"),
            "https://example.com/path?a=1&b=2"
        );
    }

    #[tokio::test]
    async fn test_extract_domain() {
        assert_eq!(extract_domain("https://Example.Com/path"), "example.com");
        assert_eq!(extract_domain("http://sub.domain.org:8080/"), "sub.domain.org");
        assert_eq!(extract_domain("invalid-url"), "");
    }

    #[tokio::test]
    async fn test_link_index_basic_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let index = LinkIndex::open(temp_dir.path()).await?;

        // Initially no pages
        assert_eq!(index.page_count().await?, 0);
        assert_eq!(index.link_count().await?, 0);

        // Register a page with outbound links
        let page_url = "https://example.com/page1";
        let local_path = temp_dir.path().join("page1.html");
        let outbound = vec![
            "https://example.com/page2".to_string(),
            "https://other.com/external".to_string(),
        ];

        index.register_page(page_url, &local_path, &outbound).await?;

        // Verify page was registered
        assert_eq!(index.page_count().await?, 1);
        assert_eq!(index.link_count().await?, 2);

        // Get local path
        let retrieved = index.get_local_path(page_url).await?;
        assert_eq!(retrieved, Some(local_path.clone()));

        // Non-existent URL returns None
        let missing = index.get_local_path("https://nowhere.com").await?;
        assert_eq!(missing, None);

        // Check outbound links
        let outbound_result = index.get_outbound_links(page_url).await?;
        assert_eq!(outbound_result.len(), 2);

        // Filter existing
        let check_urls = vec![
            "https://example.com/page1".to_string(),  // exists
            "https://example.com/page2".to_string(),  // doesn't exist (only linked to)
            "https://nowhere.com".to_string(),         // doesn't exist
        ];
        let existing = index.filter_existing(&check_urls).await?;
        assert_eq!(existing.len(), 1);
        assert!(existing.contains(&normalize_url("https://example.com/page1")));

        index.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_inbound_links() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let index = LinkIndex::open(temp_dir.path()).await?;

        let target_url = "https://example.com/target";
        let target_path = temp_dir.path().join("target.html");

        // Page A links to target
        let page_a = "https://example.com/page-a";
        let path_a = temp_dir.path().join("page-a.html");
        index.register_page(page_a, &path_a, &[target_url.to_string()]).await?;

        // Page B also links to target
        let page_b = "https://example.com/page-b";
        let path_b = temp_dir.path().join("page-b.html");
        index.register_page(page_b, &path_b, &[target_url.to_string()]).await?;

        // Now register target itself
        index.register_page(target_url, &target_path, &[]).await?;

        // Get inbound links to target
        let inbound = index.get_inbound_links(target_url).await?;
        assert_eq!(inbound.len(), 2);

        let source_urls: HashSet<_> = inbound.iter().map(|(url, _)| url.as_str()).collect();
        assert!(source_urls.contains(&normalize_url(page_a).as_str()));
        assert!(source_urls.contains(&normalize_url(page_b).as_str()));

        index.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_domain_queries() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let index = LinkIndex::open(temp_dir.path()).await?;

        // Register pages from different domains
        index.register_page(
            "https://example.com/page1",
            &temp_dir.path().join("ex1.html"),
            &[],
        ).await?;
        index.register_page(
            "https://example.com/page2",
            &temp_dir.path().join("ex2.html"),
            &[],
        ).await?;
        index.register_page(
            "https://other.com/page1",
            &temp_dir.path().join("ot1.html"),
            &[],
        ).await?;

        // Query by domain
        let example_pages = index.get_pages_by_domain("example.com").await?;
        assert_eq!(example_pages.len(), 2);

        let other_pages = index.get_pages_by_domain("other.com").await?;
        assert_eq!(other_pages.len(), 1);

        index.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_page_update() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let index = LinkIndex::open(temp_dir.path()).await?;

        let page_url = "https://example.com/page";
        let path1 = temp_dir.path().join("v1.html");
        let path2 = temp_dir.path().join("v2.html");

        // Initial registration
        index.register_page(page_url, &path1, &["https://link1.com".to_string()]).await?;

        let retrieved1 = index.get_local_path(page_url).await?;
        assert_eq!(retrieved1, Some(path1));

        // Update with new path and new links
        index.register_page(page_url, &path2, &["https://link2.com".to_string()]).await?;

        // Path should be updated
        let retrieved2 = index.get_local_path(page_url).await?;
        assert_eq!(retrieved2, Some(path2));

        // Links should be replaced (old cleared, new inserted)
        let outbound = index.get_outbound_links(page_url).await?;
        assert_eq!(outbound.len(), 1);
        assert!(outbound[0].contains("link2"));

        index.close().await;
        Ok(())
    }
}
