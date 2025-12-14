//! Footnote marker removal using regex

use regex::Regex;
use std::sync::LazyLock;

/// Regex pattern for footnote markers (compiled once, cached globally)
static FOOTNOTE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\^[^\]]+\]")
        .expect("FOOTNOTE_REGEX: hardcoded regex is valid")
});

/// Remove footnote markers `[^1]` style
pub(crate) fn remove_footnote_markers(text: String) -> String {
    FOOTNOTE_REGEX.replace_all(&text, "").into_owned()
}
