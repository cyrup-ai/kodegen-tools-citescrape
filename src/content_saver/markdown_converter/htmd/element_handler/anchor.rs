use parking_lot::Mutex;
use std::rc::Rc;

use super::super::{
    Element,
    node_util::{get_parent_node, get_node_tag_name},
    options::{LinkReferenceStyle, LinkStyle},
    text_util::{JoinOnStringIterator, StripWhitespace, TrimDocumentWhitespace, concat_strings, is_invisible_unicode},
};
use super::{ElementHandler, HandlerResult, Handlers};
use super::element_util::is_widget_element_with_context;
use crate::serialize_if_faithful;
use html5ever::Attribute;
use markup5ever_rcdom::{Node, NodeData};

/// Check if an anchor element is a heading permalink/anchor link.
///
/// Returns true if ANY of these conditions are met:
///
/// 1. **CLASS-BASED**: `<a>` with class containing:
///    - "anchor", "permalink", "header-link", "heading-link", "hash-link"
///
/// 2. **ARIA-HIDDEN**: `<a>` with `aria-hidden="true"`
///    (often used for icon-only permalink anchors)
///
/// 3. **ARIA-LABEL**: `<a>` with `aria-label` containing "Navigate to header"
///    (accessibility text pattern)
///
/// 4. **CONTENT-BASED**: Anchor text content is ONLY:
///    - "#", "§" (section sign), "¶" (pilcrow)
///    - Invisible Unicode characters (U+200B, U+200C, U+200D, U+FEFF, etc.)
///
/// This function uses the same proven detection logic from `headings.rs` but
/// applies it to anchor elements processed as siblings rather than children.
fn is_heading_anchor_link(attrs: &[Attribute], node: &Rc<Node>) -> bool {
    // Check for permalink anchor classes
    for attr in attrs.iter() {
        let attr_name = &attr.name.local;
        
        if attr_name == "class" {
            let class_value = attr.value.to_lowercase();
            if class_value.contains("anchor")
                || class_value.contains("permalink")
                || class_value.contains("header-link")
                || class_value.contains("heading-link")
                || class_value.contains("hash-link")
            {
                return true;
            }
        }
        
        // aria-hidden anchors are typically icon-only permalinks
        if attr_name == "aria-hidden" && &*attr.value == "true" {
            return true;
        }
        
        // aria-label with accessibility text
        if attr_name == "aria-label" {
            let label_lower = attr.value.to_lowercase();
            let normalized: String = label_lower
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if normalized.contains("navigate to header") 
                || normalized.contains("permalink")
                || normalized.contains("anchor link")
            {
                return true;
            }
        }
    }
    
    // Check text content for common permalink symbols or invisible Unicode
    let text = get_anchor_text_content(node);
    let text_trimmed = text.trim();
    
    // Exact match for permalink symbols
    if text_trimmed == "#" || text_trimmed == "§" || text_trimmed == "¶" {
        return true;
    }
    
    // Check if anchor text contains only invisible Unicode characters
    // This catches anchors like <a href="#id">​</a> where ​ is U+200B
    !text_trimmed.is_empty() 
        && text_trimmed.chars().all(is_invisible_unicode)
}

/// Get text content of an anchor node (recursive).
///
/// Filters out invisible Unicode characters during collection to get
/// the actual visible text content.
fn get_anchor_text_content(node: &Rc<Node>) -> String {
    let mut text = String::new();
    collect_anchor_text(node, &mut text);
    text
}

/// Recursively collect text content from an anchor node tree.
///
/// Filters invisible Unicode characters to determine if the anchor
/// has any visible text content.
fn collect_anchor_text(node: &Rc<Node>, buffer: &mut String) {
    match &node.data {
        NodeData::Text { contents } => {
            // Filter out invisible Unicode characters during text extraction
            let text: String = contents.borrow()
                .chars()
                .filter(|c| !is_invisible_unicode(*c))
                .collect();
            buffer.push_str(&text);
        }
        NodeData::Element { .. } => {
            for child in node.children.borrow().iter() {
                collect_anchor_text(child, buffer);
            }
        }
        _ => {}
    }
}

