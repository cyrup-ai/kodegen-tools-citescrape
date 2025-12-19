//! Language inference for code blocks
//!
//! Weighted pattern scoring language detection inspired by GitHub Linguist
//! and highlight.js. Uses pattern categorization with confidence thresholds.
//!
//! Performance optimizations:
//! - Quick literal detection for definitive markers (O(1) on small prefix)
//! - First-line analysis for shebangs and declarations
//! - Representative sampling for large files (>50KB)
//! - Early termination when high-confidence match found

use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};

use super::language_patterns::ALL_LANGUAGES;

// ============================================================================
// Performance Tuning Constants
// ============================================================================

/// Size limit for quick detection prefix scan (bytes)
const QUICK_DETECT_PREFIX_SIZE: usize = 500;

/// Threshold above which we sample instead of full scan (bytes)
const SAMPLE_THRESHOLD: usize = 50_000; // 50KB

/// Sample size from start of file (bytes)
const HEAD_SAMPLE_SIZE: usize = 20_000; // 20KB

/// Sample size from end of file (bytes)
const TAIL_SAMPLE_SIZE: usize = 10_000; // 10KB

// ============================================================================
// Data Structures for Weighted Scoring
// ============================================================================

/// Confidence level for language detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    None,      // score < 5
    Low,       // 5 <= score < 12
    Medium,    // 12 <= score < 20
    High,      // score >= 20
}

/// Pattern weight categories - inspired by highlight.js relevance scoring
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum PatternCategory {
    Unique,      // 10 pts - Only this language has this pattern
    Strong,      // 8 pts  - Very indicative, rarely elsewhere
    Medium,      // 5 pts  - Common but shared across languages
    Weak,        // 2 pts  - Mildly suggestive
    Negative,    // -10 pts - Disqualifies this language
}

impl PatternCategory {
    pub const fn weight(&self) -> i8 {
        match self {
            PatternCategory::Unique => 10,
            PatternCategory::Strong => 8,
            PatternCategory::Medium => 5,
            PatternCategory::Weak => 2,
            PatternCategory::Negative => -10,
        }
    }
}

/// A single pattern with its category
pub struct WeightedPattern {
    pub pattern: &'static str,
    pub category: PatternCategory,
}

impl WeightedPattern {
    pub const fn new(pattern: &'static str, category: PatternCategory) -> Self {
        Self { pattern, category }
    }
}

/// Complete definition for one language
pub struct LanguageDefinition {
    pub name: &'static str,
    pub patterns: &'static [WeightedPattern],
}

/// Pre-compiled patterns for performance
/// Uses RegexSet for fast multi-pattern matching, individual Regex for counting
pub struct CompiledLanguage {
    pub name: &'static str,
    regex_set: RegexSet,
    individual: Vec<Regex>,
    weights: Vec<i8>,
}

impl CompiledLanguage {
    pub fn compile(lang: &LanguageDefinition) -> Self {
        let patterns: Vec<&str> = lang.patterns.iter()
            .map(|p| p.pattern)
            .collect();
        
        let weights: Vec<i8> = lang.patterns.iter()
            .map(|p| p.category.weight())
            .collect();
        
        Self {
            name: lang.name,
            regex_set: RegexSet::new(&patterns).expect("Invalid regex patterns in language definition"),
            individual: patterns.iter()
                .map(|p| Regex::new(p).expect("Invalid regex pattern"))
                .collect(),
            weights,
        }
    }
    
    /// Calculate weighted score for code against this language's patterns
    /// Uses diminishing returns: first match gets full weight, subsequent matches get half
    pub fn score(&self, code: &str) -> i32 {
        let matches: Vec<usize> = self.regex_set.matches(code).iter().collect();
        
        matches.iter()
            .map(|&i| {
                let count = self.individual[i].find_iter(code).count();
                // Diminishing returns: first match full weight, subsequent half
                // Cap at 5 matches to prevent single pattern dominating
                self.weights[i] as i32 * (1 + (count.min(5).saturating_sub(1)) / 2) as i32
            })
            .sum()
    }
}

// ============================================================================
// Pre-compiled Language Patterns (lazy initialization)
// ============================================================================

