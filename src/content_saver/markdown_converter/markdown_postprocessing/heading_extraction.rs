//! Heading extraction and normalization utilities.

/// Pre-built heading prefixes to avoid repeated string allocations
pub const HEADING_PREFIXES: [&str; 6] = ["# ", "## ", "### ", "#### ", "##### ", "###### "];

/// Extract heading level and content from a markdown line
///
/// Removes optional closing hashes from ATX headings per `CommonMark` spec.
/// For example: `## Title ##` becomes `## Title`
#[must_use]
pub fn extract_heading_level(line: &str) -> Option<(usize, &str)> {
    // Match markdown headings (# to ######)
    if line.starts_with('#') {
        let level = line.chars().take_while(|&c| c == '#').count();
        if level > 0 && level <= 6 {
            let content = line[level..].trim_start();

            // Remove optional closing hashes (must be preceded by whitespace per CommonMark)
            // Strategy: scan from right, skip hashes, then skip whitespace
            // If we skipped both, we have a valid closing sequence

            let bytes = content.as_bytes();
            let mut end = bytes.len();
            let mut hash_start = end;

            // Skip trailing hashes
            while hash_start > 0 && bytes[hash_start - 1] == b'#' {
                hash_start -= 1;
            }

            // If we found trailing hashes, check for preceding whitespace
            if hash_start < end {
                let mut ws_start = hash_start;
                // Skip whitespace before the hashes
                while ws_start > 0 && bytes[ws_start - 1].is_ascii_whitespace() {
                    ws_start -= 1;
                }

                // If there was whitespace between content and hashes, remove both
                // OR if all of content is hashes (hash_start == 0), return empty
                if ws_start < hash_start || hash_start == 0 {
                    end = ws_start;
                }
            }

            return Some((level, &content[..end]));
        }
    }
    None
}

/// Normalize heading level to ensure it's within valid range (1-6)
#[must_use]
pub fn normalize_heading_level(level: usize) -> usize {
    // Ensure headings are in the range 1-6
    level.clamp(1, 6)
}
