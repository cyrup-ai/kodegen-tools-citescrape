//! Handler for <footer> elements
//!
//! Footer elements contain site-wide footer content like copyright notices,
//! legal links, and social media links. This content is not relevant to the
//! page's main content and should be discarded entirely.

use super::super::Element;
use super::{HandlerResult, Handlers};

/// Handler for `<footer>` elements - discards all content.
pub(super) fn footer_handler(_handlers: &dyn Handlers, _element: Element) -> Option<HandlerResult> {
    // Discard all footer content - return empty string to consume element
    Some("".into())
}
