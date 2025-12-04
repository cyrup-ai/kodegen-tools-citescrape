//! Custom tag handlers for html2md conversion
//!
//! This module contains specialized TagHandler implementations that extend
//! html2md's default behavior to preserve additional semantic information.

pub mod code_language_handler;
pub mod inline_element_handler;
pub mod link_handler;

use std::collections::HashMap;
use html2md::TagHandlerFactory;

/// Create HashMap of custom tag handlers for html2md::parse_html_custom
pub fn create_custom_handlers() -> HashMap<String, Box<dyn TagHandlerFactory>> {
    let mut handlers: HashMap<String, Box<dyn TagHandlerFactory>> = HashMap::new();

    // Code block handlers (existing)
    handlers.insert("pre".to_string(), Box::new(code_language_handler::CodeLanguageHandlerFactory));
    handlers.insert("code".to_string(), Box::new(code_language_handler::CodeLanguageHandlerFactory));
    handlers.insert("samp".to_string(), Box::new(code_language_handler::CodeLanguageHandlerFactory));

    // Inline element handlers (NEW)
    // Semantic inline elements
    handlers.insert("kbd".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("var".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("abbr".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("dfn".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("mark".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    
    // Text formatting
    handlers.insert("strong".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("em".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("b".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("i".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("u".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("s".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("small".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    
    // Edit tracking
    handlers.insert("ins".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("del".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("sub".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    handlers.insert("sup".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));
    
    // Generic container (used by many frameworks)
    handlers.insert("span".to_string(), Box::new(inline_element_handler::InlineElementHandlerFactory));

    // Link handler (NEW: preserves link text from aria-label, title, and nested elements)
    handlers.insert("a".to_string(), Box::new(link_handler::LinkHandlerFactory));

    handlers
}
