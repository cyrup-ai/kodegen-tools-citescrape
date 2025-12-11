//! Search query execution logic

use anyhow::Result;
use tantivy::collector::{Count, TopDocs};

use super::parsing::parse_query_sync;
use super::results::{SearchResults, convert_to_search_result};
use super::snippets::SnippetGenerators;
use crate::search::engine::SearchEngine;
use crate::search::errors::{SearchError, SearchResult};
use crate::search::runtime_helpers::fallback_task;
use crate::search::types::SearchResultItem;

/// Execute a search query against the index with fallback behavior
pub(crate) async fn execute_search_query(
    engine: &SearchEngine,
    query_str: &str,
    limit: usize,
    offset: usize,
    highlight: bool,
    domain_filter: Option<&str>,
    crawl_id_filter: Option<&str>,
) -> Result<SearchResults> {
    let engine = engine.clone();
    let query_str = query_str.to_string();

    // Clone values and filters for closures
    let engine_primary = engine.clone();
    let query_primary = query_str.clone();
    let engine_fallback = engine.clone();
    let query_fallback = query_str.clone();
    let domain_filter_primary = domain_filter.map(|s| s.to_string());
    let crawl_id_filter_primary = crawl_id_filter.map(|s| s.to_string());
    let domain_filter_fallback = domain_filter.map(|s| s.to_string());
    let crawl_id_filter_fallback = crawl_id_filter.map(|s| s.to_string());

    // Use fallback_task for primary and fallback search
    let result = fallback_task(
        move || async move {
            execute_search_with_features(
                engine_primary,
                query_primary,
                limit,
                offset,
                highlight,
                domain_filter_primary.as_deref(),
                crawl_id_filter_primary.as_deref(),
            )
            .await
        },
        move || async move {
            tracing::warn!("Attempting fallback search with reduced features");
            execute_search_with_features(
                engine_fallback,
                query_fallback,
                limit,
                offset,
                false,
                domain_filter_fallback.as_deref(),
                crawl_id_filter_fallback.as_deref(),
            )
            .await
        },
    )
    .await;

    // Convert SearchResult<SearchResults> to Result<SearchResults>
    result.map_err(|e| anyhow::anyhow!("{e}"))
}

/// Internal search execution with configurable features
async fn execute_search_with_features(
    engine: SearchEngine,
    query_str: String,
    limit: usize,
    offset: usize,
    highlight: bool,
    domain_filter: Option<&str>,
    crawl_id_filter: Option<&str>,
) -> SearchResult<SearchResults> {
    let reader = engine.reader();
    let searcher = reader.searcher();

    // Parse the query
    let base_query = parse_query_sync(&engine, &query_str)
        .map_err(|e| SearchError::QueryParsing(format!("Failed to parse query: {e}")))?;

    // Build combined query with domain/crawl_id filters
    let final_query: Box<dyn tantivy::query::Query> = {
        let mut subqueries: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = vec![
            (tantivy::query::Occur::Must, base_query),
        ];

        // Add domain filter if provided
        if let Some(domain) = domain_filter {
            let domain_query = tantivy::query::TermQuery::new(
                tantivy::Term::from_field_text(engine.schema().domain, domain),
                tantivy::schema::IndexRecordOption::Basic,
            );
            subqueries.push((
                tantivy::query::Occur::Must,
                Box::new(domain_query) as Box<dyn tantivy::query::Query>,
            ));
        }

        // Add crawl_id filter if provided
        if let Some(crawl_id) = crawl_id_filter {
            let crawl_id_query = tantivy::query::TermQuery::new(
                tantivy::Term::from_field_text(engine.schema().crawl_id, crawl_id),
                tantivy::schema::IndexRecordOption::Basic,
            );
            subqueries.push((
                tantivy::query::Occur::Must,
                Box::new(crawl_id_query) as Box<dyn tantivy::query::Query>,
            ));
        }

        if subqueries.len() > 1 {
            Box::new(tantivy::query::BooleanQuery::new(subqueries))
        } else {
            subqueries.into_iter().next().map(|(_, q)| q).ok_or_else(|| {
                SearchError::Other("No query built".to_string())
            })?
        }
    };

    // Parse query terms for snippet fallback
    let query_terms: Vec<String> = query_str
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect();

    // Create snippet generators if highlighting is enabled
    let generators = if highlight {
        SnippetGenerators::create(&searcher, &*final_query, engine.schema()).ok()
    } else {
        None
    };

    // Get total count
    let total_count = searcher.search(&*final_query, &Count).map_err(|e| {
        SearchError::SearchExecution(format!("Failed to count search results: {e}"))
    })?;

    // Request 3x more results for deduplication (as per task spec)
    let fetch_limit = limit * 3;
    let top_docs = searcher
        .search(&*final_query, &TopDocs::with_limit(fetch_limit).and_offset(offset))
        .map_err(|e| {
            SearchError::SearchExecution(format!("Failed to execute search query: {e}"))
        })?;

    // Deduplicate by path, keeping highest score
    use std::collections::HashMap;
    let mut path_to_result: HashMap<String, SearchResultItem> = HashMap::new();

    for (score, doc_address) in top_docs {
        let doc = searcher.doc(doc_address).map_err(|e| {
            SearchError::DocumentNotFound(format!("Failed to retrieve document: {e}"))
        })?;

        let search_result = convert_to_search_result(&doc, &engine, score, generators.as_ref(), &query_terms)
            .map_err(|e| SearchError::Other(format!("Failed to convert search result: {e}")))?;

        // Deduplicate: keep highest-scoring result per path
        let path_key = search_result.path.clone();
        match path_to_result.entry(path_key) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(search_result);
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if score > e.get().score {
                    e.insert(search_result);
                }
            }
        }
    }

    // Convert to sorted vec (by score descending)
    let mut results: Vec<SearchResultItem> = path_to_result.into_values().collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Apply limit after deduplication
    results.truncate(limit);

    Ok(SearchResults {
        results,
        total_count,
        query: query_str,
        offset,
        limit,
    })
}