/// Pre-compiled patterns for all languages (compiled once at first use)
static COMPILED_LANGUAGES: Lazy<Vec<CompiledLanguage>> = Lazy::new(|| {
    ALL_LANGUAGES.iter()
        .map(|lang| CompiledLanguage::compile(lang))
        .collect()
});

// ============================================================================
// Public API
// ============================================================================

/// Extract language from CSS class patterns
///
/// Supports: "language-rust", "lang-rust", "hljs-rust", "brush:rust"
pub fn extract_language_from_class(class: &str) -> Option<String> {
    for part in class.split_whitespace() {
        // Pattern: "language-rust" or "lang-rust"
        if let Some(lang) = part.strip_prefix("language-") {
            return Some(lang.to_string());
        }
        if let Some(lang) = part.strip_prefix("lang-") {
            return Some(lang.to_string());
        }
        // Pattern: "hljs-rust" (highlight.js)
        if let Some(lang) = part.strip_prefix("hljs-") {
            return Some(lang.to_string());
        }
        // Pattern: "brush:rust" (SyntaxHighlighter)
        if let Some(lang) = part.strip_prefix("brush:") {
            return Some(lang.trim().to_string());
        }
    }
    None
}

// ============================================================================
// Staged Detection Functions (Performance Optimization)
// ============================================================================

/// Get a safe UTF-8 prefix of a string up to max_bytes.
/// Finds the largest valid char boundary at or before max_bytes.
fn safe_prefix(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// O(1) checks for definitive language markers in first 500 bytes.
/// Returns immediately on first match - no regex overhead.
fn quick_literal_detection(code: &str) -> Option<&'static str> {
    // Only check first 500 bytes for speed (safe UTF-8 boundary)
    let prefix = safe_prefix(code, QUICK_DETECT_PREFIX_SIZE);

    // Shebangs - definitive markers
    if prefix.starts_with("#!/bin/bash") || prefix.starts_with("#!/usr/bin/env bash") {
        return Some("bash");
    }
    if prefix.starts_with("#!/bin/sh") || prefix.starts_with("#!/usr/bin/env sh") {
        return Some("sh");
    }
    if prefix.starts_with("#!/usr/bin/env python") || prefix.starts_with("#!/usr/bin/python") {
        return Some("python");
    }
    if prefix.starts_with("#!/usr/bin/env node") {
        return Some("javascript");
    }
    if prefix.starts_with("#!/usr/bin/env ruby") || prefix.starts_with("#!/usr/bin/ruby") {
        return Some("ruby");
    }

    // Rust-unique patterns
    if prefix.contains("fn main()") && (prefix.contains("println!") || prefix.contains("let ")) {
        return Some("rust");
    }
    if prefix.contains("#[derive(") || (prefix.contains("impl ") && prefix.contains(" for ")) {
        return Some("rust");
    }

    // Go-unique patterns
    if prefix.contains("package main") && prefix.contains("func ") {
        return Some("go");
    }

    // HTML/XML markers
    if prefix.contains("<!DOCTYPE html") || prefix.starts_with("<html") {
        return Some("html");
    }
    if prefix.starts_with("<?xml") {
        return Some("xml");
    }

    None
}

/// Check first line for language indicators (shebangs, declarations)
fn first_line_detection(code: &str) -> Option<&'static str> {
    let first_line = match code.lines().next() {
        Some(line) => line.trim(),
        None => return None,
    };

    // TOML sections
    if first_line == "[package]" || first_line == "[dependencies]" || first_line == "[workspace]" {
        return Some("toml");
    }

    // YAML document marker - check second line for confirmation
    if first_line == "---"
        && let Some(second) = code.lines().nth(1)
        && second.contains(": ")
        && !second.trim_start().starts_with('{')
    {
        return Some("yaml");
    }

    // JSON detection - object or array at start
    if first_line.starts_with('{') || first_line.starts_with('[') {
        let trimmed = code.trim();
        // Quick JSON validation: starts with { or [ and ends with } or ]
        if (trimmed.ends_with('}') || trimmed.ends_with(']')) && trimmed.contains("\":") {
            return Some("json");
        }
    }

    // Dockerfile
    if first_line.starts_with("FROM ") && (code.contains("RUN ") || code.contains("COPY ")) {
        return Some("dockerfile");
    }

    None
}

