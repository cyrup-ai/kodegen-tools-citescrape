//! Event-driven link rewriter using lol_html for streaming HTML processing.
//!
//! This module handles rewriting links in crawled HTML files:
//! 1. When a page is saved, rewrite its outbound links to point to local copies (if they exist)
//! 2. When a page is saved, retroactively update all existing pages that link TO this new page
//!
//! The rewriting is event-driven: triggered AFTER pages are saved to disk.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use lol_html::{HtmlRewriter, Settings, element};
use dashmap::DashMap;
use regex::Regex;
use tokio::sync::{Mutex, Semaphore};
#[allow(unused_imports)]  // Used by rewrite_single_link_in_markdown (Aho-Corasick for single-link case)
use aho_corasick::AhoCorasick;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use crate::link_index::{LinkIndex, normalize_url};

// =============================================================================
// MARKDOWN LINK PATTERNS - Regex for comprehensive link rewriting
// =============================================================================

/// Matches inline markdown links: ](url) or ](url "title") or ](<url>)
/// Captures the URL in group 1
/// Bounded quantifiers prevent catastrophic backtracking (max 2000 char URL)
static INLINE_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Pattern breakdown:
    // \]\(       - literal ](
    // <?         - optional opening angle bracket
    // ([^>\s\)"']{1,2000}) - capture URL (bounded, no whitespace/>/)/"/') 
    // >?         - optional closing angle bracket
    // (?:[\s\t]+["'][^"']*["'])? - optional title with quotes
    // \)         - literal )
    Regex::new(r#"\]\(<?([^>\s\)"']{1,2000})>?(?:[\s\t]+["'][^"']*["'])?\)"#)
        .expect("INLINE_LINK_RE: hardcoded regex is valid")
});

/// Matches reference-style link definitions: [ref]: url or [ref]: <url> "title"
/// Captures: group 1 = reference label, group 2 = URL
static REFERENCE_DEF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\[([^\]]{1,200})\]:\s*<?([^>\s]{1,2000})>?(?:\s+["'][^"']*["'])?$"#)
        .expect("REFERENCE_DEF_RE: hardcoded regex is valid")
});

/// Matches autolinks: <https://example.com> or <http://example.com>
/// Captures the URL in group 1
static AUTOLINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<(https?://[^>]{1,2000})>")
        .expect("AUTOLINK_RE: hardcoded regex is valid")
});

/// Result of a link rewriting operation.
#[derive(Debug, Clone, Default)]
pub struct RewriteResult {
    /// Number of outbound links rewritten in the new page
    pub outbound_rewritten: usize,
    /// Number of existing pages updated with links to this new page
    pub inbound_updated: usize,
    /// Errors encountered during inbound updates (non-fatal)
    pub inbound_errors: Vec<String>,
}

