//! Language inference for code blocks
//!
//! Heuristic-based language detection for code content when HTML attributes
//! don't provide language information.

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

/// Infer programming language from code content using heuristic patterns
///
/// This is a fallback when HTML attributes don't provide language information.
/// Uses pattern matching on common language keywords and syntax.
///
/// Supported languages (in priority order):
/// - Plain text (panic, backtrace, logs)
/// - TOML, YAML, JSON (config files)
/// - Shell commands (cargo, npm, git, etc.)
/// - Rust, Python, TypeScript, JavaScript, Go, Java, C/C++, Ruby, PHP, Shell, SQL
pub fn infer_language_from_content(code: &str) -> Option<String> {
    let code = code.trim();

    // Need reasonable code sample for reliable detection
    if code.len() < 10 {
        return None;
    }

    let lower = code.to_lowercase();

    // PRIORITY 1: Plain text outputs (panic, backtrace, logs)
    // Check FIRST to prevent misclassification as programming languages
    if code.contains("panicked at")
        || code.contains("thread '") && code.contains("' panicked")
        || code.contains("stack backtrace:")
        || lower.contains("backtrace")
        || code.starts_with("Well, this is embarrassing")
        || code.contains("note: run with `RUST_BACKTRACE=")
        || code.contains("Error: ")
        || code.contains("error:")
        || code.contains("warning:")
        || lower.contains("exception")
        || lower.contains("traceback")
    {
        return Some("text".to_string());
    }

    // PRIORITY 2: TOML configuration files
    // Check before Rust (Rust code can contain = signs)
    if (code.contains(" = \"") || code.contains(" = '"))
        && (code.contains("[package]")
            || code.contains("[dependencies]")
            || code.contains("[dev-dependencies]")
            || code.contains("[build-dependencies]")
            || code.contains("[profile.")
            || code.contains("[target.")
            || code.contains("version = ")
            || code.contains("edition = ")
            || (code.lines().filter(|l| l.contains(" = ")).count() >= 3))
    {
        return Some("toml".to_string());
    }

    // PRIORITY 2.5: Rust struct/enum definitions (before YAML to prevent confusion)
    // These contain : and indentation but are NOT YAML
    if (code.contains("pub struct ") || code.contains("struct "))
        && code.contains(": ")
        && (code.contains("#[derive") || code.contains("impl ") || code.contains("pub fn "))
    {
        return Some("rust".to_string());
    }

    // PRIORITY 3: YAML configuration files
    // Must exclude Rust structs which have similar patterns
    if (code.contains(": ") || code.contains(":\n"))
        && !code.contains("fn ") // Avoid Rust functions
        && !code.contains("class ") // Avoid Python classes
        // NEW: Exclude Rust patterns
        && !code.contains("#[") // Rust attributes
        && !code.contains("pub struct ") // Rust structs
        && !code.contains("impl ") // Rust implementations
        && !code.contains("-> ") // Rust return types
        && !code.contains("::") // Rust path separator
        // Positive YAML indicators
        && (code.starts_with("---")
            || code.contains("  ") // YAML relies on indentation
            || code.lines().filter(|l| l.trim_start().starts_with("- ")).count() >= 2)
    {
        return Some("yaml".to_string());
    }

    // PRIORITY 4: JSON
    let trimmed = code.trim();
    if ((trimmed.starts_with("{") && trimmed.ends_with("}"))
        || (trimmed.starts_with("[") && trimmed.ends_with("]")))
        && (code.contains("\":") || code.contains("\": "))
    {
        return Some("json".to_string());
    }

    // PRIORITY 5: Shell commands (single-line commands)
    // Common package managers and build tools
    let first_line = code.lines().next().unwrap_or("");
    let first_word = first_line.split_whitespace().next().unwrap_or("");

    match first_word {
        "cargo" | "rustc" | "rustup" | "rustdoc" => return Some("bash".to_string()),
        "npm" | "npx" | "yarn" | "pnpm" => return Some("bash".to_string()),
        "git" => return Some("bash".to_string()),
        "docker" | "docker-compose" => return Some("bash".to_string()),
        "kubectl" | "helm" => return Some("bash".to_string()),
        "python" | "python3" | "pip" | "pip3" => return Some("bash".to_string()),
        "node" | "deno" | "bun" => return Some("bash".to_string()),
        "go" | "make" | "cmake" => return Some("bash".to_string()),
        "cd" | "ls" | "pwd" | "mkdir" | "rm" | "cp" | "mv" => return Some("bash".to_string()),
        "curl" | "wget" | "ssh" => return Some("bash".to_string()),
        _ => {}
    }

    // Also detect command chains and redirects
    if code.lines().count() <= 3 // Short snippets
        && (code.contains(" && ") || code.contains(" || ")
            || code.contains(" | ") || code.contains(" > ")
            || code.contains("sudo ") || code.contains("export "))
    {
        return Some("bash".to_string());
    }

    // Rust detection (high confidence patterns)
    if code.contains("fn ") && code.contains("->")
        || code.contains("impl ")
        || code.contains("pub fn ")
        || code.contains("let mut ")
        || code.contains("::") && code.contains("<")
        || code.contains("Result<")
        || code.contains("Option<")
    {
        return Some("rust".to_string());
    }

    // Python detection
    if code.contains("def ") && code.contains(":")
        || code.contains("import ")
        || code.contains("from ") && code.contains(" import ")
        || code.contains("class ") && code.contains("__init__")
        || code.contains("self.")
        || code.contains("elif ")
    {
        return Some("python".to_string());
    }

    // TypeScript detection (before JavaScript)
    if code.contains(": string") || code.contains(": number")
        || code.contains("interface ")
        || code.contains(": boolean")
        || code.contains("type ") && code.contains("=")
        || code.contains("as ")
    {
        return Some("typescript".to_string());
    }

    // JavaScript detection
    if code.contains("function ") || code.contains("=>")
        || code.contains("const ") || code.contains("let ")
        || code.contains("console.log")
        || code.contains("require(")
        || code.contains("export ") || code.contains("import ")
    {
        return Some("javascript".to_string());
    }

    // Go detection
    if code.contains("func ") && (code.contains("package ") || code.contains("import ("))
        || code.contains("defer ")
        || code.contains("go func")
        || code.contains("make(")
    {
        return Some("go".to_string());
    }

    // Java detection
    if code.contains("public class ") || code.contains("private ")
        || code.contains("void ") && code.contains("{")
        || code.contains("@Override")
        || code.contains("System.out.")
    {
        return Some("java".to_string());
    }

    // C/C++ detection
    if code.contains("#include")
        || code.contains("int main(")
        || code.contains("std::")
        || code.contains("nullptr")
        || code.contains("printf(")
    {
        return Some("cpp".to_string());
    }

    // Ruby detection
    if code.contains("end") && (code.contains("def ") || code.contains("class "))
        || code.contains("puts ")
        || code.contains("require '")
        || code.contains("do |")
    {
        return Some("ruby".to_string());
    }

    // PHP detection
    if code.starts_with("<?php") || code.contains("<?php")
        || code.contains("$this->")
        || code.contains("function ") && code.contains("$")
    {
        return Some("php".to_string());
    }

    // Shell/Bash detection
    if code.starts_with("#!")
        || code.contains("#!/bin/bash")
        || code.contains("#!/bin/sh")
        || lower.contains("echo ")
        || code.contains("if [ ")
        || code.contains("fi")
    {
        return Some("bash".to_string());
    }

    // SQL detection
    if lower.contains("select ") && lower.contains(" from ")
        || lower.contains("insert into ")
        || lower.contains("create table ")
        || lower.contains("update ") && lower.contains(" set ")
    {
        return Some("sql".to_string());
    }

    // No confident detection
    None
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
    fn test_infer_bash_command() {
        assert_eq!(
            infer_language_from_content("cargo build --release"),
            Some("bash".to_string())
        );
        assert_eq!(
            infer_language_from_content("npm install express"),
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
        let code = r#"{"name": "test", "version": "1.0"}"#;
        assert_eq!(infer_language_from_content(code), Some("json".to_string()));
    }

    #[test]
    fn test_infer_text_for_backtrace() {
        let code = "thread 'main' panicked at 'error', src/main.rs:10:5\nstack backtrace:";
        assert_eq!(infer_language_from_content(code), Some("text".to_string()));
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
        assert_eq!(infer_language_from_content("x = 1"), None);
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
}