/// Create a representative sample for large files.
/// Most language signatures are at the top (imports, declarations).
fn create_representative_sample(code: &str) -> String {
    let len = code.len();
    if len <= SAMPLE_THRESHOLD {
        return code.to_string();
    }

    // Find line boundaries to avoid cutting mid-line
    let head_end = code[..HEAD_SAMPLE_SIZE.min(len)]
        .rfind('\n')
        .unwrap_or(HEAD_SAMPLE_SIZE.min(len));

    let tail_start_approx = len.saturating_sub(TAIL_SAMPLE_SIZE);
    let tail_start = code[tail_start_approx..]
        .find('\n')
        .map(|i| tail_start_approx + i + 1)
        .unwrap_or(tail_start_approx);

    // Combine head and tail with separator
    format!("{}\n...\n{}", &code[..head_end], &code[tail_start..])
}

/// Run full regex-based scoring against all languages.
/// Called after quick checks fail.
fn score_all_languages(code: &str) -> Option<String> {
    // Calculate scores for all languages
    let mut scores: Vec<(&str, i32)> = COMPILED_LANGUAGES
        .iter()
        .map(|lang| (lang.name, lang.score(code)))
        .filter(|(_, score)| *score > 0)
        .collect();

    // Sort by score descending
    scores.sort_by(|a, b| b.1.cmp(&a.1));

    // No matches above 0
    if scores.is_empty() {
        return None;
    }

    let (winner_lang, winner_score) = scores[0];

    // Determine confidence level
    let confidence = match winner_score {
        s if s >= 20 => Confidence::High,
        s if s >= 12 => Confidence::Medium,
        s if s >= 5 => Confidence::Low,
        _ => Confidence::None,
    };

    // For Medium/Low confidence, require meaningful margin over second place
    if scores.len() > 1 && confidence != Confidence::High {
        let second_score = scores[1].1;
        let margin = winner_score - second_score;

        let margin_ratio = if winner_score > 0 {
            margin as f32 / winner_score as f32
        } else {
            0.0
        };

        if margin_ratio < 0.3 {
            match confidence {
                Confidence::Medium => {
                    if winner_score >= 10 {
                        return Some(winner_lang.to_string());
                    }
                    return None;
                }
                Confidence::Low => return None,
                _ => {}
            }
        }
    }

    if confidence == Confidence::None {
        None
    } else {
        Some(winner_lang.to_string())
    }
}

/// Main entry point for language inference from code content.
///
/// Uses staged detection for optimal performance:
/// 1. Quick literal checks (O(1) on first 500 bytes)
/// 2. First-line analysis (O(1))
/// 3. Sampling for large content (reduces O(n) to O(30KB))
/// 4. Full weighted scoring on sample
pub fn infer_language_from_content(code: &str) -> Option<String> {
    let trimmed = code.trim();

    // Skip empty or very short code
    if trimmed.len() < 5 {
        return None;
    }

    // Stage 1: Quick literal checks (O(1) on first 500 bytes)
    if let Some(lang) = quick_literal_detection(trimmed) {
        return Some(lang.to_string());
    }

    // Stage 2: First-line analysis
    if let Some(lang) = first_line_detection(trimmed) {
        return Some(lang.to_string());
    }

    // Stage 3: Create sample for large files
    let sample = if trimmed.len() > SAMPLE_THRESHOLD {
        create_representative_sample(trimmed)
    } else {
        trimmed.to_string()
    };

    // Stage 4: Full weighted scoring on sample
    score_all_languages(&sample)
}

