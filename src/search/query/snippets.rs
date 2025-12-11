//! Diversity-aware snippet generation
//!
//! Unlike Tantivy's default SnippetGenerator which scores fragments by total term frequency,
//! this implementation prioritizes TERM DIVERSITY - fragments containing more UNIQUE query
//! terms are scored higher than fragments with many occurrences of fewer terms.
//!
//! Example: For query "resume conversation session":
//! - Fragment A: 5x "session" → diversity=1, frequency=5
//! - Fragment B: 1x "resume" + 1x "conversation" + 1x "session" → diversity=3, frequency=3
//!
//! Tantivy would pick A (higher total frequency), we pick B (higher diversity).

use std::collections::{BTreeMap, HashSet};
use std::ops::Range;

use anyhow::Result;
use html_escape::encode_text;
use tantivy::query::Query;
use tantivy::schema::Value;
use tantivy::tokenizer::TextAnalyzer;
use tantivy::{Score, Searcher, TantivyDocument, Term};

use crate::search::engine::SearchEngine;
use crate::search::schema::SearchSchema;

const DEFAULT_MAX_CHARS: usize = 250;
const DIVERSITY_WEIGHT: f32 = 10.0; // Heavy weight for unique term count
const FREQUENCY_WEIGHT: f32 = 1.0;  // Light weight for total occurrences

/// Fragment candidate with diversity-aware scoring
#[derive(Debug)]
struct FragmentCandidate {
    start_offset: usize,
    stop_offset: usize,
    unique_terms: HashSet<String>,
    term_positions: Vec<(String, Range<usize>)>,
    diversity_score: f32,
    frequency_score: f32,
}

impl FragmentCandidate {
    fn new(start_offset: usize) -> Self {
        Self {
            start_offset,
            stop_offset: start_offset,
            unique_terms: HashSet::new(),
            term_positions: Vec::new(),
            diversity_score: 0.0,
            frequency_score: 0.0,
        }
    }

    fn add_term(&mut self, term: &str, term_score: f32, offset_from: usize, offset_to: usize) {
        self.stop_offset = offset_to;
        
        // Track unique terms for diversity scoring
        let is_new_term = self.unique_terms.insert(term.to_lowercase());
        if is_new_term {
            self.diversity_score += DIVERSITY_WEIGHT * term_score;
        }
        
        // Track all occurrences for frequency scoring and highlighting
        self.frequency_score += FREQUENCY_WEIGHT * term_score;
        self.term_positions.push((term.to_string(), offset_from..offset_to));
    }

    /// Combined score prioritizing diversity over frequency
    fn total_score(&self) -> f32 {
        self.diversity_score + self.frequency_score
    }

    fn highlighted_ranges(&self) -> Vec<Range<usize>> {
        self.term_positions
            .iter()
            .map(|(_, range)| {
                // Adjust to fragment-relative offsets
                let start = range.start.saturating_sub(self.start_offset);
                let end = range.end.saturating_sub(self.start_offset);
                start..end
            })
            .collect()
    }
}

/// Diversity-aware snippet generator
pub(crate) struct SnippetGenerators {
    terms: BTreeMap<String, Score>,
    tokenizer: Option<TextAnalyzer>,
    max_chars: usize,
}

impl SnippetGenerators {
    pub(crate) fn create(
        searcher: &Searcher,
        query: &dyn Query,
        schema: &SearchSchema,
    ) -> Result<Self> {
        // Extract query terms with IDF scores
        // Include terms from both title and plain_content fields since QueryParser
        // creates terms for all default search fields
        let mut term_set: std::collections::BTreeSet<&Term> = std::collections::BTreeSet::new();
        query.query_terms(&mut |term, _| {
            if term.field() == schema.plain_content || term.field() == schema.title {
                term_set.insert(term);
            }
        });

        let mut terms: BTreeMap<String, Score> = BTreeMap::new();
        for term in term_set {
            if let Some(term_str) = term.value().as_str() {
                let doc_freq = searcher.doc_freq(term).unwrap_or(0);
                if doc_freq > 0 {
                    // IDF-based scoring like Tantivy
                    let score = 1.0 / (1.0 + doc_freq as Score);
                    tracing::debug!(term = %term_str, doc_freq, score, "Extracted query term");
                    terms.insert(term_str.to_lowercase(), score);
                }
            }
        }
        tracing::debug!(term_count = terms.len(), terms = ?terms.keys().collect::<Vec<_>>(), "Total query terms extracted");

        // Get tokenizer for the content field
        let tokenizer = searcher.index().tokenizer_for_field(schema.plain_content).ok();

        Ok(Self {
            terms,
            tokenizer,
            max_chars: DEFAULT_MAX_CHARS,
        })
    }

