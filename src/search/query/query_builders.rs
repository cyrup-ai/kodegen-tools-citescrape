//! Query builders for different search types
//!
//! All query builders properly tokenize query terms through the same pipeline
//! as indexed content. This ensures stemmed terms match correctly.

use anyhow::Result;
use tantivy::{
    Term,
    query::{BooleanQuery, FuzzyTermQuery, Occur, Query},
};

use crate::search::engine::SearchEngine;

// ====================
// SYNCHRONOUS VERSIONS
// ====================

/// Build a multi-field text query searching title, content, and raw_markdown.
///
/// Uses the QueryParser which automatically tokenizes through the same pipeline
/// as indexing. This ensures "ignore" matches indexed "ignor" (stemmed).
pub(crate) fn build_text_query_sync(engine: &SearchEngine, text: &str) -> Result<Box<dyn Query>> {
    let query_parser = engine.query_parser();
    
    query_parser
        .parse_query(text)
        .map_err(|e| anyhow::anyhow!("Query parsing failed: {}", e))
}

/// Build a phrase query for exact phrase matching.
///
/// Uses QueryParser with quoted text to ensure proper tokenization and
/// phrase matching with positions.
pub(crate) fn build_phrase_query_sync(
    engine: &SearchEngine,
    phrase: &str,
) -> Result<Box<dyn Query>> {
    let query_parser = engine.query_parser();
    
    // Wrap in quotes for phrase query - QueryParser handles this natively
    let quoted_phrase = format!("\"{}\"", phrase);
    
    query_parser
        .parse_query(&quoted_phrase)
        .map_err(|e| anyhow::anyhow!("Phrase query parsing failed: {}", e))
}

/// Build a boolean query using tantivy's query parser with field mapping.
///
/// Supports AND, OR, NOT operators and field-specific queries.
pub(crate) fn build_boolean_query_sync(
    engine: &SearchEngine,
    query_str: &str,
) -> Result<Box<dyn Query>> {
    let query_parser = engine.query_parser();

    // Try to parse as a standard boolean query first
    match query_parser.parse_query(query_str) {
        Ok(query) => Ok(query),
        Err(parse_error) => {
            // Log warning so users know fallback occurred
            tracing::warn!(
                query = %query_str,
                error = %parse_error,
                "Boolean query parsing failed, falling back to text search"
            );

            // Fallback to text query if boolean parsing fails
            build_text_query_sync(engine, query_str)
        }
    }
}

/// Build a field-specific query.
///
/// Uses QueryParser with field prefix syntax (e.g., "title:rust async").
/// The QueryParser automatically handles tokenization for the specified field.
pub(crate) fn build_field_query_sync(
    engine: &SearchEngine,
    field_name: &str,
    query: &str,
) -> Result<Box<dyn Query>> {
    let query_parser = engine.query_parser();
    
    // Map user-friendly field names to schema field names
    let schema_field_name = match field_name.to_lowercase().as_str() {
        "title" => "title",
        "content" | "text" => "plain_content",
        "markdown" | "raw" => "raw_markdown",
        "url" => "url",
        "path" => "path",
        _ => {
            return Err(anyhow::anyhow!("Unknown field: {field_name}"));
        }
    };
    
    // Build field-prefixed query string
    // QueryParser handles tokenization automatically
    let field_query = format!("{}:{}", schema_field_name, query);
    
    query_parser
        .parse_query(&field_query)
        .map_err(|e| anyhow::anyhow!("Field query parsing failed: {}", e))
}

/// Build a fuzzy query for handling typos.
///
/// FuzzyTermQuery is not supported by QueryParser, so we manually tokenize
/// the term through the field's tokenizer before creating the fuzzy query.
pub(crate) fn build_fuzzy_query_sync(
    engine: &SearchEngine,
    term_str: &str,
    distance: u8,
) -> Result<Box<dyn Query>> {
    let schema = engine.schema();
    let mut subqueries: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    // Helper to tokenize a term and create fuzzy queries for a field
    let mut add_fuzzy_for_field = |field: tantivy::schema::Field| {
        if let Some(mut analyzer) = engine.get_text_analyzer(field) {
            let mut token_stream = analyzer.token_stream(term_str);
            token_stream.process(&mut |token| {
                // Create fuzzy query with the TOKENIZED (stemmed) term
                let term = Term::from_field_text(field, &token.text);
                let fuzzy = FuzzyTermQuery::new(term, distance, true);
                subqueries.push((Occur::Should, Box::new(fuzzy) as Box<dyn Query>));
            });
        }
    };

    // Add fuzzy queries for title and content fields
    add_fuzzy_for_field(schema.title);
    add_fuzzy_for_field(schema.plain_content);

    if subqueries.is_empty() {
        return Err(anyhow::anyhow!(
            "Fuzzy query produced no terms after tokenization for: {}",
            term_str
        ));
    }

    Ok(Box::new(BooleanQuery::new(subqueries)))
}