/// Check if content contains shell-specific patterns
fn has_shell_patterns(content: &str) -> bool {
    use regex::Regex;
    use once_cell::sync::Lazy;
    
    // Regex patterns that match shell commands even without spaces
    static SHELL_COMMAND_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?m)^\s*(?:brew|apt-get|apt|yum|dnf|pacman|npm|yarn|pnpm|cargo|pip|rustup|curl|wget|git|docker|kubectl|terraform|ansible|ssh|scp|rsync)\b"
        ).expect("Invalid shell command regex")
    });
    
    static SHELL_SUBCOMMAND_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"\b(?:brew|npm|yarn|pnpm|cargo|git|docker|kubectl)(?:install|update|upgrade|add|remove|build|run|test|clone|pull|push|commit)\b"
        ).expect("Invalid shell subcommand regex")
    });
    
    static SHELL_COMMAND_WITH_FLAGS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\b(?:brew|curl|wget|git|npm|cargo)\s*(?:-{1,2}\w+|install|clone|build)").expect("Invalid command with flags regex")
    });
    
    // Shell export statements: export VAR_NAME=value (NOT JS/TS export default/const/class)
    // Matches uppercase variable names typical in shell scripts
    static SHELL_EXPORT_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?m)^\s*export\s+[A-Z_][A-Z_0-9]*=").expect("Invalid shell export regex")
    });

    // Shell pipe detection - pipe followed by common shell utilities
    static SHELL_PIPE_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"\|\s*(grep|awk|sed|cut|sort|uniq|head|tail|wc|less|more|tee|xargs|tr|cat|bash|sh|zsh|perl|ruby|python)\b"
        ).expect("Invalid shell pipe regex")
    });
    
    // Markdown table detection - lines that start and end with |
    static MARKDOWN_TABLE_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?m)^\s*\|.*\|\s*$").expect("Invalid markdown table regex")
    });
    
    // Table separator detection
    static TABLE_SEPARATOR_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\|[\s-]*\|").expect("Invalid table separator regex")
    });

    // Check for shell commands at line start (works with or without spaces)
    if SHELL_COMMAND_REGEX.is_match(content) {
        return true;
    }
    
    // Check for common command+subcommand patterns (handles missing spaces)
    if SHELL_SUBCOMMAND_REGEX.is_match(content) {
        return true;
    }
    
    // Check for commands with flags
    if SHELL_COMMAND_WITH_FLAGS.is_match(content) {
        return true;
    }
    
    // Check for shell export statements (export VAR=value)
    if SHELL_EXPORT_REGEX.is_match(content) {
        return true;
    }

    // Check for shell-specific syntax patterns
    if content.contains("#!/bin/bash")
        || content.contains("#!/bin/sh")
        || content.contains("#!/usr/bin/env bash")
        || content.contains("#!/usr/bin/env sh")
        || content.contains("$HOME")
        || content.contains("$PATH")
        || content.contains("${")
        || content.contains("2>&1")
        || content.contains(">/dev/null")
        || content.contains(">>")       // Append redirection
    {
        return true;
    }
    
    // Smart pipe detection - only match shell-style pipes
    // First exclude markdown tables
    if content.contains('|') {
        // If it looks like a markdown table, don't treat as shell
        if MARKDOWN_TABLE_REGEX.is_match(content) || TABLE_SEPARATOR_REGEX.is_match(content) {
            // Don't flag as shell based on pipe alone
        } else if SHELL_PIPE_REGEX.is_match(content) {
            // Pipe followed by shell utility = definitely shell
            return true;
        }
        // Otherwise, pipe alone is not enough evidence
    }
    
    // Smart && detection - only match shell-style usage
    // Shell && typically appears after commands or at line boundaries
    // JS/TS && appears in expressions like (a && b) or a && b ? x : y
    if content.contains("&&") {
        // Check if it looks like shell command chaining (command && command)
        // vs JS/TS logical AND (within expressions)
        static SHELL_AND_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?m)^\s*\w+.*&&\s*\w+").expect("Invalid shell AND regex")
        });
        
        // Only flag as shell if && looks like command chaining
        // Exclude if it looks like JS/TS expression
        if !content.contains("if ") 
            && !content.contains("if(")
            && !content.contains("while ")
            && !content.contains("while(")
            && !content.contains(" ? ")
            && !content.contains("return ")
            && SHELL_AND_REGEX.is_match(content) 
        {
            return true;
        }
    }

    false
}

