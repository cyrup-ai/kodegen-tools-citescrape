mod anchor;
mod aside;
mod blockquote;
mod br;
mod button;
mod caption;
mod code;
mod details;
mod dialog;
mod div;
mod element_util;
mod input;
mod footer;
mod header;
mod emphasis;
mod head_body;
mod headings;
mod hr;
mod html;
mod img;
mod li;
mod list;
mod nav;
mod p;
mod pre;
mod section;
mod span;
mod table;
mod tbody;
mod td_th;
mod thead;
mod tr;
pub mod language_inference;
pub mod language_patterns;
pub mod list_processing;

use super::{
    dom_walker::walk_node,
    options::{Options, TranslationMode},
    text_util::concat_strings,
};
use self::element_util::serialize_element;

use super::Element;
use anchor::AnchorElementHandler;
use aside::aside_handler;
use blockquote::blockquote_handler;
use br::br_handler;
use caption::caption_handler;
use code::code_handler;
use details::{details_handler, summary_handler};
use button::button_handler;
use dialog::dialog_handler;
use div::div_handler;
use input::input_handler;
use emphasis::emphasis_handler;
use footer::footer_handler;
use head_body::head_body_handler;
use header::header_handler;
use headings::headings_handler;
use hr::hr_handler;
use html::html_handler;
use html5ever::Attribute;
use img::img_handler;
use li::list_item_handler;
use list::list_handler;
use markup5ever_rcdom::Node;
use nav::nav_handler;
use p::p_handler;
use pre::pre_handler;
use section::section_handler;
use span::span_handler;
use std::{collections::HashMap, rc::Rc};
use table::table_handler;
use tbody::tbody_handler;
use td_th::td_th_handler;
use thead::thead_handler;
use tr::tr_handler;

/// The processing result of an `ElementHandler`.
pub struct HandlerResult {
    /// The converted content.
    pub content: String,
    /// See [`Element::markdown_translated`]
    pub markdown_translated: bool,
}

impl From<String> for HandlerResult {
    fn from(value: String) -> Self {
        HandlerResult {
            content: value,
            markdown_translated: true,
        }
    }
}

impl From<&str> for HandlerResult {
    fn from(value: &str) -> Self {
        HandlerResult {
            content: value.to_string(),
            markdown_translated: true,
        }
    }
}

/// Trait for handling the conversion of a specific HTML element to Markdown.
pub trait ElementHandler: Send + Sync {
    /// Append additional content to the end of the converted Markdown.
    fn append(&self) -> Option<String> {
        None
    }

    /// Handle the conversion of an element.
    fn handle(&self, handlers: &dyn Handlers, element: Element) -> Option<HandlerResult>;
}

impl<F> ElementHandler for F
where
    F: (Fn(&dyn Handlers, Element) -> Option<HandlerResult>) + Send + Sync,
{
    fn handle(&self, handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
        self(handlers, element)
    }
}

/// Builtin element handlers
pub(crate) struct ElementHandlers {
    pub(crate) handlers: Vec<Box<dyn ElementHandler>>,
    pub(crate) tag_to_handler_indices: HashMap<&'static str, Vec<usize>>,
    pub(crate) options: Options,
}