pub(super) struct AnchorElementHandler {
    links: Mutex<Vec<String>>,
}

impl ElementHandler for AnchorElementHandler {
    fn append(&self) -> Option<String> {
        let mut links = self.links.lock();
        if links.is_empty() {
            return None;
        }
        let result = concat_strings!("\n\n", links.join("\n"), "\n\n");
        links.clear();
        Some(result)
    }

    fn handle(&self, handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
        // Get parent tag name for context-aware filtering
        let parent_node = get_parent_node(element.node);
        let parent_tag = parent_node.as_ref()
            .and_then(|parent| get_node_tag_name(parent));
        
        // Skip widget elements (but preserve accessibility in table contexts)
        if is_widget_element_with_context(element.attrs, parent_tag) {
            return Some("".into());
        }
        
        // Skip heading permalink/anchor links
        if is_heading_anchor_link(element.attrs, element.node) {
            return Some("".into());
        }
        
        let mut link: Option<String> = None;
        let mut title: Option<String> = None;
        for attr in element.attrs.iter() {
            let name = &attr.name.local;
            if name == "href" {
                link = Some(attr.value.to_string())
            } else if name == "title" {
                title = Some(attr.value.to_string());
            } else {
                // This is an attribute which can't be translated to Markdown.
                serialize_if_faithful!(handlers, element, 0);
            }
        }

        let Some(link) = link else {
            return Some(handlers.walk_children(element.node, element.is_pre));
        };

        let process_title = |text: String| {
            text.lines()
                .map(|line| line.trim_document_whitespace().replace('"', "\\\""))
                .filter(|line| !line.is_empty())
                .join("\n")
        };

        // Handle new lines in title
        let title = title.map(process_title);

        let link = link.replace('(', "\\(").replace(')', "\\)");

        let content = handlers.walk_children(element.node, element.is_pre).content;
        let md = match handlers.options().link_style {
            LinkStyle::Inlined => self.build_inlined_anchor(&content, link, title, false),
            LinkStyle::InlinedPreferAutolinks => {
                self.build_inlined_anchor(&content, link, title, true)
            }
            LinkStyle::Referenced => self.build_referenced_anchor(
                &content,
                link,
                title,
                &handlers.options().link_reference_style,
            ),
        };

        Some(md.into())
    }
}

impl AnchorElementHandler {
    pub(super) fn new() -> Self {
        Self {
            links: Mutex::new(Vec::new())
        }
    }

    fn build_inlined_anchor(
        &self,
        content: &str,
        link: String,
        title: Option<String>,
        prefer_autolinks: bool,
    ) -> String {
        if prefer_autolinks && content == link {
            return concat_strings!("<", link, ">");
        }

        let has_spaces_in_link = link.contains(' ');
        let (content, _) = content.strip_leading_document_whitespace();
        let (content, trailing_whitespace) = content.strip_trailing_document_whitespace();
        concat_strings!(
            "[",
            content,
            "](",
            if has_spaces_in_link { "<" } else { "" },
            link,
            title
                .as_ref()
                .map_or(String::new(), |t| concat_strings!(" \"", t, "\"")),
            if has_spaces_in_link { ">" } else { "" },
            ")",
            trailing_whitespace.unwrap_or("")
        )
    }

    fn build_referenced_anchor(
        &self,
        content: &str,
        link: String,
        title: Option<String>,
        style: &LinkReferenceStyle,
    ) -> String {
        let title = title.map_or(String::new(), |t| concat_strings!(" \"", t, "\""));
        
        let mut links = self.links.lock();
        
        let (current, append) = match style {
            LinkReferenceStyle::Full => {
                let index = links.len() + 1;
                (
                    concat_strings!("[", content, "][", index.to_string(), "]"),
                    concat_strings!("[", index.to_string(), "]: ", link, title),
                )
            }
            LinkReferenceStyle::Collapsed => (
                concat_strings!("[", content, "][]"),
                concat_strings!("[", content, "]: ", link, title),
            ),
            LinkReferenceStyle::Shortcut => (
                concat_strings!("[", content, "]"),
                concat_strings!("[", content, "]: ", link, title),
            ),
        };
        
        links.push(append);
        current
    }
}