/// Check if content contains PowerShell-specific patterns
fn has_powershell_patterns(content: &str) -> bool {
    use regex::Regex;
    use once_cell::sync::Lazy;
    
    // PowerShell cmdlet verb-noun pattern (most distinctive PowerShell feature)
    static POWERSHELL_CMDLET_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"\b(Get|Set|New|Remove|Add|Clear|Copy|Move|Rename|Write|Read|Test|Invoke|Start|Stop|Restart|Enable|Disable|Import|Export|ConvertTo|ConvertFrom|Select|Where|ForEach|Sort|Group|Measure)-[A-Z]\w+"
        ).expect("Invalid PowerShell cmdlet regex")
    });
    
    // PowerShell aliases
    static POWERSHELL_ALIAS_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\b(irm|iex|iwr)\b").expect("Invalid PowerShell alias regex")
    });
    
    // .NET type invocation syntax [Type]::Method
    static DOTNET_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\[[A-Z]\w+(\.[A-Z]\w+)*\]::").expect("Invalid .NET type regex")
    });
    
    // PowerShell comparison operators
    static POWERSHELL_OPERATOR_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"-(eq|ne|gt|ge|lt|le|like|notlike|match|notmatch|contains|notcontains|in|notin|and|or|not|xor)\b")
            .expect("Invalid PowerShell operator regex")
    });
    
    // PowerShell automatic variables
    static POWERSHELL_VAR_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\$(PSScriptRoot|PSVersionTable|PSCommandPath|PSBoundParameters|ErrorActionPreference|ProgressPreference|Host|MyInvocation)\b")
            .expect("Invalid PowerShell variable regex")
    });
    
    // Check for PowerShell cmdlets (strongest signal)
    if POWERSHELL_CMDLET_REGEX.is_match(content) {
        return true;
    }
    
    // Check for PowerShell aliases
    if POWERSHELL_ALIAS_REGEX.is_match(content) {
        return true;
    }
    
    // Check for .NET type syntax
    if DOTNET_TYPE_REGEX.is_match(content) {
        return true;
    }
    
    // Check for PowerShell operators
    if POWERSHELL_OPERATOR_REGEX.is_match(content) {
        return true;
    }
    
    // Check for PowerShell automatic variables
    if POWERSHELL_VAR_REGEX.is_match(content) {
        return true;
    }
    
    false
}

