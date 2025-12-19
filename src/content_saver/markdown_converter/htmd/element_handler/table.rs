use super::element_util::serialize_element;
use super::super::Element;
use super::{HandlerResult, Handlers};
use super::super::node_util::get_node_tag_name;
use super::super::options::TranslationMode;
use crate::serialize_if_faithful;
use super::super::text_util::{TrimDocumentWhitespace, concat_strings};
use markup5ever_rcdom::NodeData;
use std::rc::Rc;

/// Handler for table elements.
///
/// Converts HTML tables to Markdown tables using the pipe syntax:
/// ```text
/// | Header1 | Header2 |
/// | ------- | ------- |
/// | Cell1   | Cell2   |
/// ```
pub(crate) fn table_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    serialize_if_faithful!(handlers, element, 0);
    // All child table elements must be markdown translated to markdown
    // translate the table in faithful mode.
    // We track markdown translation status manually because we iterate children
    let mut all_children_translated = true;

    // We only need content if we fail to parse the table structure
    // But for now, let's just grab it lazily if needed?
    // Actually, the original code used content.trim().is_empty() check.
    // Let's try to parse first.

    // Extract table rows
    let mut captions: Vec<String> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut has_thead = false;

    // Extract rows and headers from the table structure
    if let NodeData::Element { .. } = &element.node.data {
        for child in element.node.children.borrow().iter() {
            if let NodeData::Element { name, .. } = &child.data {
                let tag_name = name.local.as_ref();

                match tag_name {
                    "caption" => {
                        if let Some(res) = handlers.handle(child) {
                            captions.push(res.content.trim_document_whitespace().to_string());
                        }
                    }
                    "thead" => {
                        let tr = child
                            .children
                            .borrow()
                            .iter()
                            .find(|it| get_node_tag_name(it).is_some_and(|tag| tag == "tr"))
                            .cloned();

                        let row_node = match tr {
                            Some(tr) => tr,
                            None => child.clone(),
                        };

                        has_thead = true;
                        let (cells, translated) = extract_row_cells(handlers, &row_node, "th");
                        headers = cells;
                        all_children_translated &= translated;
                        if headers.is_empty() {
                            let (cells, translated) = extract_row_cells(handlers, &row_node, "td");
                            headers = cells;
                            all_children_translated &= translated;
                        }
                    }
                    "tbody" | "tfoot" => {
                        for row_node in child.children.borrow().iter() {
                            if let NodeData::Element { name, .. } = &row_node.data
                                && name.local.as_ref() == "tr"
                            {
                                // If no thead is found, use the first th row as header
                                if !has_thead && headers.is_empty() {
                                    let (cells, translated) =
                                        extract_row_cells(handlers, row_node, "th");
                                    headers = cells;
                                    all_children_translated &= translated;
                                    has_thead = !headers.is_empty();

                                    if has_thead {
                                        continue;
                                    }
                                }

                                let (row_cells, translated) =
                                    extract_row_cells(handlers, row_node, "td");
                                all_children_translated &= translated;
                                if !row_cells.is_empty() {
                                    rows.push(row_cells);
                                }
                            }
                        }
                    }
                    "tr" => {
                        // If no thead is found, use the first row as headers
                        if !has_thead && headers.is_empty() {
                            let (cells, translated) = extract_row_cells(handlers, child, "th");
                            headers = cells;
                            all_children_translated &= translated;
                            if headers.is_empty() {
                                let (cells, translated) = extract_row_cells(handlers, child, "td");
                                if !cells.is_empty() {
                                    headers = cells;
                                    all_children_translated &= translated;
                                }
                            }
                            has_thead = !headers.is_empty();
                        } else {
                            let (row_cells, translated) = extract_row_cells(handlers, child, "td");
                            all_children_translated &= translated;
                            if !row_cells.is_empty() {
                                rows.push(row_cells);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if handlers.options().translation_mode == TranslationMode::Faithful && !all_children_translated
    {
        return Some(HandlerResult {
            content: serialize_element(handlers, &element),
            markdown_translated: false,
        });
    }

    // If we didn't find any rows or cells, just return the content as-is
    if rows.is_empty() && headers.is_empty() {
        let content = handlers.walk_children(element.node, element.is_pre).content;
        let content = content.trim_matches('\n');
        if content.is_empty() {
            return None;
        }
        return Some(concat_strings!("\n\n", content, "\n\n").into());
    }

    // Determine the number of columns by finding the max column count
    let num_columns = if headers.is_empty() {
        rows.iter().map(|row| row.len()).max().unwrap_or(0)
    } else {
        headers.len()
    };

    if num_columns == 0 {
        let content = handlers.walk_children(element.node, element.is_pre).content;
        let content = content.trim_matches('\n');
        if content.is_empty() {
            return None;
        }
        return Some(concat_strings!("\n\n", content, "\n\n").into());
    }

    // Build the Markdown table
    let mut table_md = String::from("\n\n");

    for caption in captions {
        table_md.push_str(&format!("{caption}\n"));
    }

    let col_widths = compute_column_widths(&headers, &rows, num_columns);

    if !headers.is_empty() {
        table_md.push_str(&format_row_padded(&headers, num_columns, &col_widths));
        table_md.push_str(&format_separator_padded(num_columns, &col_widths));
    }
    for row in rows {
        table_md.push_str(&format_row_padded(&row, num_columns, &col_widths));
    }

    table_md.push('\n');
    Some(table_md.into())
}

/// Extract cells from a row node
fn extract_row_cells(
    handlers: &dyn Handlers,
    row_node: &Rc<markup5ever_rcdom::Node>,
    cell_tag: &str,
) -> (Vec<String>, bool) {
    let mut cells = Vec::new();
    let mut all_translated = true;

    for cell_node in row_node.children.borrow().iter() {
        if let NodeData::Element { name, .. } = &cell_node.data
            && name.local.as_ref() == cell_tag
        {
            let Some(res) = handlers.handle(cell_node) else {
                continue;
            };
            if !res.markdown_translated {
                all_translated = false;
            }
            let cell_content = res.content.trim_document_whitespace().to_string();
            cells.push(cell_content);
        }
    }

    (cells, all_translated)
}

/// Normalize cell content for Markdown table representation
fn normalize_cell_content(content: &str) -> String {
    let content = content
        .replace('\n', " ")
        .replace('\r', "")
        .replace('|', "&#124;");
    content.trim_document_whitespace().to_string()
}

fn format_row_padded(row: &[String], num_columns: usize, col_widths: &[usize]) -> String {
    let mut line = String::from("|");
    for (i, col_width) in col_widths.iter().enumerate().take(num_columns) {
        let cell = row
            .get(i)
            .map(|s| normalize_cell_content(s))
            .unwrap_or_default();
        // Enforce minimum width of 1 for visual consistency with separator
        let effective_width = std::cmp::max(*col_width, 1);
        let pad = effective_width.saturating_sub(cell.chars().count());
        line.push_str(&concat_strings!(" ", cell, " ".repeat(pad), " |"));
    }
    line.push('\n');
    line
}

fn format_separator_padded(num_columns: usize, col_widths: &[usize]) -> String {
    let mut line = String::from("|");
    for (_, col_width) in col_widths.iter().enumerate().take(num_columns) {
        // GFM spec requires minimum 1 dash per delimiter cell
        let effective_width = std::cmp::max(*col_width, 1);
        line.push_str(&concat_strings!(" ", "-".repeat(effective_width), " |"));
    }
    line.push('\n');
    line
}

fn compute_column_widths(
    headers: &[String],
    rows: &[Vec<String>],
    num_columns: usize,
) -> Vec<usize> {
    let mut widths = vec![0; num_columns];
    
    // Normalize headers before measuring width
    for (i, header) in headers.iter().enumerate().take(num_columns) {
        let normalized = normalize_cell_content(header);
        widths[i] = normalized.chars().count();
    }
    
    // Normalize row cells before measuring width
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(num_columns) {
            let normalized = normalize_cell_content(cell);
            let len = normalized.chars().count();
            if len > widths[i] {
                widths[i] = len;
            }
        }
    }
    widths
}
