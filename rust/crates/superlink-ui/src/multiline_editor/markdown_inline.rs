//! CommonMark inline styles with offsets retained for native editing and IME.
use super::{Decoration, MarkdownLine};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub(super) fn parse(line: &mut MarkdownLine, source: &str, base: usize, initial: Decoration) {
    let raw = &source[base..];
    let leading = raw.len() - raw.trim_start_matches([' ', '\t']).len();
    let end = source
        .trim_end_matches([' ', '\t'])
        .len()
        .max(base + leading);
    let mut consumed = base;
    if leading > 0 {
        append(
            line,
            source,
            &mut consumed,
            base..base + leading,
            &source[base..base + leading],
            initial,
        );
    }
    let base = base + leading;
    let body = &source[base..end];
    let mut current = initial;
    let mut stack = Vec::new();
    for (event, range) in Parser::new_ext(body, Options::ENABLE_STRIKETHROUGH).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if matches!(
                    tag,
                    Tag::Strong | Tag::Emphasis | Tag::Strikethrough | Tag::Link { .. }
                ) {
                    stack.push(current);
                    match tag {
                        Tag::Strong => current.bold = true,
                        Tag::Emphasis => current.italic = true,
                        Tag::Strikethrough => current.strike = true,
                        Tag::Link { .. } => current.link = true,
                        _ => {}
                    }
                }
            }
            Event::End(
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link,
            ) => {
                current = stack.pop().unwrap_or(initial);
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                append(
                    line,
                    source,
                    &mut consumed,
                    base + range.start..base + range.end,
                    &text,
                    current,
                );
            }
            Event::Code(text) => {
                let mut style = current;
                style.code = true;
                let mut range = base + range.start..base + range.end;
                if let Some(offset) = source[range.clone()].find(text.as_ref()) {
                    range = range.start + offset..range.start + offset + text.len();
                }
                append(line, source, &mut consumed, range, &text, style);
            }
            _ => {}
        }
    }
    if consumed == base && !body.is_empty() {
        // Keep unsupported standalone block syntax editable.
        append(line, source, &mut consumed, base..end, body, initial);
    }
    line.skip(source, consumed, end);
    consumed = end;
    if end < source.len() {
        append(
            line,
            source,
            &mut consumed,
            end..source.len(),
            &source[end..],
            initial,
        );
    }
}

fn append(
    line: &mut MarkdownLine,
    source: &str,
    consumed: &mut usize,
    range: Range<usize>,
    text: &str,
    style: Decoration,
) {
    if range.start < *consumed {
        return;
    }
    line.skip(source, *consumed, range.start);
    let raw = &source[range.clone()];
    if raw == text {
        for (offset, ch) in raw.char_indices() {
            line.map.push((range.start + offset, line.text.len()));
            line.text.push(ch);
            line.map
                .push((range.start + offset + ch.len_utf8(), line.text.len()));
        }
    } else {
        // Escapes/entities can have different source/display lengths. Keep every
        // mapped position on a UTF-8 boundary and preserve both span endpoints.
        let raw_offsets = raw
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(raw.len()))
            .collect::<Vec<_>>();
        let visible = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>();
        let start = line.text.len();
        for (i, offset) in raw_offsets.iter().enumerate() {
            let mapped = i * (visible.len() - 1) / (raw_offsets.len() - 1).max(1);
            line.map
                .push((range.start + offset, start + visible[mapped]));
        }
        line.text.push_str(text);
    }
    match line.runs.last_mut() {
        Some((len, previous)) if *previous == style => *len += text.len(),
        _ => line.runs.push((text.len(), style)),
    }
    *consumed = range.end;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_emphasis_code_and_escaped_delimiters_preserve_offsets() {
        let source = "***中文*** and `**literal**` and \\*escaped\\*";
        let line = MarkdownLine::parse(source);
        assert_eq!(line.text, "中文 and **literal** and *escaped*");
        assert!(
            line.runs
                .iter()
                .any(|(_, style)| style.bold && style.italic)
        );
        assert!(line.runs.iter().any(|(_, style)| style.code && !style.bold));
        for (raw, display) in &line.map {
            assert!(source.is_char_boundary(*raw));
            assert!(line.text.is_char_boundary(*display));
        }
        assert!(
            line.map
                .windows(2)
                .all(|w| w[0].0 <= w[1].0 && w[0].1 <= w[1].1)
        );
    }
    #[test]
    fn links_and_underscores_keep_the_label_editable() {
        let line = MarkdownLine::parse("__strong__ [文档](https://example.invalid)");
        assert_eq!(line.text, "strong 文档");
        assert!(line.runs.iter().any(|(_, s)| s.link));
    }
    #[test]
    fn preserves_outer_spaces_and_entity_utf8_positions() {
        for source in ["  **中文**  ", "    ", "&amp;😀", "\t__粗体__ "] {
            let line = MarkdownLine::parse(source);
            assert!(
                line.map
                    .windows(2)
                    .all(|w| w[0].0 <= w[1].0 && w[0].1 <= w[1].1)
            );
            for (raw, display) in &line.map {
                assert!(source.is_char_boundary(*raw));
                assert!(line.text.is_char_boundary(*display));
            }
        }
        assert_eq!(MarkdownLine::parse("  **中文**  ").text, "  中文  ");
    }
}
