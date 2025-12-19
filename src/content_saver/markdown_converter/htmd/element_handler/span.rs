use cssparser::{Parser, ParserInput, Token};
use html5ever::Attribute;
use markup5ever_rcdom::NodeData;

use super::super::{Element, text_util::{StripWhitespace, concat_strings}};
use super::{HandlerResult, Handlers};
use crate::serialize_if_faithful;

pub(super) fn span_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    // Math notation support (existing functionality)
    if element.attrs.len() == 1
        && let attr = &element.attrs[0]
        && *attr.name.local == *"class"
        && let children = element.node.children.borrow()
        && children.len() == 1
        && let NodeData::Text { contents } = &children[0].data
    {
        if *attr.value == *"math math-inline" {
            return Some(concat_strings!("$", contents.borrow().to_string(), "$").into());
        }
        if *attr.value == *"math math-display" {
            return Some(concat_strings!("$$", contents.borrow().to_string(), "$$").into());
        }
    }

    // CSS style detection for bold/italic formatting
    if let Some(style) = get_attr(element.attrs, "style") {
        let (is_bold, is_italic) = parse_style_formatting(&style);
        
        // If style has no bold/italic formatting (color-only syntax highlighting)
        // fall through to default handling
        if !is_bold && !is_italic {
            let content = handlers.walk_children(element.node, element.is_pre).content;
            return Some(content.trim_matches('\n').into());
        }
        
        // Apply emphasis formatting using proper CommonMark approach
        // per https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis
        let content = handlers.walk_children(element.node, element.is_pre).content;
        if content.is_empty() {
            return None;
        }
        
        // Preserve leading/trailing whitespace outside the emphasis markers
        let (content, leading_ws) = content.strip_leading_whitespace();
        let (content, trailing_ws) = content.strip_trailing_whitespace();
        
        if content.is_empty() {
            return None;
        }
        
        let marker = match (is_bold, is_italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            _ => "",
        };
        
        let result = concat_strings!(
            leading_ws.unwrap_or(""),
            marker,
            content,
            marker,
            trailing_ws.unwrap_or("")
        );
        return Some(result.into());
    }

    // Faithful mode check for spans without special handling
    serialize_if_faithful!(handlers, element, -1);

    // Default: return contents with newlines trimmed
    let content = handlers.walk_children(element.node, element.is_pre).content;
    Some(content.trim_matches('\n').into())
}

/// Get attribute value from element, filtering empty values
fn get_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    attrs.iter()
        .find(|attr| &*attr.name.local == name)
        .map(|attr| attr.value.to_string())
        .filter(|v| !v.trim().is_empty())
}

/// Parse inline CSS style attribute and detect bold/italic formatting.
///
/// Uses Mozilla's cssparser crate for standards-compliant CSS tokenization.
/// This correctly handles edge cases like:
/// - Data URIs with semicolons: `background: url(data:image/png;base64,...)`
/// - Quoted strings with semicolons: `content: "Hello; World"`
/// - CSS functions with complex arguments
///
/// Returns (is_bold, is_italic) tuple based on CSS properties:
/// - font-weight: bold, bolder, or numeric >= 600
/// - font-style: italic or oblique
/// - font: shorthand containing weight/style values
fn parse_style_formatting(style: &str) -> (bool, bool) {
    let mut is_bold = false;
    let mut is_italic = false;
    
    let mut input = ParserInput::new(style);
    let mut parser = Parser::new(&mut input);
    
    // Parse CSS declaration list (property: value pairs separated by semicolons)
    loop {
        // Check if input is exhausted
        if parser.is_exhausted() {
            break;
        }
        
        // Try to parse a property name (must be an identifier)
        let property = match parser.expect_ident() {
            Ok(name) => name.to_string().to_ascii_lowercase(),
            Err(_) => {
                // Skip to next declaration
                skip_to_semicolon_or_end(&mut parser);
                continue;
            }
        };
        
        // Expect colon after property name
        if parser.expect_colon().is_err() {
            skip_to_semicolon_or_end(&mut parser);
            continue;
        }
        
        // Parse value based on property name
        match property.as_str() {
            "font-weight" => {
                is_bold = parse_font_weight_value(&mut parser);
            }
            "font-style" => {
                is_italic = parse_font_style_value(&mut parser);
            }
            "font" => {
                // Font shorthand can contain both weight and style
                let (bold, italic) = parse_font_shorthand_value(&mut parser);
                is_bold = is_bold || bold;
                is_italic = is_italic || italic;
            }
            _ => {
                // Skip unknown property value
                skip_to_semicolon_or_end(&mut parser);
            }
        }
    }
    
    (is_bold, is_italic)
}

