//! Custom code block handler that preserves language hints from HTML attributes.
//!
//! This handler replaces html2md's default CodeHandler to extract language information
//! from `data-language` attributes or CSS class names (e.g., `language-rust`).

use html2md::{Handle, StructuredPrinter, TagHandler, TagHandlerFactory};
use markup5ever_rcdom::NodeData;
use std::collections::HashMap;

/// Custom handler for `<pre>` and `<code>` tags that extracts language hints
pub struct CodeLanguageHandler {
    /// Language extracted from HTML attributes (e.g., "rust", "python")
    language: Option<String>,
    /// Tag type: "pre" or "code"
    code_type: String,
    /// Track if we're inside a nested code block
    inside_code: bool,
}

impl CodeLanguageHandler {
    pub fn new() -> Self {
        Self {
            language: None,
            code_type: String::new(),
            inside_code: false,
        }
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
                    // Closing fence
                    printer.insert_newline();
                    printer.append_str("```");
                    printer.insert_newline();
                }
            }
            "code" | "samp" => {
                // Don't wrap if already inside <pre> block
                if !self.inside_code {
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
            
            // Extract language for <pre> tags
            if self.code_type == "pre" {
                self.language = Self::extract_language(tag);
            }
            
            // Track nesting
            if self.code_type == "code" || self.code_type == "samp" {
                self.inside_code = true;
            }
            
            self.do_handle(printer, true);
        }
    }

    fn after_handle(&mut self, printer: &mut StructuredPrinter) {
        self.do_handle(printer, false);
    }

    fn skip_descendants(&self) -> bool {
        false
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
pub fn create_custom_handlers() -> HashMap<String, Box<dyn TagHandlerFactory>> {
    let mut handlers: HashMap<String, Box<dyn TagHandlerFactory>> = HashMap::new();
    
    // Register our custom handler for pre, code, and samp tags
    handlers.insert("pre".to_string(), Box::new(CodeLanguageHandlerFactory));
    handlers.insert("code".to_string(), Box::new(CodeLanguageHandlerFactory));
    handlers.insert("samp".to_string(), Box::new(CodeLanguageHandlerFactory));
    
    handlers
}
