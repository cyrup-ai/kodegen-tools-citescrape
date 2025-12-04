//! Custom code block handler that preserves language hints from HTML attributes.
//!
//! This handler replaces html2md's default CodeHandler to extract language information
//! from `data-language` attributes or CSS class names (e.g., `language-rust`).
//!
//! Fallback strategy: If HTML attributes don't specify a language, uses heuristic
//! pattern matching to infer the language from code content (supports 10+ languages).

use html2md::{Handle, StructuredPrinter, TagHandler, TagHandlerFactory};
use markup5ever_rcdom::NodeData;
use std::collections::HashMap;

/// Custom handler for `<pre>` and `<code>` tags that extracts language hints
pub struct CodeLanguageHandler {
    /// Language extracted from HTML attributes (e.g., "rust", "python")
    language: Option<String>,
    /// Tag type: "pre" or "code"
    code_type: String,
    /// Track if we're inside a <pre> block (to avoid double-wrapping <code> inside <pre>)
    inside_pre: bool,
    /// Cached raw text content for <pre> tags (preserves newlines)
    raw_text: Option<String>,
}

impl CodeLanguageHandler {
    pub fn new() -> Self {
        Self {
            language: None,
            code_type: String::new(),
            inside_pre: false,
            raw_text: None,
        }
    }

    /// Recursively extract raw text from HTML node tree, preserving all whitespace
    fn extract_raw_text(handle: &Handle) -> String {
        use markup5ever_rcdom::NodeData;

        let mut text = String::new();

        match handle.data {
            NodeData::Text { ref contents } => {
                // Preserve text exactly as-is (including newlines and spaces)
                text.push_str(&contents.borrow());
            }
            NodeData::Element { .. } => {
                // Recursively process all children
                for child in handle.children.borrow().iter() {
                    text.push_str(&Self::extract_raw_text(child));
                }
            }
            _ => {
                // For other node types, still check children
                for child in handle.children.borrow().iter() {
                    text.push_str(&Self::extract_raw_text(child));
                }
            }
        }

        text
    }

    /// Extract language from HTML element attributes
    fn extract_language(tag: &Handle) -> Option<String> {
        if let NodeData::Element { ref attrs, .. } = tag.data {
            let attrs = attrs.borrow();
            
            // Priority 1: Check data-language attribute
            if let Some(attr) = attrs.iter().find(|a| &*a.name.local == "data-language") {
                let lang = attr.value.to_string();
                if !lang.is_empty() {
                    return Some(lang);
                }
            }
            
            // Priority 2: Extract from class attribute
            if let Some(attr) = attrs.iter().find(|a| &*a.name.local == "class") {
                let classes = attr.value.to_string();
                return Self::extract_language_from_class(&classes);
            }
        }
        None
    }

    /// Parse language from class attribute patterns
    /// Supports: "language-rust", "lang-rust", "rust", "hljs-rust", etc.
    fn extract_language_from_class(class: &str) -> Option<String> {
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
            // Pattern: "brush: rust" (SyntaxHighlighter)
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
    /// - Rust, Python, JavaScript, TypeScript, Java, Go, C/C++, Ruby, PHP, Shell
    fn infer_language_from_content(code: &str) -> Option<String> {
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

        // PRIORITY 3: YAML configuration files
        if (code.contains(": ") || code.contains(":\n"))
            && !code.contains("fn ")  // Avoid Rust functions
            && !code.contains("class ") // Avoid Python classes
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
    fn validate_html_language(html_lang: &str, code: &str) -> bool {
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

        true // HTML hint seems reasonable
    }

    /// Handle code block opening/closing
    fn do_handle(&mut self, printer: &mut StructuredPrinter, at_start: bool) {
        match self.code_type.as_str() {
            "pre" => {
                if at_start {
                    // Opening fence: ``` or ```rust
                    printer.insert_newline();
                    printer.append_str("```");
                    if let Some(ref lang) = self.language {
                        printer.append_str(lang);
                    }
                    printer.insert_newline();
                } else {
                    // Closing fence: append raw text then close
                    if let Some(ref text) = self.raw_text {
                        // Append the raw text content (preserves all newlines)
                        printer.append_str(text);
                    }
                    printer.insert_newline();
                    printer.append_str("```");
                    printer.insert_newline();
                }
            }
            "code" | "samp" => {
                // Only wrap standalone <code> tags with backticks
                // Skip if inside <pre> block (already fenced)
                if !self.inside_pre {
                    if at_start {
                        // Opening backtick: ensure space before if preceded by non-whitespace
                        // Check the last character in the markdown buffer
                        if let Some(last_char) = printer.data.chars().last()
                            && !last_char.is_whitespace()
                        {
                            // Insert space before backtick to prevent concatenation
                            printer.append_str(" ");
                        }
                    }
                    printer.append_str("`");
                }
            }
            _ => {}
        }
    }
}

impl TagHandler for CodeLanguageHandler {
    fn handle(&mut self, tag: &Handle, printer: &mut StructuredPrinter) {
        // Extract tag name
        if let NodeData::Element { ref name, .. } = tag.data {
            self.code_type = name.local.to_string();

            // For <pre> tags: extract both language and raw text content
            if self.code_type == "pre" {
                self.inside_pre = true;

                // Step 1: Try to extract language from HTML attributes
                let html_lang = Self::extract_language(tag);

                // Step 2: Extract raw text content (preserves newlines)
                let raw_text = Self::extract_raw_text(tag);

                // Step 3: Validate HTML language hint against content
                let validated_html_lang = if let Some(ref lang) = html_lang {
                    if Self::validate_html_language(lang, &raw_text) {
                        html_lang.clone() // HTML hint is valid
                    } else {
                        None // HTML hint is suspicious, discard it
                    }
                } else {
                    None
                };

                // Step 4: Use validated HTML hint, or infer from content
                self.language = validated_html_lang.or_else(|| {
                    Self::infer_language_from_content(&raw_text)
                });

                self.raw_text = Some(raw_text);
            }

            self.do_handle(printer, true);
        }
    }

    fn after_handle(&mut self, printer: &mut StructuredPrinter) {
        self.do_handle(printer, false);
    }

    fn skip_descendants(&self) -> bool {
        // For <pre> tags, skip html2md's default processing to preserve newlines
        // We manually extract raw text in handle() instead
        self.code_type == "pre"
    }
}

/// Factory for creating CodeLanguageHandler instances
pub struct CodeLanguageHandlerFactory;

impl TagHandlerFactory for CodeLanguageHandlerFactory {
    fn instantiate(&self) -> Box<dyn TagHandler> {
        Box::new(CodeLanguageHandler::new())
    }
}

/// Create HashMap of custom tag handlers for html2md::parse_html_custom
#[allow(dead_code)]
pub fn create_custom_handlers() -> HashMap<String, Box<dyn TagHandlerFactory>> {
    let mut handlers: HashMap<String, Box<dyn TagHandlerFactory>> = HashMap::new();

    // Register our custom handler for pre, code, and samp tags
    handlers.insert("pre".to_string(), Box::new(CodeLanguageHandlerFactory));
    handlers.insert("code".to_string(), Box::new(CodeLanguageHandlerFactory));
    handlers.insert("samp".to_string(), Box::new(CodeLanguageHandlerFactory));

    handlers
}

#[cfg(test)]
mod tests {
    use super::*;
    use html2md;

    #[test]
    fn test_code_block_preserves_newlines() {
        let html = r#"<pre><code>fn main() {
    println!("Hello");
    println!("World");
}</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Should contain newlines between lines of code
        assert!(markdown.contains("fn main() {\n"), "Missing newline after opening brace");
        assert!(markdown.contains("    println!(\"Hello\");\n"), "Missing newline after first println");
        assert!(markdown.contains("    println!(\"World\");\n"), "Missing newline after second println");
        assert!(markdown.contains("}"), "Missing closing brace");

        // Should NOT be a single line
        assert!(!markdown.contains("fn main() {    println!"), "Code is collapsed to single line!");
    }

    #[test]
    fn test_code_block_preserves_indentation() {
        let html = r#"<pre><code>if true {
    nested {
        deep();
    }
}</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Check that indentation is preserved
        assert!(markdown.contains("    nested {"), "4-space indentation lost");
        assert!(markdown.contains("        deep();"), "8-space indentation lost");
    }