    pub(crate) fn generate_snippet(&self, doc: &TantivyDocument, engine: &SearchEngine) -> String {
        let schema = engine.schema();

        // Get plain content from document
        let content = doc
            .get_first(schema.plain_content)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if content.is_empty() || self.terms.is_empty() {
            // Fallback to stored snippet
            return doc
                .get_first(schema.snippet)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }

        // Generate diversity-aware snippet
        if let Some(snippet) = self.generate_diversity_snippet(content) {
            return snippet;
        }

        // Fallback to stored snippet
        doc.get_first(schema.snippet)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn generate_diversity_snippet(&self, text: &str) -> Option<String> {
        let tokenizer = self.tokenizer.as_ref()?;
        let mut tokenizer = tokenizer.clone();
        
        tracing::debug!(text_len = text.len(), terms = ?self.terms.keys().collect::<Vec<_>>(), "Generating snippet from content");

        // Find all fragments
        let fragments = self.search_fragments(&mut tokenizer, text);
        if fragments.is_empty() {
            return None;
        }

        // Select best fragment by diversity-weighted score
        let best = fragments
            .iter()
            .max_by(|a, b| {
                a.total_score()
                    .partial_cmp(&b.total_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;

        // Extract and highlight the fragment
        let fragment_text = &text[best.start_offset..best.stop_offset];
        let highlighted = self.highlight_fragment(fragment_text, &best.highlighted_ranges());

        Some(highlighted)
    }

    fn search_fragments(&self, tokenizer: &mut TextAnalyzer, text: &str) -> Vec<FragmentCandidate> {
        let mut token_stream = tokenizer.token_stream(text);
        let mut fragment = FragmentCandidate::new(0);
        let mut fragments = Vec::new();
        let mut matched_count = 0;
        let mut total_tokens = 0;

        while let Some(token) = token_stream.next() {
            total_tokens += 1;
            // Log first 20 tokens and any containing "resum" or "convers"
            if total_tokens <= 20 || token.text.contains("resum") || token.text.contains("convers") || token.text.contains("session") {
                tracing::debug!(token_num = total_tokens, token_text = %token.text, offset_from = token.offset_from, "Token produced");
            }
            // Start new fragment if current one exceeds max chars
            if (token.offset_to - fragment.start_offset) > self.max_chars {
                if !fragment.unique_terms.is_empty() {
                    fragments.push(fragment);
                }
                fragment = FragmentCandidate::new(token.offset_from);
            }

            fragment.stop_offset = token.offset_to;

            // Check if token matches any query term
            let token_lower = token.text.to_lowercase();
            // Debug: check specific tokens
            if token_lower == "resum" || token_lower == "convers" || token_lower == "session" {
                tracing::debug!(token = %token_lower, terms = ?self.terms.keys().collect::<Vec<_>>(), has_match = self.terms.contains_key(&token_lower), "Checking special token");
            }
            if let Some(&score) = self.terms.get(&token_lower) {
                matched_count += 1;
                tracing::debug!(token = %token_lower, offset_from = token.offset_from, offset_to = token.offset_to, "Token matched query term");
                fragment.add_term(&token_lower, score, token.offset_from, token.offset_to);
            }
        }
        tracing::debug!(total_tokens, matched_count, fragments_count = fragments.len(), "Fragment search complete");

        // Don't forget the last fragment
        if !fragment.unique_terms.is_empty() {
            fragments.push(fragment);
        }

        fragments
    }

    fn highlight_fragment(&self, text: &str, ranges: &[Range<usize>]) -> String {
        // Sort and merge overlapping ranges
        let mut sorted_ranges = ranges.to_vec();
        sorted_ranges.sort_by_key(|r| (r.start, r.end));
        
        let merged = merge_ranges(&sorted_ranges);

        // Build highlighted HTML
        let mut html = String::new();
        let mut pos = 0;

        for range in merged {
            // Clamp ranges to text bounds
            let start = range.start.min(text.len());
            let end = range.end.min(text.len());
            
            if start > pos {
                html.push_str(&encode_text(&text[pos..start]));
            }
            if end > start {
                html.push_str("<b>");
                html.push_str(&encode_text(&text[start..end]));
                html.push_str("</b>");
            }
            pos = end;
        }

        if pos < text.len() {
            html.push_str(&encode_text(&text[pos..]));
        }

        html
    }
}

fn merge_ranges(ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut result: Vec<Range<usize>> = Vec::new();
    
    for range in ranges {
        if let Some(last) = result.last_mut()
            && last.end >= range.start
        {
            last.end = last.end.max(range.end);
            continue;
        }
        result.push(range.clone());
    }
    
    result
}
