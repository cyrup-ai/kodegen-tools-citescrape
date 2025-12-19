use super::super::{Element, text_util::{JoinOnStringIterator, TrimDocumentWhitespace, concat_strings}};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn img_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    let mut src: Option<String> = None;  // ✅ Clear naming - this is the src attribute
    let mut alt: Option<String> = None;
    let mut title: Option<String> = None;
    
    for attr in element.attrs.iter() {
        let name = &attr.name.local;
        match name.as_ref() {
            "src" => src = Some(attr.value.to_string()),
            "alt" => alt = Some(attr.value.to_string()),
            "title" => title = Some(attr.value.to_string()),
            // Modern HTML5 img attributes - preserve in faithful mode, ignore in pure mode
            "srcset" | "sizes" | "loading" | "decoding" | "width" | "height" 
            | "crossorigin" | "referrerpolicy" | "ismap" | "usemap" => {
                serialize_if_faithful!(handlers, element, 0);
            }
            // Unknown attributes - let serialize_if_faithful decide
            _ => {
                serialize_if_faithful!(handlers, element, 0);
            }
        }
    }

    // src is required for valid markdown image syntax
    src.as_ref()?;

    let process_alt_title = |text: String| {
        text.lines()
            .map(|line| line.trim_document_whitespace().replace('"', "\\\""))
            .filter(|line| !line.is_empty())
            .join("\n")
    };

    // Handle new lines in alt
    let alt = alt.map(process_alt_title);

    // Handle new lines in title
    let title = title.map(process_alt_title);

    // Escape markdown special characters in src URL
    let src = src.map(|text| text.replace('(', "\\(").replace(')', "\\)"));

    let has_spaces_in_link = src.as_ref().is_some_and(|link| link.contains(' '));

    // Build markdown image syntax: ![alt](src "title")
    let empty_string = String::new();
    let md = concat_strings!(
        "![",
        alt.as_ref().unwrap_or(&empty_string),
        "](",
        if has_spaces_in_link { "<" } else { "" },
        src.as_ref().unwrap_or(&empty_string),
        title
            .as_ref()
            .map_or(String::new(), |t| concat_strings!(" \"", t, "\"")),
        if has_spaces_in_link { ">" } else { "" },
        ")"
    );
    Some(md.into())
}