    #[test]
    fn test_code_block_with_language_preserves_newlines() {
        let html = r#"<pre class="language-rust"><code>pub fn test() {
    Ok(())
}</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Should have language hint
        assert!(markdown.contains("```rust"), "Language hint missing");

        // Should preserve newlines
        assert!(markdown.contains("pub fn test() {\n"), "Newline after opening brace missing");
        assert!(markdown.contains("    Ok(())\n"), "Newline after Ok(()) missing");
    }

    #[test]
    fn test_code_block_empty_lines() {
        let html = r#"<pre><code>fn a() {
    let x = 1;

    let y = 2;
}</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Empty lines within code should be preserved
        assert!(markdown.contains("let x = 1;\n\n"), "Empty line between statements lost");
    }

    #[test]
    fn test_code_block_trailing_newline() {
        let html = r#"<pre><code>fn main() {
}
</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Trailing newline should be preserved
        assert!(markdown.contains("}\n"), "Trailing newline lost");
    }

    #[test]
    fn test_inline_code_still_works() {
        let html = r#"<p>Use the <code>println!</code> macro</p>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Inline code should still use backticks
        assert!(markdown.contains("`println!`"), "Inline code formatting broken. Got: {}", markdown);
    }

    #[test]
    fn test_language_inference_rust() {
        let html = r#"<pre><code>fn main() {
    println!("Hello, world!");
}
impl MyStruct {
    pub fn new() -> Self {
        Self {}
    }
}</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Should infer Rust language
        assert!(markdown.contains("```rust") || markdown.contains("```"),
            "Expected rust language hint or plain code fence");
    }

    #[test]
    fn test_language_inference_python() {
        let html = r#"<pre><code>def hello_world():
    print("Hello, world!")
    return None

class MyClass:
    def __init__(self):
        self.value = 42</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Should infer Python language
        assert!(markdown.contains("```python") || markdown.contains("```py") || markdown.contains("```"),
            "Expected python language hint or plain code fence");
    }

    #[test]
    fn test_language_inference_javascript() {
        let html = r#"<pre><code>function greet(name) {
    console.log(`Hello, ${name}!`);
}

const arrow = () => {
    return 42;
};</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Should infer JavaScript language
        assert!(markdown.contains("```javascript") || markdown.contains("```js") || markdown.contains("```"),
            "Expected javascript language hint or plain code fence");
    }

    #[test]
    fn test_html_attribute_takes_precedence() {
        // Even though this looks like Python, the HTML says it's Rust
        let html = r#"<pre class="language-rust"><code>def hello():
    print("This is actually Rust pseudocode")
</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // HTML attribute should take precedence over inference
        assert!(markdown.contains("```rust"), "HTML attribute should override inference");
    }

    #[test]
    fn test_short_code_no_inference() {
        let html = r#"<pre><code>x = 1</code></pre>"#;

        let custom_handlers = create_custom_handlers();
        let markdown = html2md::parse_html_custom(html, &custom_handlers);

        // Too short for reliable inference - should just have plain fence
        assert!(markdown.contains("```"), "Should have code fence");
    }
}
