//! Source-based toolbar state and reversible block formatting.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Copy, Default)]
pub(crate) struct FormattingState {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    fenced: bool,
    blocks: u8,
    enabled: bool,
}
impl FormattingState {
    pub fn active(self, prefix: &str) -> bool {
        match prefix {
            "**" => self.bold,
            "*" => self.italic,
            "~~" => self.strike,
            "`" => self.code,
            "# " => self.blocks & 1 != 0,
            "## " => self.blocks & 2 != 0,
            "### " => self.blocks & 4 != 0,
            "- " => self.blocks & 8 != 0,
            "1. " => self.blocks & 16 != 0,
            "> " => self.blocks & 32 != 0,
            p if p.contains("```") => self.fenced,
            _ => false,
        }
    }
    pub fn disabled(self, prefix: &str, suffix: &str) -> bool {
        !self.enabled || self.fenced && !suffix.is_empty() && !prefix.contains("```")
    }
}
fn paragraph(source: &str, range: &Range<usize>) -> Range<usize> {
    let start = source[..range.start]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = source[range.end..]
        .find('\n')
        .map(|i| range.end + i)
        .unwrap_or(source.len());
    start..end
}
fn ordered_prefix(line: &str) -> usize {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        digits + 2
    } else {
        0
    }
}
pub(crate) fn state(source: &str, selected: Range<usize>, enabled: bool) -> FormattingState {
    let range = paragraph(source, &selected);
    let text = &source[range.clone()];
    let fenced = source[..selected.start]
        .lines()
        .filter(|l| l.trim_start().starts_with("```"))
        .count()
        % 2
        == 1;
    let mut state = FormattingState {
        fenced,
        enabled,
        ..Default::default()
    };
    for (index, prefix) in ["# ", "## ", "### ", "- ", "1. ", "> "].iter().enumerate() {
        if text.split('\n').all(|line| {
            if *prefix == "1. " {
                ordered_prefix(line) > 0
            } else {
                line.starts_with(prefix)
            }
        }) {
            state.blocks |= 1 << index;
        }
    }
    let mut current = (false, false, false, false);
    let mut stack = Vec::new();
    let mut matching = Vec::new();
    let mut from = selected.start - range.start;
    let to = selected.end - range.start;
    if from == to && from == text.len() && from > 0 {
        from = text[..from].char_indices().next_back().unwrap().0;
    }
    for (event, span) in Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH).into_offset_iter() {
        match event {
            Event::Start(Tag::Strong | Tag::Emphasis | Tag::Strikethrough) => {
                stack.push(current);
                match event {
                    Event::Start(Tag::Strong) => current.0 = true,
                    Event::Start(Tag::Emphasis) => current.1 = true,
                    _ => current.2 = true,
                }
            }
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                current = stack.pop().unwrap_or_default()
            }
            Event::Text(_) | Event::Code(_)
                if span.start <= from && from < span.end || from < span.end && span.start < to =>
            {
                let mut style = current;
                style.3 = matches!(event, Event::Code(_));
                matching.push(style);
            }
            _ => {}
        }
    }
    if !matching.is_empty() {
        state.bold = matching.iter().all(|s| s.0);
        state.italic = matching.iter().all(|s| s.1);
        state.strike = matching.iter().all(|s| s.2);
        state.code = matching.iter().all(|s| s.3);
    }
    state
}

pub(crate) fn toggle_block(text: &str, prefix: &str) -> String {
    let remove = text.split('\n').all(|line| {
        if prefix == "1. " {
            ordered_prefix(line) > 0
        } else {
            line.starts_with(prefix)
        }
    });
    text.split('\n')
        .enumerate()
        .map(|(index, line)| {
            if remove {
                return if prefix == "1. " {
                    line[ordered_prefix(line)..].into()
                } else {
                    line.strip_prefix(prefix).unwrap_or(line).into()
                };
            }
            let mut line = line;
            if prefix.starts_with('#') {
                let hashes = line.bytes().take_while(|b| *b == b'#').count();
                if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
                    line = &line[hashes + 1..];
                }
            }
            if prefix == "1. " {
                format!("{}. {line}", index + 1)
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
    pub selected: Range<usize>,
}
pub(crate) fn toggle_fence(source: &str, selected: Range<usize>) -> Edit {
    let mut start = 0;
    let mut opening = None;
    for line in source.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            if let Some((from, body)) = opening.take() {
                if selected.start >= body && selected.end <= start {
                    let body_text = &source[body..start];
                    let mut replacement = body_text
                        .strip_suffix('\n')
                        .unwrap_or(body_text)
                        .to_string();
                    if line.ends_with('\n') {
                        replacement.push('\n');
                    }
                    let cursor = from + selected.start.saturating_sub(body).min(replacement.len());
                    let end = from + selected.end.saturating_sub(body).min(replacement.len());
                    return Edit {
                        range: from..start + line.len(),
                        replacement,
                        selected: cursor..end,
                    };
                }
            } else {
                opening = Some((start, start + line.len()));
            }
        }
        start += line.len();
    }
    let range = paragraph(source, &selected);
    let replacement = format!("```\n{}\n```", &source[range.clone()]);
    Edit {
        selected: selected.start + 4..selected.end + 4,
        range,
        replacement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toolbar_tracks_nested_styles_and_disables_inline_actions_in_fences() {
        let text = "***中文***";
        let s = state(text, 3..6, true);
        assert!(s.active("**") && s.active("*"));
        let s = state("```sql\n文字\n```", 7..7, true);
        assert!(s.active("\n```\n") && s.disabled("**", "**"));
    }
    #[test]
    fn headings_switch_level_and_numbering_round_trips() {
        assert_eq!(toggle_block("## 中文", "# "), "# 中文");
        let numbered = toggle_block("甲\n乙", "1. ");
        assert_eq!(numbered, "1. 甲\n2. 乙");
        assert_eq!(toggle_block(&numbered, "1. "), "甲\n乙");
    }
    #[test]
    fn fenced_block_toggle_keeps_source_selection() {
        let edit = toggle_fence("中文", 0..6);
        assert_eq!(edit.replacement, "```\n中文\n```");
        assert_eq!(edit.selected, 4..10);
        let undo = toggle_fence(&edit.replacement, edit.selected);
        assert_eq!(undo.replacement, "中文");
        assert_eq!(undo.selected, 0..6);
    }
    #[test]
    fn removing_a_fence_preserves_the_following_paragraph() {
        let source = "```sql\n中\n```\nafter";
        let edit = toggle_fence(source, 7..10);
        assert_eq!(
            format!(
                "{}{}{}",
                &source[..edit.range.start],
                edit.replacement,
                &source[edit.range.end..]
            ),
            "中\nafter"
        );
    }
}
