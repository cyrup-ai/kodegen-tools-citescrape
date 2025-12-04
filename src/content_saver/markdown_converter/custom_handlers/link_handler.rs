//! Custom link handler that preserves link text from HTML attributes.
//!
//! This handler replaces html2md's default anchor handler to extract link text
//! from multiple sources: nested text nodes, aria-label, title, alt, and href.
//!
//! Fallback strategy: text content → aria-label → title → alt → href (cleaned)

use html2md::{Handle, StructuredPrinter, TagHandler, TagHandlerFactory};
use markup5ever_rcdom::NodeData;

/// Custom handler for `<a>` tags with intelligent text extraction
pub struct LinkHandler {
    /// Extracted link text (from various sources)
    link_text: String,
    /// Link href URL
    href: String,
    /// Link title (tooltip)
    title: Option<String>,
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            link_text: String::new(),
            href: String::new(),
            title: None,
        }
    }

    /// Recursively extract text from all nested text nodes
    fn extract_nested_text(handle: &Handle) -> String {
        let mut text = String::new();

        match handle.data {
            NodeData::Text { ref contents } => {
                text.push_str(&contents.borrow());
            }
            NodeData::Element { .. } => {
                for child in handle.children.borrow().iter() {
                    text.push_str(&Self::extract_nested_text(child));
                }
            }
            _ => {
                for child in handle.children.borrow().iter() {
                    text.push_str(&Self::extract_nested_text(child));
                }
            }
        }

        text
    }

    /// Extract attribute value from HTML element
    fn get_attribute(tag: &Handle, attr_name: &str) -> Option<String> {
        if let NodeData::Element { ref attrs, .. } = tag.data {
            let attrs = attrs.borrow();
            if let Some(attr) = attrs.iter().find(|a| &*a.name.local == attr_name) {
                let value = attr.value.to_string();
                if !value.trim().is_empty() {
                    return Some(value);
                }
            }
        }
        None
    }

    /// Clean URL for display (remove query params, fragments, etc.)
    fn clean_url_for_display(url: &str) -> String {
        // Remove leading slash for relative URLs
        let cleaned = url.trim_start_matches('/');
        
        // Remove query parameters and fragments
        let cleaned = cleaned.split('?').next().unwrap_or(cleaned);
        let cleaned = cleaned.split('#').next().unwrap_or(cleaned);
        
        // Remove file extensions for cleaner display
        let cleaned = cleaned.trim_end_matches(".html");
        let cleaned = cleaned.trim_end_matches(".htm");
        let cleaned = cleaned.trim_end_matches("/index");
        
        // Capitalize first letter and replace hyphens/underscores with spaces
        let cleaned = cleaned.replace(['-', '_'], " ");
        
        // Title case the first word
        if let Some(first_char) = cleaned.chars().next() {
            format!("{}{}", first_char.to_uppercase(), &cleaned[first_char.len_utf8()..])
        } else {
            cleaned.to_string()
        }
    }

    /// Extract link text using fallback hierarchy
    fn extract_link_text_with_fallback(tag: &Handle) -> String {
        // Priority 1: Direct text content from nested elements
        let text = Self::extract_nested_text(tag);
        if !text.trim().is_empty() {
            return text.trim().to_string();
        }

        // Priority 2: aria-label attribute (accessibility best practice)
        if let Some(aria_label) = Self::get_attribute(tag, "aria-label") {
            return aria_label;
        }

        // Priority 3: title attribute (tooltip text)
        if let Some(title) = Self::get_attribute(tag, "title") {
            return title;
        }

        // Priority 4: alt attribute (for image links)
        if let Some(alt) = Self::get_attribute(tag, "alt") {
            return alt;
        }

        // Priority 5: href as fallback (cleaned for readability)
        if let Some(href) = Self::get_attribute(tag, "href") {
            return Self::clean_url_for_display(&href);
        }

        // Last resort: empty string
        String::new()
    }

    /// Extract href attribute
    fn extract_href(tag: &Handle) -> String {
        Self::get_attribute(tag, "href").unwrap_or_default()
    }

    /// Extract title attribute
    fn extract_title(tag: &Handle) -> Option<String> {
        Self::get_attribute(tag, "title")
    }
}