/// Parse font-weight value: bold | bolder | lighter | normal | 100-900
fn parse_font_weight_value(parser: &mut Parser) -> bool {
    let mut is_bold = false;
    
    // Consume tokens until semicolon or end
    while let Ok(token) = parser.next() {
        match token {
            Token::Ident(ident) => {
                let lower = ident.to_ascii_lowercase();
                match lower.as_str() {
                    "bold" | "bolder" => {
                        is_bold = true;
                    }
                    "normal" | "lighter" => {
                        is_bold = false;
                    }
                    _ => {}
                }
            }
            Token::Number { int_value: Some(n), .. } => {
                // CSS font-weight: 100-900, with 600+ being bold
                is_bold = *n >= 600;
            }
            Token::Semicolon => break,
            _ => {}
        }
    }
    
    is_bold
}

/// Parse font-style value: italic | oblique | normal
fn parse_font_style_value(parser: &mut Parser) -> bool {
    let mut is_italic = false;
    
    // Consume tokens until semicolon or end
    while let Ok(token) = parser.next() {
        match token {
            Token::Ident(ident) => {
                let lower = ident.to_ascii_lowercase();
                match lower.as_str() {
                    "italic" | "oblique" => {
                        is_italic = true;
                    }
                    "normal" => {
                        is_italic = false;
                    }
                    _ => {}
                }
            }
            Token::Semicolon => break,
            _ => {}
        }
    }
    
    is_italic
}

/// Parse font shorthand: [font-style] [font-variant] [font-weight] font-size [/line-height] font-family
///
/// Per CSS spec, font-style, font-variant, and font-weight may appear in any order
/// before the required font-size value. We scan for weight and style keywords.
///
/// Reference: https://developer.mozilla.org/en-US/docs/Web/CSS/font
fn parse_font_shorthand_value(parser: &mut Parser) -> (bool, bool) {
    let mut is_bold = false;
    let mut is_italic = false;
    
    // Scan all tokens in the value looking for weight/style indicators
    while let Ok(token) = parser.next() {
        match token {
            Token::Ident(ident) => {
                let lower = ident.to_ascii_lowercase();
                match lower.as_str() {
                    // Font-weight keywords
                    "bold" | "bolder" => is_bold = true,
                    "lighter" => is_bold = false,
                    
                    // Font-style keywords
                    "italic" | "oblique" => is_italic = true,
                    
                    // Other values (normal, font-family names, etc.) - ignore
                    _ => {}
                }
            }
            Token::Number { int_value: Some(n), .. } => {
                // Could be font-weight (100-900) or font-size
                // Font-weight values are always multiples of 100 in range 100-900
                let n = *n;
                if (100..=900).contains(&n) && n % 100 == 0 {
                    is_bold = n >= 600;
                }
                // Otherwise it's likely font-size, ignore
            }
            Token::Semicolon => break,
            _ => {}
        }
    }
    
    (is_bold, is_italic)
}

/// Skip tokens until we hit a semicolon or end of input
fn skip_to_semicolon_or_end(parser: &mut Parser) {
    loop {
        match parser.next() {
            Ok(Token::Semicolon) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}