impl ElementHandlers {
    pub fn new(options: Options) -> Self {
        let mut handlers = Self {
            handlers: Vec::new(),
            tag_to_handler_indices: HashMap::new(),
            options,
        };

        // img
        handlers.add_handler(vec!["img"], img_handler);

        // a
        handlers.add_handler(vec!["a"], AnchorElementHandler::new());

        // list
        handlers.add_handler(vec!["ol", "ul"], list_handler);

        // li
        handlers.add_handler(vec!["li"], list_item_handler);

        // quote
        handlers.add_handler(vec!["blockquote"], blockquote_handler);

        // code
        handlers.add_handler(vec!["code"], code_handler);

        // strong
        handlers.add_handler(vec!["strong", "b"], bold_handler);

        // italic
        handlers.add_handler(vec!["i", "em"], italic_handler);

        // headings
        handlers.add_handler(vec!["h1", "h2", "h3", "h4", "h5", "h6"], headings_handler);

        // br
        handlers.add_handler(vec!["br"], br_handler);

        // hr
        handlers.add_handler(vec!["hr"], hr_handler);

        // table
        handlers.add_handler(vec!["table"], table_handler);

        // td, th
        handlers.add_handler(vec!["td", "th"], td_th_handler);

        // tr
        handlers.add_handler(vec!["tr"], tr_handler);

        // tbody
        handlers.add_handler(vec!["tbody"], tbody_handler);

        // thead
        handlers.add_handler(vec!["thead"], thead_handler);

        // caption
        handlers.add_handler(vec!["caption"], caption_handler);

        // p
        handlers.add_handler(vec!["p"], p_handler);

        // pre
        handlers.add_handler(vec!["pre"], pre_handler);

        // head, body
        handlers.add_handler(vec!["head", "body"], head_body_handler);

        // html
        handlers.add_handler(vec!["html"], html_handler);

        handlers.add_handler(vec!["span"], span_handler);

        // Other block elements. This is taken from the [CommonMark
        // spec](https://spec.commonmark.org/0.31.2/#html-blocks).
        handlers.add_handler(
            vec![
                "address",
                "article",
                "aside",
                "base",
                "basefont",
                "center",
                "col",
                "colgroup",
                "dd",
                "dialog",
                "dir",
                "div",
                "dl",
                "dt",
                "fieldset",
                "figcaption",
                "figure",
                "footer",
                "form",
                "frame",
                "frameset",
                "header",
                "iframe",
                "legend",
                "link",
                "main",
                "menu",
                "menuitem",
                "nav",
                "noframes",
                "optgroup",
                "option",
                "param",
                "script",
                "search",
                "section",
                "style",
                "textarea",
                "tfoot",
                "title",
                "track",
            ],
            block_handler,
        );

        // details/summary - collapsible sections converted to markdown
        handlers.add_handler(vec!["details"], details_handler);
        handlers.add_handler(vec!["summary"], summary_handler);

        // div - specialized handling for expressive-code wrappers and widget filtering
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["div"], div_handler);

        // section - widget filtering for social/cookie/ad sections
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["section"], section_handler);

        // aside - widget filtering for social/cookie/ad asides
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["aside"], aside_handler);

        // nav - extract h1 only, discard navigation content
        // Must be registered AFTER block_handler to override it
        handlers.add_handler(vec!["nav"], nav_handler);

        // header - extract h1 only, discard header chrome
        // Must be registered AFTER block_handler to override it
        handlers.add_handler(vec!["header"], header_handler);

        // footer - discard all footer content
        // Must be registered AFTER block_handler to override it
        handlers.add_handler(vec!["footer"], footer_handler);

        // form, iframe - skip entirely (no markdown equivalent)
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["form", "iframe"], form_iframe_handler);

        // button - skip entirely (no markdown equivalent)
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["button"], button_handler);

        // input, select, textarea - form inputs, skip entirely
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["input", "select", "textarea"], input_handler);

        // dialog - skip entirely (no markdown equivalent)
        // Must be registered AFTER block_handler to take priority
        handlers.add_handler(vec!["dialog"], dialog_handler);

        // script, style - always discard (no markdown representation)
        // Must be registered AFTER block_handler to take precedence
        handlers.add_handler(vec!["script", "style"], script_style_handler);

        handlers
    }

    pub fn add_handler<Handler>(&mut self, tags: Vec<&'static str>, handler: Handler)
    where
        Handler: ElementHandler + 'static,
    {
        assert!(!tags.is_empty(), "tags cannot be empty.");
        let handler_idx = self.handlers.len();
        self.handlers.push(Box::new(handler));
        // Update tag to handler indices
        for tag in tags {
            let indices = self
                .tag_to_handler_indices
                .entry(tag)
                .or_default();
            indices.insert(0, handler_idx);
        }
    }

    pub fn handle(
        &self,
        node: &Rc<Node>,
        tag: &str,
        attrs: &[Attribute],
        markdown_translated: bool,
        skipped_handlers: usize,
        is_pre: bool,
    ) -> Option<HandlerResult> {
        match self.find_handler(tag, skipped_handlers) {
            Some(handler) => handler.handle(
                self,
                Element {
                    node,
                    tag,
                    attrs,
                    markdown_translated,
                    skipped_handlers,
                    is_pre,
                },
            ),
            None => {
                if self.options.translation_mode == TranslationMode::Faithful {
                    Some(HandlerResult {
                        content: serialize_element(
                            self,
                            &Element {
                                node,
                                tag,
                                attrs,
                                markdown_translated,
                                skipped_handlers: 0,
                                is_pre,
                            },
                        ),
                        markdown_translated: false,
                    })
                } else {
                    // Default behavior: walk children and return their content
                    Some(self.walk_children(node, is_pre))
                }
            }
        }
    }

    fn find_handler(&self, tag: &str, skipped_handlers: usize) -> Option<&dyn ElementHandler> {
        let handler_indices = self.tag_to_handler_indices.get(tag)?;
        let idx = *handler_indices.get(skipped_handlers)?;
        Some(self.handlers[idx].as_ref())
    }
}

