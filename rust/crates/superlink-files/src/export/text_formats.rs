//! Markdown and HTML rendering shared by the text exports.

/// Escapes text the way Go's html.EscapeString does.
pub(crate) fn html_escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '\'' => output.push_str("&#39;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&#34;"),
            other => output.push(other),
        }
    }
    output
}

/// Escapes one Markdown table cell without breaking the table structure.
pub(crate) fn markdown_cell(value: &str) -> String {
    let escaped = html_escape(value);
    escaped
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace('\n', "<br>")
}

/// Renders one Markdown table row.
pub(crate) fn markdown_row(values: &[String]) -> String {
    let cells: Vec<String> = values.iter().map(|value| markdown_cell(value)).collect();
    format!("| {} |\n", cells.join(" | "))
}

/// Renders the Markdown table header and its separator.
pub(crate) fn markdown_header(columns: &[String]) -> String {
    let cells: Vec<String> = columns.iter().map(|value| markdown_cell(value)).collect();
    let mut separator = String::new();
    for index in 0..columns.len() {
        if index > 0 {
            separator.push_str(" | ");
        }
        separator.push_str("---");
    }
    format!("| {} |\n| {} |\n", cells.join(" | "), separator)
}

/// Fixed HTML preamble; the table body is streamed afterwards.
pub(crate) const HTML_HEADER: &str = "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>Query result</title><style>body{font:14px system-ui}table{border-collapse:collapse}td,th{border:1px solid #d1d5db;padding:8px;text-align:left}.null{color:#9ca3af}</style></head><body><table><thead><tr>";

/// Closing markup for the HTML export.
pub(crate) const HTML_FOOTER: &str = "</tbody></table></body></html>\n";
