//! Visible Markdown text with reversible byte mapping for selection and IME.
use super::*;
#[path = "markdown_inline.rs"]
mod inline;
#[derive(Clone, Copy, Default, PartialEq)]
struct Decoration {
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    link: bool,
}
#[derive(Clone)]
pub(super) struct MarkdownLine {
    pub text: String,
    map: Vec<(usize, usize)>,
    runs: Vec<(usize, Decoration)>,
}
impl MarkdownLine {
    pub fn parse(source: &str) -> Self {
        Self::parse_context(source, false)
    }
    pub fn parse_context(source: &str, code: bool) -> Self {
        let mut line = Self {
            text: String::new(),
            map: vec![(0, 0)],
            runs: Vec::new(),
        };
        let mut decoration = Decoration::default();
        if source.trim_start().starts_with("```") {
            line.skip(source, 0, source.len());
            return line;
        }
        if code && !source.trim_start().starts_with("```") {
            decoration.code = true;
            line.text = source.into();
            line.map = source
                .char_indices()
                .map(|(i, _)| (i, i))
                .chain(std::iter::once((source.len(), source.len())))
                .collect();
            line.runs = vec![(source.len(), decoration)];
            return line;
        }
        let mut index = 0;
        let mut heading_bold = false;
        let heading = source.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&heading) && source.as_bytes().get(heading) == Some(&b' ') {
            index = heading + 1;
            heading_bold = true;
            line.skip(source, 0, index);
        } else if source.starts_with("- ") || source.starts_with("* ") {
            line.text.push_str("• ");
            line.runs.push((line.text.len(), decoration));
            index = 2;
            line.map.push((2, line.text.len()));
        } else if source.starts_with("> ") {
            index = 2;
            decoration.italic = true;
            line.skip(source, 0, index);
        } else if source.starts_with("```") {
            index = source.len();
            line.skip(source, 0, index);
        }
        // Ordered list markers remain visible and retain source positions.
        if index == 0 {
            let digits = source.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && source[digits..].starts_with(". ") {
                index = digits + 2;
                line.text.push_str(&source[..index]);
                line.runs.push((index, decoration));
                line.map.extend((0..=index).map(|offset| (offset, offset)));
            }
        }
        decoration.bold |= heading_bold;
        inline::parse(&mut line, source, index, decoration);

        line
    }
    fn skip(&mut self, source: &str, start: usize, end: usize) {
        for (index, _) in source[start..end].char_indices() {
            self.map.push((start + index, self.text.len()));
        }
        self.map.push((end, self.text.len()));
    }
    pub fn display_offset(&self, source: usize) -> usize {
        self.map[self
            .map
            .partition_point(|(raw, _)| *raw <= source)
            .saturating_sub(1)]
        .1
    }
    pub fn source_offset(&self, display: usize) -> usize {
        self.map[self
            .map
            .partition_point(|(_, visible)| *visible <= display)
            .saturating_sub(1)]
        .0
    }
    pub fn text_runs(&self, style: &gpui::TextStyle, theme: &Theme) -> Vec<gpui::TextRun> {
        self.runs
            .iter()
            .map(|(len, decoration)| {
                let mut font = style.font();
                if decoration.bold {
                    font.weight = gpui::FontWeight::BOLD;
                }
                if decoration.italic {
                    font.style = gpui::FontStyle::Italic;
                }
                if decoration.code {
                    font.family = "Menlo".into();
                }
                gpui::TextRun {
                    len: *len,
                    font,
                    color: if decoration.code || decoration.link {
                        theme.accent
                    } else {
                        theme.text
                    },
                    background_color: decoration.code.then_some(theme.button),
                    underline: decoration.link.then_some(gpui::UnderlineStyle {
                        thickness: px(1.),
                        color: Some(theme.accent),
                        wavy: false,
                    }),
                    strikethrough: decoration.strike.then_some(gpui::StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(theme.text),
                    }),
                }
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_text_selection_maps_to_original_markdown() {
        let line = MarkdownLine::parse("## **中文** and `x`");
        assert_eq!(line.text, "中文 and x");
        assert_eq!(line.source_offset(0), 5);
        assert_eq!(line.display_offset(8), 3);
        for (raw, visible) in &line.map {
            assert!("## **中文** and `x`".is_char_boundary(*raw));
            assert!(line.text.is_char_boundary(*visible));
        }
    }
    #[test]
    fn unfinished_markup_remains_editable() {
        assert_eq!(MarkdownLine::parse("**unfinished").text, "**unfinished");
    }
}
