//! Handler for media elements: <video>, <audio>, <source>, <track>
//!
//! Converts media embeds to links or poster images.

use super::super::{Element, text_util::concat_strings};
use super::element_util::get_attr;
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;
use markup5ever_rcdom::NodeData;

/// Handle `<video>` element -> link or poster image
pub(super) fn video_handler(
    handlers: &dyn Handlers,
    element: Element,
) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    
    let src = get_attr(element.attrs, "src");
    let poster = get_attr(element.attrs, "poster");
    
    // Priority: poster image > video link > <source> children
    if let Some(poster_url) = poster {
        let poster_url = poster_url.replace('(', "\\(").replace(')', "\\)");
        return Some(concat_strings!("![Video](", poster_url, ")").into());
    }
    
    if let Some(video_url) = src {
        let video_url = video_url.replace('(', "\\(").replace(')', "\\)");
        return Some(concat_strings!("![Video](", video_url, ")").into());
    }
    
    // Selectively process ONLY <source> element children, ignore text nodes (fallback content)
    for child in element.node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && name.local.as_ref() == "source"
            && let Some(result) = handlers.handle(child)
            && !result.content.is_empty()
        {
            return Some(result);
        }
        // Text nodes (fallback content) are completely ignored - no match arm needed
    }
    
    // No valid source found
    Some("".into())
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
    
    // Selectively process ONLY <source> element children, ignore text nodes (fallback content)
    for child in element.node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &child.data
            && name.local.as_ref() == "source"
            && let Some(result) = handlers.handle(child)
            && !result.content.is_empty()
        {
            return Some(result);
        }
        // Text nodes (fallback content) are completely ignored - no match arm needed
    }
    
    // No valid source found
    Some("".into())
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
        let url_escaped = url.replace('(', "\\(").replace(')', "\\)");
        
        // Determine media type from MIME type attribute or file extension
        let is_audio = media_type.as_ref().is_some_and(|t| t.starts_with("audio"))
            || is_audio_extension(&url);
        
        let is_video = media_type.as_ref().is_some_and(|t| t.starts_with("video"))
            || is_video_extension(&url);
        
        // Generate appropriate markdown
        if is_audio {
            // Audio files use link syntax with emoji
            return Some(concat_strings!("[🔊 Audio](", url_escaped, ")").into());
        } else if is_video {
            // Video files use image syntax for better rendering
            return Some(concat_strings!("![Video](", url_escaped, ")").into());
        } else {
            // Unknown media type - use generic label with image syntax
            return Some(concat_strings!("![Media](", url_escaped, ")").into());
        }
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

/// Check if URL has video file extension
fn is_video_extension(url: &str) -> bool {
    const VIDEO_EXTENSIONS: &[&str] = &[
        ".mov", ".mp4", ".m4v", ".webm", ".ogv", ".ogg",
        ".avi", ".mkv", ".flv", ".wmv", ".mpg", ".mpeg"
    ];
    
    let url_lower = url.to_lowercase();
    VIDEO_EXTENSIONS.iter().any(|ext| url_lower.ends_with(ext))
}

/// Check if URL has audio file extension
fn is_audio_extension(url: &str) -> bool {
    const AUDIO_EXTENSIONS: &[&str] = &[
        ".mp3", ".m4a", ".wav", ".flac", ".aac", 
        ".opus", ".oga", ".weba"
    ];
    
    let url_lower = url.to_lowercase();
    AUDIO_EXTENSIONS.iter().any(|ext| url_lower.ends_with(ext))
}
