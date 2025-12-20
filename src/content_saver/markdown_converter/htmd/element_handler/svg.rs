//! Handler for SVG and Canvas elements
//!
//! SVG elements contain visual diagrams with spatial coordinates.
//! Text extraction without preserving coordinates produces garbled output.
//! Therefore, SVG and canvas elements are discarded entirely during markdown conversion.

use super::super::Element;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<svg>` element - discard entirely
///
/// SVG diagrams have no meaningful markdown representation.
/// Text nodes within SVG (<text> elements) have x/y coordinates
/// that define spatial relationships. Extracting text without
/// coordinates produces nonsense like "8 b c d e f Group a b c".
///
/// Examples of SVG content:
/// - Bar charts, line graphs, pie charts
/// - Architecture diagrams, flowcharts
/// - ASCII art rendered as SVG
/// - Mathematical diagrams
///
/// Rationale: Better to skip than produce garbled content.
pub(super) fn svg_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    // In faithful mode with attributes, serialize as HTML
    serialize_if_faithful!(handlers, element, 0);
    
    // In pure mode or faithful mode without attributes: discard
    Some("".into())
}

/// Handle `<canvas>` element - discard entirely  
///
/// Canvas elements are bitmap rendering surfaces with no DOM children.
/// Content is rendered via JavaScript - no semantic content to extract.
/// Only fallback text (for non-JS browsers) might exist, which is not
/// meaningful markdown content.
///
/// Rationale: Better to skip than extract irrelevant fallback text.
pub(super) fn canvas_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    // In faithful mode with attributes, serialize as HTML
    serialize_if_faithful!(handlers, element, 0);
    
    // In pure mode or faithful mode without attributes: discard
    Some("".into())
}