impl TagHandler for LinkHandler {
    fn handle(&mut self, tag: &Handle, printer: &mut StructuredPrinter) {
        // Extract link attributes
        self.link_text = Self::extract_link_text_with_fallback(tag);
        self.href = Self::extract_href(tag);
        self.title = Self::extract_title(tag);

        // Opening bracket for markdown link
        printer.append_str("[");
    }

    fn after_handle(&mut self, printer: &mut StructuredPrinter) {
        // Append link text (we already extracted it, don't process children)
        printer.append_str(&self.link_text);
        
        // Closing bracket and URL
        printer.append_str("](");
        printer.append_str(&self.href);
        
        // Optional: Add title as markdown link title
        if let Some(ref title) = self.title
            && !title.is_empty() && title != &self.link_text {
                printer.append_str(" \"");
                printer.append_str(title);
                printer.append_str("\"");
            }
        
        printer.append_str(")");
    }

    fn skip_descendants(&self) -> bool {
        // Skip processing children since we manually extracted text
        true
    }
}

/// Factory for creating LinkHandler instances
pub struct LinkHandlerFactory;

impl TagHandlerFactory for LinkHandlerFactory {
    fn instantiate(&self) -> Box<dyn TagHandler> {
        Box::new(LinkHandler::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use html2md;
    use std::collections::HashMap;

    #[test]
    fn test_link_with_text_content() {
        let html = r#"<a href="/guide">Installation Guide</a>"#;
        let mut handlers = HashMap::new();
        handlers.insert("a".to_string(), Box::new(LinkHandlerFactory) as Box<dyn TagHandlerFactory>);
        
        let markdown = html2md::parse_html_custom(html, &handlers);
        assert!(markdown.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_with_aria_label() {
        let html = r#"<a href="/guide" aria-label="Installation Guide"></a>"#;
        let mut handlers = HashMap::new();
        handlers.insert("a".to_string(), Box::new(LinkHandlerFactory) as Box<dyn TagHandlerFactory>);
        
        let markdown = html2md::parse_html_custom(html, &handlers);
        assert!(markdown.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_with_title_attribute() {
        let html = r#"<a href="/guide" title="Installation Guide"></a>"#;
        let mut handlers = HashMap::new();
        handlers.insert("a".to_string(), Box::new(LinkHandlerFactory) as Box<dyn TagHandlerFactory>);
        
        let markdown = html2md::parse_html_custom(html, &handlers);
        assert!(markdown.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_with_nested_span() {
        let html = r#"<a href="/guide"><span class="link-text">Installation Guide</span></a>"#;
        let mut handlers = HashMap::new();
        handlers.insert("a".to_string(), Box::new(LinkHandlerFactory) as Box<dyn TagHandlerFactory>);
        
        let markdown = html2md::parse_html_custom(html, &handlers);
        assert!(markdown.contains("[Installation Guide](/guide)"));
    }

    #[test]
    fn test_link_fallback_to_href() {
        let html = r#"<a href="/installation-guide"></a>"#;
        let mut handlers = HashMap::new();
        handlers.insert("a".to_string(), Box::new(LinkHandlerFactory) as Box<dyn TagHandlerFactory>);
        
        let markdown = html2md::parse_html_custom(html, &handlers);
        // Should clean up href: "/installation-guide" -> "Installation guide"
        assert!(markdown.contains("[Installation guide](/installation-guide)"));
    }

    #[test]
    fn test_aria_label_takes_precedence_over_empty_text() {
        let html = r#"<a href="/guide" aria-label="Installation Guide">   </a>"#;
        let mut handlers = HashMap::new();
        handlers.insert("a".to_string(), Box::new(LinkHandlerFactory) as Box<dyn TagHandlerFactory>);
        
        let markdown = html2md::parse_html_custom(html, &handlers);
        assert!(markdown.contains("[Installation Guide](/guide)"));
    }
}
