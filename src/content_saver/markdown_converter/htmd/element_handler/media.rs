//! Handler for media elements: <video>, <audio>, <source>, <track>
//!
//! Converts media embeds to links or poster images.

use super::super::{Element, text_util::concat_strings};
use super::element_util::get_attr;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

/// Handle `<video>` element -> link or poster image
pub(super) fn video_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let src = get_attr(element.attrs, "src");
    let poster = get_attr(element.attrs, "poster");
    
    // Priority: poster image > video link > walk children for <source>
    if let Some(poster_url) = poster {
        let poster_url = poster_url.replace('(', "\\(").replace(')', "\\)");
        return Some(concat_strings!("![Video](", poster_url, ")").into());
    }
    
    if let Some(video_url) = src {
        let video_url = video_url.replace('(', "\\(").replace(')', "\\)");
        return Some(concat_strings!("[▶ Video](", video_url, ")").into());
    }
    
    // Check for <source> children
    Some(handlers.walk_children(element.node, element.is_pre))
}

/// Handle `<audio>` element -> link
pub(super) fn audio_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let src = get_attr(element.attrs, "src");
    
    if let Some(audio_url) = src {
        let audio_url = audio_url.replace('(', "\\(").replace(')', "\\)");
        return Some(concat_strings!("[🔊 Audio](", audio_url, ")").into());
    }
    
    // Check for <source> children
    Some(handlers.walk_children(element.node, element.is_pre))
}

/// Handle `<source>` element inside video/audio
pub(super) fn source_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let src = get_attr(element.attrs, "src");
    let media_type = get_attr(element.attrs, "type");
    
    if let Some(url) = src {
        let url = url.replace('(', "\\(").replace(')', "\\)");
        let label = if media_type.as_ref().is_some_and(|t| t.starts_with("audio")) {
            "🔊 Audio"
        } else {
            "▶ Media"
        };
        return Some(concat_strings!("[", label, "](", url, ")").into());
    }
    
    None
}

/// Handle `<track>` element (subtitles/captions) - skip
pub(super) fn track_handler(
    _handlers: &dyn Handlers,
    _element: Element,
) -> Option<HandlerResult> {
    // Track elements contain metadata, not content
    Some("".into())
}
