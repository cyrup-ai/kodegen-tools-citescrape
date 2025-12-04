//! Generic inline element handler for comprehensive text extraction
//!
//! This handler ensures ALL inline HTML elements have their text content
//! properly extracted, even when html2md's default handlers fail.
//!
//! Handles semantic elements (<kbd>, <var>, <abbr>), formatting elements
//! (<strong>, <em>), and generic containers (<span>) used by modern frameworks.

use html2md::{Handle, StructuredPrinter, TagHandler, TagHandlerFactory};
use markup5ever_rcdom::NodeData;

/// Generic handler for inline HTML elements
pub struct InlineElementHandler {
    /// Tag name (kbd, var, strong, etc.)
    tag_name: String,
    /// Extracted text content
    text_content: Option<String>,
}

impl InlineElementHandler {
    pub fn new() -> Self {
        Self {
            tag_name: String::new(),
            text_content: None,
        }
    }

    /// Recursively extract raw text from HTML node tree
    /// 
    /// This is the CRITICAL function that ensures text is never lost.
    /// Identical pattern to CodeLanguageHandler::extract_raw_text()
    fn extract_raw_text(handle: &Handle) -> String {
        let mut text = String::new();

        match handle.data {
            NodeData::Text { ref contents } => {
                text.push_str(&contents.borrow());
            }
            NodeData::Element { .. } => {
                for child in handle.children.borrow().iter() {
                    text.push_str(&Self::extract_raw_text(child));
                }
            }
            _ => {
                for child in handle.children.borrow().iter() {
                    text.push_str(&Self::extract_raw_text(child));
                }
            }
        }

        text
    }

    /// Determine markdown formatting based on tag type
    fn get_markdown_wrapper(&self, text: &str) -> String {
        match self.tag_name.as_str() {
            // Strong emphasis
            "strong" | "b" => format!("**{}**", text),
            
            // Emphasis
            "em" | "i" => format!("*{}*", text),
            
            // Semantic inline code-like elements - use backticks
            "kbd" | "var" => format!("`{}`", text),
            
            // Strikethrough (if markdown supports it)
            "s" | "del" | "strike" => format!("~~{}~~", text),
            
            // Underline - preserve as HTML (markdown doesn't have native underline)
            "u" | "ins" => format!("<u>{}</u>", text),
            
            // Subscript/superscript - preserve as HTML
            "sub" => format!("<sub>{}</sub>", text),
            "sup" => format!("<sup>{}</sup>", text),
            
            // Abbreviations - preserve content, optionally add title
            "abbr" | "dfn" => text.to_string(),
            
            // Mark/highlight - preserve as HTML
            "mark" => format!("<mark>{}</mark>", text),
            
            // Generic containers and unknown elements - extract text only
            _ => text.to_string(),
        }
    }
}

impl TagHandler for InlineElementHandler {
    fn handle(&mut self, tag: &Handle, printer: &mut StructuredPrinter) {
        if let NodeData::Element { ref name, .. } = tag.data {
            self.tag_name = name.local.to_string();
            
            // Extract ALL text content from this element and its children
            let raw_text = Self::extract_raw_text(tag);
            
            // Apply markdown formatting based on element type
            let formatted = self.get_markdown_wrapper(&raw_text);
            
            // Output to markdown
            printer.append_str(&formatted);
            
            // Store for potential reuse
            self.text_content = Some(raw_text);
        }
    }

    fn after_handle(&mut self, _printer: &mut StructuredPrinter) {
        // No-op: all processing done in handle()
    }

    fn skip_descendants(&self) -> bool {
        // CRITICAL: Skip html2md's default child processing
        // We manually extracted all text in handle() via extract_raw_text()
        true
    }
}

/// Factory for creating InlineElementHandler instances
#[derive(Clone)]
pub struct InlineElementHandlerFactory;

impl TagHandlerFactory for InlineElementHandlerFactory {
    fn instantiate(&self) -> Box<dyn TagHandler> {
        Box::new(InlineElementHandler::new())
    }
}
