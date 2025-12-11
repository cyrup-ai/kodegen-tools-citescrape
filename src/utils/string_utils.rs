//! UTF-8-safe string truncation utilities
//!
//! This module provides safe string slicing functions that respect UTF-8 character
//! boundaries, preventing panics when working with multi-byte characters like
//! box-drawing symbols (┌─┐), emoji, and other Unicode characters.

/// Safely truncate a string to a maximum number of CHARACTERS (not bytes).
///
/// This function respects UTF-8 character boundaries and will never panic,
/// even with multi-byte characters like box-drawing symbols or emoji.
///
/// # Arguments
/// * `s` - String slice to truncate
/// * `max_chars` - Maximum number of Unicode characters (not bytes)
///
/// # Returns
/// * String slice containing at most `max_chars` characters, or the full string
///   if it's shorter than `max_chars`
///
/// # Performance
/// * O(n) where n = max_chars (iterates through characters)
/// * Zero allocation - returns slice of original string
///
/// # Examples
/// ```
/// # use kodegen_tools_citescrape::utils::string_utils::safe_truncate_chars;
/// // ASCII characters (1 byte each)
/// assert_eq!(safe_truncate_chars("Hello, World!", 5), "Hello");
///
/// // Multi-byte UTF-8 characters (3 bytes each)
/// let text = "┌───────┐ ┌───────┐ ┌───────┐";
/// assert_eq!(safe_truncate_chars(text, 9), "┌───────┐");
///
/// // Emoji (4 bytes each)
/// assert_eq!(safe_truncate_chars("🎉🎊🎈", 2), "🎉🎊");
///
/// // String shorter than max_chars
/// assert_eq!(safe_truncate_chars("Hi", 100), "Hi");
/// ```
#[inline]
pub fn safe_truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s, // String has fewer than max_chars characters
        Some((byte_idx, _)) => &s[..byte_idx], // Slice at char boundary
    }
}

/// Find a safe byte index for truncation, preferring word boundaries.
///
/// This function finds the byte index of the last word boundary (whitespace or
/// punctuation) within the first `max_chars` characters. If no boundary is found,
/// returns the byte index of the `max_chars`-th character.
///
/// # Arguments
/// * `s` - String slice to analyze
/// * `max_chars` - Maximum number of characters to consider
/// * `boundary_chars` - String containing characters considered word boundaries
///   (e.g., " -,;:" for whitespace and common punctuation)
///
/// # Returns
/// * Byte index of the last word boundary within first `max_chars` characters,
///   or byte index of `max_chars`-th character if no boundary found,
///   or length of string if string is shorter than `max_chars`
///
/// # Examples
/// ```
/// # use kodegen_tools_citescrape::utils::string_utils::safe_truncate_boundary;
/// let text = "Hello, wonderful world of Unicode!";
/// 
/// // Find boundary within first 20 chars
/// let idx = safe_truncate_boundary(text, 20, " ,;:");
/// assert_eq!(&text[..idx], "Hello, wonderful");
///
/// // Works with multi-byte characters
/// let text = "Box drawing: ┌─┐ and more symbols";
/// let idx = safe_truncate_boundary(text, 20, " ");
/// assert_eq!(&text[..idx], "Box drawing: ┌─┐");
/// ```
pub fn safe_truncate_boundary(s: &str, max_chars: usize, boundary_chars: &str) -> usize {
    // Find byte index of max_chars-th character (or end of string)
    let max_byte_idx = s
        .char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len());

    // Search backwards from max_byte_idx for a word boundary
    // rfind on a slice returns position relative to slice start
    s[..max_byte_idx]
        .rfind(|c: char| c.is_whitespace() || boundary_chars.contains(c))
        .unwrap_or(max_byte_idx)
}