/// Validate that HTML-provided language hint matches content
///
/// Returns true if the hint seems correct, false if suspicious.
/// Used to override incorrect HTML hints like "typescript" on plain text.
pub fn validate_html_language(html_lang: &str, code: &str) -> bool {
    let lower = code.to_lowercase();
    let html_lang_lower = html_lang.to_lowercase();

    // Plain text indicators - if present, programming language hints are wrong
    if code.contains("panicked at")
        || code.contains("stack backtrace:")
        || lower.contains("backtrace")
        || code.contains("thread '") && code.contains("' panicked")
    {
        // Content is plain text, reject programming language hints
        if matches!(
            html_lang_lower.as_str(),
            "rust" | "python" | "javascript" | "typescript" | "java" | "go" | "cpp"
        ) {
            return false; // HTML hint is wrong
        }
    }

    // Shell patterns - if present, reject non-shell language hints
    if has_shell_patterns(code)
        && matches!(
            html_lang_lower.as_str(),
            "sql" | "mysql" | "postgres" | "postgresql"
                | "ruby"
                | "php"
                | "cpp" | "c++"
                | "toml"
                | "css"
                | "go"
        )
    {
        return false; // HTML says sql/ruby/php/cpp/toml/css/go, but content is shell
    }

    // PowerShell indicators - if present, reject non-PowerShell language hints
    if has_powershell_patterns(code)
        && matches!(
            html_lang_lower.as_str(),
            "toml" | "bash" | "sh" | "shell" 
                | "python" | "ruby" | "perl"
                | "sql" | "mysql" | "postgres" | "postgresql"
                | "yaml" | "json"
                | "javascript" | "typescript"
        )
    {
        return false; // HTML says toml/bash/etc, but content is PowerShell
    }

    // TOML indicators - if present, bash/shell hints are wrong
    if (code.contains(" = \"") || code.contains(" = '"))
        && (code.contains("[package]") || code.contains("[dependencies]"))
        && matches!(html_lang_lower.as_str(), "bash" | "sh" | "shell")
    {
        return false; // HTML says shell, but content is TOML
    }

    // Config format indicators - reject programming language hints
    if ((code.contains(": ") && code.contains("  ")) // YAML indentation
        || (code.trim().starts_with("{") && code.contains("\":"))) // JSON
        && matches!(
            html_lang_lower.as_str(),
            "rust" | "python" | "javascript" | "typescript"
        )
    {
        return false; // HTML says programming language, but content is config
    }

    // Rust indicators - if present, YAML hints are wrong
    if html_lang_lower == "yaml" {
        // Strong Rust patterns that YAML never has
        if code.contains("#[derive")
            || code.contains("pub struct ")
            || code.contains("impl ")
            || code.contains("pub fn ")
            || code.contains("-> ") // Function return type
            || code.contains("::") // Path separator
            // Rust primitive types in field declarations
            || code.contains(": u8")
            || code.contains(": u16")
            || code.contains(": u32")
            || code.contains(": u64")
            || code.contains(": i8")
            || code.contains(": i16")
            || code.contains(": i32")
            || code.contains(": i64")
            || code.contains(": bool")
            || code.contains(": f32")
            || code.contains(": f64")
            || code.contains(": usize")
            || code.contains(": isize")
            // Rust type annotations in structs
            || (code.contains("struct ") && code.contains(": "))
        {
            return false; // HTML says yaml, but content is Rust
        }
    }

    true // HTML hint seems reasonable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_language_from_class() {
        assert_eq!(
            extract_language_from_class("language-rust"),
            Some("rust".to_string())
        );
        assert_eq!(
            extract_language_from_class("lang-python"),
            Some("python".to_string())
        );
        assert_eq!(
            extract_language_from_class("hljs-javascript"),
            Some("javascript".to_string())
        );
        assert_eq!(
            extract_language_from_class("brush:java"),
            Some("java".to_string())
        );
        assert_eq!(extract_language_from_class("no-match"), None);
    }

    #[test]
    fn test_infer_rust() {
        let code = r#"fn main() {
    println!("Hello");
}
impl MyStruct {
    pub fn new() -> Self { Self {} }
}"#;
        assert_eq!(infer_language_from_content(code), Some("rust".to_string()));
    }

    #[test]
    fn test_infer_rust_simple_fn() {
        // This was the original bug - fn main() {} alone should detect as Rust
        let code = "fn main() {}";
        assert_eq!(infer_language_from_content(code), Some("rust".to_string()));
    }

    #[test]
    fn test_infer_rust_println() {
        // Single println needs more context for confident detection
        let code = r#"fn main() {
    println!("Hello, world!");
}"#;
        assert_eq!(infer_language_from_content(code), Some("rust".to_string()));
    }

    #[test]
    fn test_infer_python() {
        let code = r#"def hello_world():
    print("Hello")
    return None

class MyClass:
    def __init__(self):
        self.value = 42"#;
        assert_eq!(infer_language_from_content(code), Some("python".to_string()));
    }

    #[test]
    fn test_infer_javascript() {
        let code = r#"function greet(name) {
    console.log(`Hello, ${name}!`);
}
const arrow = () => { return 42; };"#;
        assert_eq!(
            infer_language_from_content(code),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn test_infer_typescript() {
        let code = r#"interface User {
    name: string;
    age: number;
}
function greet(user: User): void {
    console.log(user.name);
}"#;
        assert_eq!(
            infer_language_from_content(code),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_infer_bash_command() {
        assert_eq!(
            infer_language_from_content("#!/bin/bash\necho 'Hello'"),
            Some("bash".to_string())
        );
    }

    #[test]
    fn test_infer_toml() {
        let code = r#"[package]
name = "my-crate"
version = "0.1.0"
edition = "2021""#;
        assert_eq!(infer_language_from_content(code), Some("toml".to_string()));
    }

    #[test]
    fn test_infer_json() {
        let code = r#"{
    "name": "test",
    "version": "1.0"
}"#;
        assert_eq!(infer_language_from_content(code), Some("json".to_string()));
    }

    #[test]
    fn test_infer_yaml() {
        // Use more Kubernetes-style YAML for clearer detection
        let code = r#"---
apiVersion: v1
kind: ConfigMap
metadata:
  name: my-config
spec:
  replicas: 3"#;
        assert_eq!(infer_language_from_content(code), Some("yaml".to_string()));
    }

    #[test]
    fn test_validate_rejects_rust_for_backtrace() {
        let code = "thread 'main' panicked at 'error'\nstack backtrace:";
        assert!(!validate_html_language("rust", code));
    }

    #[test]
    fn test_validate_accepts_correct_hint() {
        let code = "fn main() { println!(\"Hello\"); }";
        assert!(validate_html_language("rust", code));
    }

    #[test]
    fn test_short_code_no_inference() {
        // Very short ambiguous code shouldn't infer
        assert_eq!(infer_language_from_content("abc"), None);
    }

    #[test]
    fn test_infer_rust_struct_not_yaml() {
        let code = r#"#[derive(Debug, Default)]
pub struct App {
    counter: u8,
    exit: bool,
}"#;
        assert_eq!(infer_language_from_content(code), Some("rust".to_string()));
    }

    #[test]
    fn test_validate_rejects_yaml_for_rust_struct() {
        let code = r#"#[derive(Debug)]
pub struct App {
    counter: u8,
}"#;
        assert!(!validate_html_language("yaml", code));
    }

    #[test]
    fn test_validate_accepts_yaml_for_actual_yaml() {
        let code = r#"name: my-app
version: 1.0.0
dependencies:
  - rust
  - tokio"#;
        assert!(validate_html_language("yaml", code));
    }

    #[test]
    fn test_infer_go() {
        let code = r#"package main

import "fmt"

func main() {
    fmt.Println("Hello")
}"#;
        assert_eq!(infer_language_from_content(code), Some("go".to_string()));
    }

    #[test]
    fn test_infer_java() {
        let code = r#"public class Main {
    public static void main(String[] args) {
        System.out.println("Hello");
    }
}"#;
        assert_eq!(infer_language_from_content(code), Some("java".to_string()));
    }

    #[test]
    fn test_infer_cpp() {
        let code = r#"#include <iostream>

int main() {
    std::cout << "Hello" << std::endl;
    return 0;
}"#;
        assert_eq!(infer_language_from_content(code), Some("cpp".to_string()));
    }

    #[test]
    fn test_infer_sql() {
        let code = "SELECT * FROM users WHERE id = 1;";
        assert_eq!(infer_language_from_content(code), Some("sql".to_string()));
    }

    #[test]
    fn test_infer_html() {
        let code = r#"<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body><div>Hello</div></body>
</html>"#;
        assert_eq!(infer_language_from_content(code), Some("html".to_string()));
    }

    #[test]
    fn test_infer_css() {
        let code = r#".container {
    display: flex;
    padding: 20px;
    background-color: #fff;
}"#;
        assert_eq!(infer_language_from_content(code), Some("css".to_string()));
    }

    #[test]
    fn test_all_patterns_compile() {
        // Force lazy initialization - will panic if any regex is invalid
        let _ = COMPILED_LANGUAGES.len();
        assert!(!COMPILED_LANGUAGES.is_empty());
    }

    #[test]
    fn test_validate_rejects_css_for_shell_export() {
        // Issue #029: CSS wrongly tagged for bash export statements
        let code = r#"# Enable Microsoft Foundry integration
export CLAUDE_CODE_USE_FOUNDRY=1

# Azure resource name
export ANTHROPIC_FOUNDRY_RESOURCE={resource}"#;
        assert!(!validate_html_language("css", code));
    }

    #[test]
    fn test_validate_accepts_css_for_actual_css() {
        let code = r#".container {
    display: flex;
    padding: 20px;
    background-color: #fff;
}"#;
        assert!(validate_html_language("css", code));
    }

    #[test]
    fn test_validate_rejects_go_for_shell_export() {
        // Issue #039: Go wrongly tagged for bash export statements
        let code = r#"# HTTPS proxy (recommended)
export HTTPS_PROXY=https://proxy.example.com:8080

# Azure resource name
export ANTHROPIC_FOUNDRY_RESOURCE={resource}"#;
        assert!(!validate_html_language("go", code));
    }

    #[test]
    fn test_validate_accepts_go_for_actual_go() {
        let code = r#"package main

import "fmt"

func main() {
    fmt.Println("Hello, World!")
}"#;
        assert!(validate_html_language("go", code));
    }

    #[test]
    fn test_infer_bash_export() {
        // Issue #039: Bash export commands should detect as bash
        let code = r#"# HTTPS proxy (recommended)
export HTTPS_PROXY=https://proxy.example.com:8080

# Azure resource name
export ANTHROPIC_FOUNDRY_RESOURCE={resource}"#;
        assert_eq!(infer_language_from_content(code), Some("bash".to_string()));
    }
}