/// Provides access to the handlers for processing elements and nodes.
///
/// Handlers can use this to delegate to other handlers or recursively process child nodes.
pub trait Handlers {
    /// Skip the current handler and fall back to the previous handler (earlier in registration order).
    fn fallback(&self, element: Element) -> Option<HandlerResult>;

    /// Process a `markup5ever` node through the handlers.
    fn handle(&self, node: &Rc<Node>) -> Option<HandlerResult>;

    /// Walks children of a node and returns both content and markdown_translated status.
    /// The `is_pre` parameter indicates whether we're inside a <pre> or <code> element.
    fn walk_children(&self, node: &Rc<Node>, is_pre: bool) -> HandlerResult;

    /// Get the conversion options.
    fn options(&self) -> &Options;
}

impl Handlers for ElementHandlers {
    fn fallback(&self, element: Element) -> Option<HandlerResult> {
        self.handle(
            element.node,
            element.tag,
            element.attrs,
            element.markdown_translated,
            element.skipped_handlers + 1,
            element.is_pre,
        )
    }

    fn handle(&self, node: &Rc<Node>) -> Option<HandlerResult> {
        let mut buffer = String::new();
        let markdown_translated = walk_node(node, &mut buffer, self, None, true, false);
        Some(HandlerResult {
            content: buffer,
            markdown_translated,
        })
    }

    fn walk_children(&self, node: &Rc<Node>, is_pre: bool) -> HandlerResult {
        let mut buffer = String::new();
        let tag = super::node_util::get_node_tag_name(node);
        let is_block = tag.is_some_and(super::dom_walker::is_block_element);
        
        // Compute is_pre for children: inherit parent's is_pre OR this element is pre/code
        let is_pre_for_children = is_pre || tag.is_some_and(|t| t == "pre" || t == "code");
        
        let markdown_translated =
            super::dom_walker::walk_children(node, &mut buffer, self, is_block, is_pre_for_children);
        HandlerResult {
            content: buffer,
            markdown_translated,
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }
}

fn block_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    if handlers.options().translation_mode == TranslationMode::Pure {
        let content = handlers.walk_children(element.node, element.is_pre).content;
        let content = content.trim_matches('\n');
        Some(concat_strings!("\n\n", content, "\n\n").into())
    } else {
        Some(HandlerResult {
            content: serialize_element(handlers, &element),
            markdown_translated: false,
        })
    }
}

fn bold_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    emphasis_handler(handlers, element, "**")
}

fn italic_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    emphasis_handler(handlers, element, "*")
}

/// Handler for form and iframe elements - discard entirely.
/// These are interactive elements with no markdown equivalent.
fn form_iframe_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    Some("".into())
}

/// Handler for script and style elements - discard entirely.
/// These contain JavaScript/CSS that should never appear in markdown.
fn script_style_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    Some("".into())
}
