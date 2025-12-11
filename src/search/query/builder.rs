//! Query builder for constructing search queries with a fluent interface

use anyhow::Result;

use super::execution::execute_search_query;
use super::results::SearchResults;
use crate::search::engine::SearchEngine;
use crate::search::types::SearchResultItem;

/// Search query builder with fluent interface
pub struct SearchQueryBuilder {
    query: String,
    limit: usize,
    offset: usize,
    highlight: bool,
    domain_filter: Option<String>,
    crawl_id_filter: Option<String>,
}

impl SearchQueryBuilder {
    /// Create a new search query builder
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: 10,
            offset: 0,
            highlight: true,
            domain_filter: None,
            crawl_id_filter: None,
        }
    }

    /// Set the maximum number of results to return
    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set the offset for pagination
    #[must_use]
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Enable or disable result highlighting
    #[must_use]
    pub fn highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    /// Set domain filter for search results
    #[must_use]
    pub fn domain_filter(mut self, domain: Option<impl Into<String>>) -> Self {
        self.domain_filter = domain.map(|d| d.into());
        self
    }

    /// Set crawl_id filter for search results
    #[must_use]
    pub fn crawl_id_filter(mut self, crawl_id: Option<impl Into<String>>) -> Self {
        self.crawl_id_filter = crawl_id.map(|c| c.into());
        self
    }

    /// Execute the search query and return results
    pub async fn execute(self, engine: SearchEngine) -> Result<Vec<SearchResultItem>> {
        let query = self.query.clone();
        let limit = self.limit;
        let offset = self.offset;
        let highlight = self.highlight;
        let domain_filter = self.domain_filter.as_deref();
        let crawl_id_filter = self.crawl_id_filter.as_deref();

        let search_results =
            execute_search_query(&engine, &query, limit, offset, highlight, domain_filter, crawl_id_filter).await?;
        Ok(search_results.results)
    }

    /// Execute the search query and return full results with metadata
    pub async fn execute_with_metadata(self, engine: SearchEngine) -> Result<SearchResults> {
        let query = self.query.clone();
        let limit = self.limit;
        let offset = self.offset;
        let highlight = self.highlight;
        let domain_filter = self.domain_filter.as_deref();
        let crawl_id_filter = self.crawl_id_filter.as_deref();

        execute_search_query(&engine, &query, limit, offset, highlight, domain_filter, crawl_id_filter).await
    }
}