/// Event-driven link rewriter.
///
/// Uses lol_html for efficient streaming HTML rewriting.
/// Coordinates with LinkIndex for URL → path lookups.
#[derive(Clone)]
pub struct LinkRewriter {
    index: Arc<LinkIndex>,
    #[allow(dead_code)] // Reserved for future use (e.g., path calculations)
    output_dir: PathBuf,
    /// Limit concurrent file rewrites to prevent fd exhaustion
    rewrite_semaphore: Arc<Semaphore>,
    /// Per-file locks to serialize concurrent rewrites to the SAME file
    /// Key: Canonical file path, Value: Mutex guard for that file
    file_locks: Arc<DashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl LinkRewriter {
    /// Create a new LinkRewriter.
    ///
    /// # Arguments
    /// * `index` - Shared LinkIndex for URL → path lookups
    /// * `output_dir` - Base output directory for relative path calculation
    pub fn new(index: Arc<LinkIndex>, output_dir: PathBuf) -> Self {
        Self {
            index,
            output_dir,
            // Limit to 32 concurrent file rewrites to avoid fd exhaustion
            rewrite_semaphore: Arc::new(Semaphore::new(32)),
            file_locks: Arc::new(DashMap::new()),
        }
    }

    /// Acquire a mutex lock for a specific file path.
    ///
    /// Uses DashMap for lock-free concurrent access to DIFFERENT files,
    /// while serializing access to the SAME file via per-path Mutex.
    ///
    /// # Lock Semantics
    /// - First access to a path creates a new Mutex (lazy initialization)
    /// - Subsequent accesses return the same Mutex (via Arc clone)
    /// - Lock must be held across entire read-modify-write cycle
    fn get_file_lock(&self, path: &Path) -> Arc<Mutex<()>> {
        self.file_locks
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Get the count of pages registered in the index.
    /// Used for progress reporting.
    pub async fn get_registration_count(&self) -> usize {
        self.index.page_count().await.unwrap_or(0) as usize
    }

    /// Main entry point: called AFTER HTML is saved to disk.
    ///
    /// This performs the full event-driven link rewriting:
    /// 1. Registers the page in the index (atomic)
    /// 2. Rewrites outbound links in the new page to point to existing local copies
    /// 3. Retroactively updates all pages that link TO this newly saved page
    ///
    /// # Arguments
    /// * `page_url` - The canonical URL of the saved page
    /// * `local_path` - The local file path where the HTML was saved
    /// * `outbound_links` - All HTTP/HTTPS links found in the page
    ///
    /// # Returns
    /// RewriteResult with statistics about the rewriting operation
    pub async fn on_page_saved(
        &self,
        page_url: &str,
        local_path: &Path,
        outbound_links: Vec<String>,
    ) -> Result<RewriteResult> {
        let mut result = RewriteResult::default();

        // 1. Register page and its outbound links in the index (atomic transaction)
        self.index
            .register_page(page_url, local_path, &outbound_links)
            .await
            .context("Failed to register page in link index")?;

        // 2. Check which outbound links have local copies
        let existing_destinations = self.index.filter_existing(&outbound_links).await?;

        // 3. Rewrite outbound links in the NEW page's HTML (if any exist locally)
        if !existing_destinations.is_empty() {
            let count = self
                .rewrite_outbound_links(page_url, local_path, &existing_destinations)
                .await
                .context("Failed to rewrite outbound links")?;
            result.outbound_rewritten = count;
        }

        // 4. Find all pages that link TO this newly saved page
        let inbound = self.index.get_inbound_links(page_url).await?;

        // 5. Rewrite link to this page in all those files (parallel, bounded, file-locked)
        if !inbound.is_empty() {
            let page_url = page_url.to_string();

            let update_futures: Vec<_> = inbound
                .into_iter()
                .map(|(source_url, source_path)| {
                    let sem = self.rewrite_semaphore.clone();
                    let page_url = page_url.clone();
                    let index = self.index.clone();
                    let file_locks = self.file_locks.clone();

                    async move {
                        // 1. Acquire global concurrency permit (limits total parallel I/O)
                        let _permit = sem.acquire().await.map_err(|e| anyhow!("Semaphore error: {}", e))?;

                        // 2. Acquire per-file lock (serializes access to same file)
                        let file_lock = file_locks
                            .entry(source_path.clone())
                            .or_insert_with(|| Arc::new(Mutex::new(())))
                            .clone();
                        let _file_guard = file_lock.lock().await;

                        // 3. Perform rewrite while holding both locks
                        rewrite_single_link(&source_url, &source_path, &page_url, &index).await
                    }
                })
                .collect();

            let results = futures::future::join_all(update_futures).await;

            for res in results {
                match res {
                    Ok(_) => result.inbound_updated += 1,
                    Err(e) => {
                        log::warn!("Failed to rewrite inbound link: {e}");
                        result.inbound_errors.push(e.to_string());
                    }
                }
            }
        }

        log::debug!(
            "Link rewrite complete for {}: {} outbound, {} inbound updated",
            page_url,
            result.outbound_rewritten,
            result.inbound_updated
        );

        Ok(result)
    }

    /// Rewrite all links in a file that point to known local destinations.
    ///
    /// # Arguments
    /// * `page_url` - The URL of the page being rewritten (for resolving relative links)
    /// * `file_path` - Path to the HTML file to rewrite
    /// * `destinations` - Set of normalized URLs that have local copies
    ///
    /// # Returns
    /// Number of links rewritten
    async fn rewrite_outbound_links(
        &self,
        page_url: &str,
        file_path: &Path,
        destinations: &HashSet<String>,
    ) -> Result<usize> {
        // Build URL → relative path map
        let mut url_to_relative: HashMap<String, String> = HashMap::new();

        for url in destinations {
            // FIX: Compute dest_path using get_mirror_path() instead of reading from DB
            // This ensures BOTH paths use the SAME function with SAME output_dir, guaranteeing type consistency
            let dest_path = crate::utils::get_mirror_path(url, &self.output_dir, "index.html").await?;
            
            if let Some(relative) = compute_relative_path(file_path, &dest_path) {
                url_to_relative.insert(url.clone(), relative);
            }
        }

        if url_to_relative.is_empty() {
            return Ok(0);
        }

        // Acquire file lock before any file I/O
        let file_lock = self.get_file_lock(file_path);
        let _guard = file_lock.lock().await;

        // Read, rewrite, write (now protected by lock)
        let html = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read HTML file")?;

        let (rewritten, count) = rewrite_links_in_html(&html, page_url, &url_to_relative)?;

        if count > 0 {
            tokio::fs::write(file_path, rewritten)
                .await
                .context("Failed to write rewritten HTML")?;
            
            // RESTORE: Markdown link rewriting (broken in original implementation)
            // HTML and markdown share directory: /path/page/index.{html,md}
            let md_path = file_path.with_extension("md");
            if tokio::fs::try_exists(&md_path).await.unwrap_or(false) {
                match rewrite_links_in_markdown(&md_path, &url_to_relative).await {
                    Ok(md_count) if md_count > 0 => {
                        log::debug!(
                            "Markdown link rewriting: {} links rewritten in {:?}",
                            md_count,
                            md_path
                        );
                    }
                    Ok(_) => {}, // No markdown links needed rewriting
                    Err(e) => {
                        log::warn!(
                            "Markdown link rewriting failed for {:?}: {}",
                            md_path,
                            e
                        );
                        // Non-fatal: HTML is rewritten, markdown rewrite is best-effort
                    }
                }
            }
        }

        Ok(count)
    }

    /// Get reference to the underlying LinkIndex.
    pub fn index(&self) -> &Arc<LinkIndex> {
        &self.index
    }
}

/// Rewrite a single link in a source file to point to a newly saved target.
///
/// This is used for retroactive inbound link updates.
/// Optimized to avoid HashMap overhead for the common single-link case.
async fn rewrite_single_link(
    source_url: &str,
    source_path: &Path,
    target_url: &str,
    index: &LinkIndex,
) -> Result<()> {
    // Compute target path deterministically from URL for consistency
    let target_path = crate::utils::get_mirror_path(target_url, index.output_dir(), "index.html").await?;

    let relative = compute_relative_path(source_path, &target_path)
        .ok_or_else(|| anyhow!("Cannot compute relative path from {:?} to {:?}", source_path, target_path))?;

    let html = tokio::fs::read_to_string(source_path)
        .await
        .context("Failed to read source file")?;

    // Pre-normalize target_url ONCE (eliminates redundant normalization)
    let normalized_target = normalize_url(target_url);

    // Use optimized single-link function - NO HASHMAP ALLOCATION
    let (rewritten, count) = rewrite_single_link_in_html(&html, source_url, &normalized_target, &relative)?;

    if count > 0 {
        tokio::fs::write(source_path, rewritten)
            .await
            .context("Failed to write rewritten source file")?;

        // Retroactive markdown inbound link updates (also optimized)
        let md_path = source_path.with_extension("md");
        if tokio::fs::try_exists(&md_path).await.unwrap_or(false) {
            // Use optimized single-link markdown function - NO HASHMAP
            if let Err(e) = rewrite_single_link_in_markdown(&md_path, &normalized_target, &relative).await {
                log::warn!(
                    "Retroactive markdown link update failed for {:?}: {}",
                    md_path,
                    e
                );
            }
        }
    }

    Ok(())
}

/// Compute relative path from source file to destination file.
///
/// Returns None if the path cannot be computed (e.g., different drives on Windows).
fn compute_relative_path(from_file: &Path, to_file: &Path) -> Option<String> {
    // Get the directory containing the source file
    let from_dir = from_file.parent()?;

    // Compute relative path from source directory to destination file
    pathdiff::diff_paths(to_file, from_dir).map(|p| p.to_string_lossy().to_string())
}

/// Core HTML rewriting using lol_html (streaming, efficient).
///
/// Rewrites href attributes on <a> tags when they match URLs in the map.
/// Resolves relative URLs against base_url before matching.
///
/// # Arguments
/// * `html` - The HTML content to rewrite
/// * `base_url` - The URL of the page being rewritten (for resolving relative links)
/// * `url_to_relative` - Map of normalized URL → relative local path
///
/// # Returns
/// Tuple of (rewritten HTML, number of links rewritten)
fn rewrite_links_in_html(
    html: &str,
    base_url: &str,
    url_to_relative: &HashMap<String, String>,
) -> Result<(String, usize)> {
    let mut output = Vec::with_capacity(html.len());
    let rewrite_count = std::sync::atomic::AtomicUsize::new(0);

    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                // Rewrite <a href="...">
                element!("a[href]", |el| {
                    if let Some(href) = el.get_attribute("href") {
                        // Resolve relative URLs against base before normalizing
                        let absolute_url = if let Ok(base) = url::Url::parse(base_url) {
                            // Try to resolve href against base (handles both absolute and relative)
                            match base.join(&href) {
                                Ok(resolved) => resolved.to_string(),
                                Err(_) => href.clone(),  // If join fails, use href as-is
                            }
                        } else {
                            href.clone()  // If base URL invalid, use href as-is
                        };
                        
                        let normalized = normalize_url(&absolute_url);
                        if let Some(relative) = url_to_relative.get(&normalized) {
                            el.set_attribute("href", relative)?;
                            rewrite_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| output.extend_from_slice(c),
    );

    rewriter
        .write(html.as_bytes())
        .map_err(|e| anyhow!("HTML rewrite error: {}", e))?;
    rewriter
        .end()
        .map_err(|e| anyhow!("HTML rewrite finalization error: {}", e))?;

    let result = String::from_utf8(output).context("Invalid UTF-8 in rewritten HTML")?;
    let count = rewrite_count.load(std::sync::atomic::Ordering::Relaxed);

    Ok((result, count))
}

/// Optimized single-link HTML rewriting without HashMap overhead.
///
/// For retroactive inbound link updates where exactly one link needs rewriting,
/// this avoids the ~48+ byte HashMap allocation overhead by using direct
/// string comparison instead of HashMap lookup.
///
/// # Arguments
/// * `html` - The HTML content to rewrite
/// * `base_url` - The URL of the page being rewritten (for resolving relative links)
/// * `target_url` - The normalized URL to match (caller must pre-normalize)
/// * `replacement` - The relative local path to substitute
///
/// # Returns
/// Tuple of (rewritten HTML, number of links rewritten)
fn rewrite_single_link_in_html(
    html: &str,
    base_url: &str,
    target_url: &str,
    replacement: &str,
) -> Result<(String, usize)> {
    let mut output = Vec::with_capacity(html.len());
    let rewrite_count = std::sync::atomic::AtomicUsize::new(0);

    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("a[href]", |el| {
                    if let Some(href) = el.get_attribute("href") {
                        // Resolve relative URLs against base before normalizing
                        let absolute_url = if let Ok(base) = url::Url::parse(base_url) {
                            match base.join(&href) {
                                Ok(resolved) => resolved.to_string(),
                                Err(_) => href.clone(),
                            }
                        } else {
                            href.clone()
                        };

                        let normalized = normalize_url(&absolute_url);
                        // DIRECT STRING COMPARISON - no HashMap lookup overhead
                        if normalized == target_url {
                            el.set_attribute("href", replacement)?;
                            rewrite_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| output.extend_from_slice(c),
    );

    rewriter
        .write(html.as_bytes())
        .map_err(|e| anyhow!("HTML rewrite error: {}", e))?;
    rewriter
        .end()
        .map_err(|e| anyhow!("HTML rewrite finalization error: {}", e))?;

    let result = String::from_utf8(output).context("Invalid UTF-8 in rewritten HTML")?;
    let count = rewrite_count.load(std::sync::atomic::Ordering::Relaxed);

    Ok((result, count))
}

/// Normalize URL for lookup - strips fragment and normalizes for consistent matching.
/// 
/// This addresses the root cause: URLs like `https://example.com/page#section` 
/// must match `https://example.com/page` in the url_to_relative map.
fn normalize_url_for_lookup(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) => {
            parsed.set_fragment(None);
            normalize_url(parsed.as_ref())
        }
        Err(_) => normalize_url(url)
    }
}

/// Convert HTML-relative path to markdown-relative path.
/// 
/// The url_to_relative map contains HTML paths; markdown files need .md extensions.
#[inline]
fn html_path_to_markdown(html_path: &str) -> String {
    html_path
        .replace("/index.html", "/index.md")
        .replace(".html", ".md")
}

/// Rewrite markdown links via regex-based pattern matching with URL normalization.
///
/// Architecture:
/// - Reads file line-by-line with O(1) memory footprint
/// - Uses static compiled regexes for pattern matching
/// - Normalizes URLs before lookup (strips fragments, normalizes case)
/// - Atomic file replacement via POSIX rename(2) syscall
///
/// Handles all markdown link patterns:
/// - Inline links: `](url)`, `](url "title")`, `](<url>)`
/// - Reference definitions: `[ref]: url`, `[ref]: url "title"`
/// - Autolinks: `<https://example.com>`
///
/// # Arguments
/// * `file_path` - Path to markdown file requiring link rewriting
/// * `url_to_relative` - Map of normalized URL -> relative HTML path (reused from HTML rewriting)
///
/// # Returns
/// Number of links successfully rewritten
async fn rewrite_links_in_markdown(
    file_path: &Path,
    url_to_relative: &HashMap<String, String>,
) -> Result<usize> {
    use tokio::fs::File;
    
    // Pre-compute normalized lookup map: normalize URLs + convert HTML paths to markdown
    let normalized_map: HashMap<String, String> = url_to_relative
        .iter()
        .map(|(url, html_path)| {
            (normalize_url_for_lookup(url), html_path_to_markdown(html_path))
        })
        .collect();
    
    if normalized_map.is_empty() {
        return Ok(0);
    }
    
    // Open input file for streaming read
    let input_file = File::open(file_path).await
        .context("Failed to open markdown file for reading")?;
    let reader = BufReader::new(input_file);
    
    // Create temporary output file for atomic replacement
    let temp_path = file_path.with_extension("md.tmp");
    let output_file = File::create(&temp_path).await
        .context("Failed to create temporary markdown file")?;
    let mut writer = BufWriter::new(output_file);
    
    let mut rewrite_count = 0;
    let mut lines = reader.lines();
    
    // Stream through file one line at a time (O(1) memory)
    while let Some(line_result) = lines.next_line().await? {
        let mut line = line_result;
        
        // Pattern 1: Inline links ](url) or ](url "title") or ](<url>)
        let mut inline_count = 0;
        line = INLINE_LINK_RE.replace_all(&line, |caps: &regex::Captures| {
            let url = &caps[1];
            let normalized = normalize_url_for_lookup(url);
            if let Some(md_relative) = normalized_map.get(&normalized) {
                inline_count += 1;
                format!("]({})", md_relative)
            } else {
                caps[0].to_string()
            }
        }).to_string();
        rewrite_count += inline_count;
        
        // Pattern 2: Reference definitions [ref]: url (only at start of line)
        if let Some(caps) = REFERENCE_DEF_RE.captures(&line) {
            let ref_label = &caps[1];
            let url = &caps[2];
            let normalized = normalize_url_for_lookup(url);
            if let Some(md_relative) = normalized_map.get(&normalized) {
                rewrite_count += 1;
                line = format!("[{}]: {}", ref_label, md_relative);
            }
        }
        
        // Pattern 3: Autolinks <https://...>
        let mut autolink_count = 0;
        line = AUTOLINK_RE.replace_all(&line, |caps: &regex::Captures| {
            let url = &caps[1];
            let normalized = normalize_url_for_lookup(url);
            if let Some(md_relative) = normalized_map.get(&normalized) {
                autolink_count += 1;
                // Autolinks become regular markdown links when rewritten to local paths
                // Use filename as link text for readability
                let link_text = md_relative.rsplit('/').next().unwrap_or(md_relative);
                format!("[{}]({})", link_text, md_relative)
            } else {
                caps[0].to_string()
            }
        }).to_string();
        rewrite_count += autolink_count;
        
        // Write rewritten line to temporary file
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }
    
    // Flush buffer to ensure all data written
    writer.flush().await?;
    drop(writer);  // Explicit close before rename
    
    // Atomic file replacement (crash-safe)
    if rewrite_count > 0 {
        tokio::fs::rename(&temp_path, file_path).await
            .context("Failed to atomically replace markdown file")?;
    } else {
        // No rewrites performed - cleanup temporary file
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
    
    Ok(rewrite_count)
}

/// Optimized single-link markdown rewriting without HashMap overhead.
///
/// For retroactive inbound link updates where exactly one link needs rewriting,
/// this avoids HashMap allocation and iteration overhead.
///
/// # Arguments
/// * `file_path` - Path to markdown file requiring link rewriting
/// * `target_url` - The normalized URL to match (caller must pre-normalize)
/// * `html_relative` - The HTML-style relative path (will be converted to .md)
///
/// # Returns
/// Number of links successfully rewritten
async fn rewrite_single_link_in_markdown(
    file_path: &Path,
    target_url: &str,
    html_relative: &str,
) -> Result<usize> {
    use tokio::fs::File;

    // Transform HTML path to markdown path once (not per-URL like HashMap version)
    let md_relative = html_relative
        .replace("/index.html", "/index.md")
        .replace(".html", ".md");

    // Open input file for streaming read
    let input_file = File::open(file_path)
        .await
        .context("Failed to open markdown file for reading")?;
    let reader = BufReader::new(input_file);

    // Create temporary output file for atomic replacement
    let temp_path = file_path.with_extension("md.tmp");
    let output_file = File::create(&temp_path)
        .await
        .context("Failed to create temporary markdown file")?;
    let mut writer = BufWriter::new(output_file);

    let mut rewrite_count = 0;
    let mut lines = reader.lines();

    // Build patterns once (not per-line like HashMap iteration)
    let pattern = format!("]({})", target_url);
    let replacement = format!("]({})", md_relative);
    let pattern_space = format!("]({} ", target_url);
    let replacement_space = format!("]({} ", md_relative);

    // Stream through file one line at a time (O(1) memory)
    while let Some(line_result) = lines.next_line().await? {
        let mut line = line_result;

        // Direct pattern matching - no HashMap iteration
        if line.contains(&pattern) {
            line = line.replace(&pattern, &replacement);
            rewrite_count += 1;
        }
        if line.contains(&pattern_space) {
            line = line.replace(&pattern_space, &replacement_space);
        }

        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }

    // Flush buffer to ensure all data written
    writer.flush().await?;
    drop(writer); // Explicit close before rename

    // Atomic file replacement (crash-safe)
    if rewrite_count > 0 {
        tokio::fs::rename(&temp_path, file_path)
            .await
            .context("Failed to atomically replace markdown file")?;
    } else {
        // No rewrites performed - cleanup temporary file
        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    Ok(rewrite_count)
}

/// Extract all HTTP/HTTPS links from HTML.
///
/// This is used to find outbound links before calling on_page_saved.
/// Only extracts links from <a href="..."> tags.
pub fn extract_links_from_html(html: &str, base_url: &str) -> Vec<String> {
    let mut links = Vec::new();

    // Parse base URL for resolving relative links
    let base = match url::Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return links,
    };

    // Use scraper for extraction (simpler than lol_html for read-only)
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("a[href]").unwrap();

    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            // Skip empty, javascript:, mailto:, tel:, and fragment-only links
            let href = href.trim();
            if href.is_empty()
                || href.starts_with("javascript:")
                || href.starts_with("mailto:")
                || href.starts_with("tel:")
                || href.starts_with('#')
            {
                continue;
            }

            // Resolve relative URLs against base
            match base.join(href) {
                Ok(resolved) => {
                    // Only include http/https links
                    if resolved.scheme() == "http" || resolved.scheme() == "https" {
                        links.push(resolved.to_string());
                    }
                }
                Err(_) => {
                    // If it looks like an absolute HTTP URL, include it directly
                    if href.starts_with("http://") || href.starts_with("https://") {
                        links.push(href.to_string());
                    }
                }
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    links.retain(|url| seen.insert(url.clone()));

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_relative_path() {
        // Same directory
        let from = PathBuf::from("/output/pages/page1.html");
        let to = PathBuf::from("/output/pages/page2.html");
        assert_eq!(compute_relative_path(&from, &to), Some("page2.html".to_string()));

        // Subdirectory
        let from = PathBuf::from("/output/pages/index.html");
        let to = PathBuf::from("/output/pages/docs/guide.html");
        assert_eq!(compute_relative_path(&from, &to), Some("docs/guide.html".to_string()));

        // Parent directory
        let from = PathBuf::from("/output/pages/docs/guide.html");
        let to = PathBuf::from("/output/pages/index.html");
        assert_eq!(compute_relative_path(&from, &to), Some("../index.html".to_string()));

        // Different branches
        let from = PathBuf::from("/output/pages/blog/post.html");
        let to = PathBuf::from("/output/pages/docs/guide.html");
        assert_eq!(compute_relative_path(&from, &to), Some("../docs/guide.html".to_string()));
    }

    #[test]
    fn test_rewrite_links_in_html() {
        let html = r#"
            <html>
            <body>
                <a href="https://example.com/page1">Link 1</a>
                <a href="https://example.com/page2">Link 2</a>
                <a href="https://other.com/external">External</a>
            </body>
            </html>
        "#;

        let mut url_map = HashMap::new();
        url_map.insert(normalize_url("https://example.com/page1"), "page1.html".to_string());
        url_map.insert(normalize_url("https://example.com/page2"), "../docs/page2.html".to_string());

        let base_url = "https://example.com/index.html";
        let (rewritten, count) = rewrite_links_in_html(html, base_url, &url_map).unwrap();

        assert_eq!(count, 2);
        assert!(rewritten.contains(r#"href="page1.html""#));
        assert!(rewritten.contains(r#"href="../docs/page2.html""#));
        // External link unchanged
        assert!(rewritten.contains(r#"href="https://other.com/external""#));
    }

    #[test]
    fn test_rewrite_preserves_other_attributes() {
        let html = r#"<a href="https://example.com/page" class="btn" id="link1" target="_blank">Click</a>"#;

        let mut url_map = HashMap::new();
        url_map.insert(normalize_url("https://example.com/page"), "local.html".to_string());

        let base_url = "https://example.com/index.html";
        let (rewritten, count) = rewrite_links_in_html(html, base_url, &url_map).unwrap();

        assert_eq!(count, 1);
        assert!(rewritten.contains(r#"href="local.html""#));
        assert!(rewritten.contains(r#"class="btn""#));
        assert!(rewritten.contains(r#"id="link1""#));
        assert!(rewritten.contains(r#"target="_blank""#));
    }

    #[test]
    fn test_extract_links_from_html() {
        let html = r##"
            <html>
            <body>
                <a href="https://example.com/page1">Absolute</a>
                <a href="/relative/page">Relative</a>
                <a href="sibling.html">Sibling</a>
                <a href="javascript:void(0)">JS</a>
                <a href="mailto:test@example.com">Email</a>
                <a href="#section">Fragment</a>
                <a href="">Empty</a>
            </body>
            </html>
        "##;

        let links = extract_links_from_html(html, "https://example.com/docs/index.html");

        assert_eq!(links.len(), 3);
        assert!(links.contains(&"https://example.com/page1".to_string()));
        assert!(links.contains(&"https://example.com/relative/page".to_string()));
        assert!(links.contains(&"https://example.com/docs/sibling.html".to_string()));
    }

    #[test]
    fn test_extract_links_deduplicates() {
        let html = r#"
            <a href="https://example.com/page">Link 1</a>
            <a href="https://example.com/page">Link 2 (same)</a>
            <a href="https://example.com/other">Link 3</a>
        "#;

        let links = extract_links_from_html(html, "https://example.com/");

        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_url_normalization_in_rewrite() {
        let html = r#"<a href="HTTPS://Example.COM/Page/">Link</a>"#;

        let mut url_map = HashMap::new();
        // Normalized form
        url_map.insert("https://example.com/Page".to_string(), "page.html".to_string());

        let base_url = "https://example.com/index.html";
        let (rewritten, count) = rewrite_links_in_html(html, base_url, &url_map).unwrap();

        assert_eq!(count, 1);
        assert!(rewritten.contains(r#"href="page.html""#));
    }
}
