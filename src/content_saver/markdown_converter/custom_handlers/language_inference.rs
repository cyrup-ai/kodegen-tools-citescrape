//! Language inference for code blocks
//!
//! Weighted pattern scoring language detection inspired by GitHub Linguist
//! and highlight.js. Uses pattern categorization with confidence thresholds.

use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};

use super::language_patterns::ALL_LANGUAGES;

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

/// Main entry point for language inference from code content
/// 
/// Uses weighted pattern scoring to detect programming language.
/// Returns None if confidence is too low or no patterns match.
pub fn infer_language_from_content(code: &str) -> Option<String> {
    // Skip empty or very short code (need enough context)
    let trimmed = code.trim();
    if trimmed.len() < 5 {
        return None;
    }
    
    // Calculate scores for all languages
    let mut scores: Vec<(&str, i32)> = COMPILED_LANGUAGES.iter()
        .map(|lang| (lang.name, lang.score(trimmed)))
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
        
        // Need at least 30% margin for confident detection
        let margin_ratio = if winner_score > 0 {
            margin as f32 / winner_score as f32
        } else {
            0.0
        };
        
        if margin_ratio < 0.3 {
            match confidence {
                Confidence::Medium => {
                    // Still return if score is reasonable
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
    // Detect operators WITHOUT spaces since HTML collapse is the bug we're fixing
    if content.contains("#!/bin/bash")
        || content.contains("#!/bin/sh")
        || content.contains("#!/usr/bin/env bash")
        || content.contains("#!/usr/bin/env sh")
        || content.contains('|')        // DETECT PIPE WITHOUT SPACES
        || content.contains("&&")       // DETECT AND WITHOUT SPACES
        || content.contains(">>")       // DETECT APPEND WITHOUT SPACES
        || content.contains("$HOME")
        || content.contains("$PATH")
        || content.contains("${")
        || content.contains("2>&1")
    {
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
