//! Display helpers for the multi-line SQL editor: line index math and the
//! per-line text runs that color keywords, strings, numbers and comments.
//!
//! Tokenization never rewrites the buffer, so caret math keeps using the exact
//! source text.

use std::ops::Range;

use gpui::{Hsla, TextRun, TextStyle, UnderlineStyle, px, rgb};
use superlink_sql::lexer::{Token, TokenKind};

use crate::theme::Theme;

/// Byte offsets where each line starts; an empty buffer still has one line.
pub(crate) fn line_starts(content: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

pub(crate) fn line_range(content: &str, starts: &[usize], index: usize) -> Range<usize> {
    let start = starts[index];
    let end = if index + 1 < starts.len() {
        starts[index + 1] - 1
    } else {
        content.len()
    };
    start..end
}

pub(crate) fn line_of(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(index) => index,
        Err(index) => index.saturating_sub(1),
    }
}

/// Dark palettes use a light foreground; light palettes a dark one.
fn is_dark(theme: &Theme) -> bool {
    theme.text.l > 0.5
}

fn token_color(kind: TokenKind, dark: bool, theme: &Theme) -> Hsla {
    match (kind, dark) {
        (TokenKind::Keyword, true) => rgb(0xc586c0).into(),
        (TokenKind::Keyword, false) => rgb(0x7923df).into(),
        (TokenKind::String, true) => rgb(0xce9178).into(),
        (TokenKind::String, false) => rgb(0xa31515).into(),
        (TokenKind::Number, true) => rgb(0xb5cea8).into(),
        (TokenKind::Number, false) => rgb(0x098658).into(),
        (TokenKind::Comment, true) => rgb(0x6a9955).into(),
        (TokenKind::Comment, false) => rgb(0x008000).into(),
        (TokenKind::Plain, _) => theme.text,
    }
}

pub(crate) fn text_run(len: usize, style: &TextStyle, color: Hsla, underline: bool) -> TextRun {
    TextRun {
        len,
        font: style.font(),
        color,
        background_color: None,
        underline: underline.then_some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.0),
            wavy: false,
        }),
        strikethrough: None,
    }
}

/// Runs covering one line exactly, split again at the IME marked range.
pub(crate) fn line_runs(
    tokens: &[Token],
    line: Range<usize>,
    style: &TextStyle,
    theme: &Theme,
    marked: Option<&Range<usize>>,
) -> Vec<TextRun> {
    let dark = is_dark(theme);
    let mut runs: Vec<TextRun> = Vec::new();
    let mut position = 0usize;
    for token in tokens {
        let start = position;
        let end = start + token.text.len();
        position = end;
        if end <= line.start {
            continue;
        }
        if start >= line.end {
            break;
        }
        let from = start.max(line.start);
        let to = end.min(line.end);
        if to <= from {
            continue;
        }
        let color = token_color(token.kind, dark, theme);
        match marked {
            Some(marked) if marked.end.min(to) > marked.start.max(from) => {
                let marked_from = marked.start.max(from);
                let marked_to = marked.end.min(to);
                if marked_from > from {
                    runs.push(text_run(marked_from - from, style, color, false));
                }
                runs.push(text_run(marked_to - marked_from, style, color, true));
                if to > marked_to {
                    runs.push(text_run(to - marked_to, style, color, false));
                }
            }
            _ => runs.push(text_run(to - from, style, color, false)),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FontStyle, FontWeight, TextAlign, px, rgba};
    use superlink_sql::lexer::tokenize;

    fn style() -> TextStyle {
        TextStyle {
            color: rgba(0x000000ff).into(),
            font_family: "PingFang SC".into(),
            font_features: Default::default(),
            font_fallbacks: None,
            font_size: px(13.0).into(),
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            line_height: px(20.0).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
            white_space: Default::default(),
            text_align: TextAlign::Left,
            line_clamp: None,
            text_overflow: Default::default(),
        }
    }

    #[test]
    fn empty_buffer_still_has_one_line() {
        assert_eq!(line_starts(""), vec![0]);
        assert_eq!(
            line_starts(
                "a
b
"
            ),
            vec![0, 2, 4]
        );
        assert_eq!(
            line_starts(
                "a
b"
            ),
            vec![0, 2]
        );
    }

    #[test]
    fn line_ranges_exclude_the_newline() {
        let content = "select 1;
from t;
";
        let starts = line_starts(content);
        assert_eq!(&content[line_range(content, &starts, 0)], "select 1;");
        assert_eq!(&content[line_range(content, &starts, 1)], "from t;");
        assert_eq!(&content[line_range(content, &starts, 2)], "");
    }

    #[test]
    fn offsets_map_to_the_line_they_start() {
        let starts = line_starts(
            "aa
bb
cc",
        );
        assert_eq!(line_of(&starts, 0), 0);
        assert_eq!(line_of(&starts, 2), 0);
        assert_eq!(line_of(&starts, 3), 1);
        assert_eq!(line_of(&starts, 8), 2);
    }

    #[test]
    fn runs_cover_the_whole_line_and_split_at_the_mark() {
        let content = "select 'x' from t";
        let tokens = tokenize(content);
        let theme = Theme::light();
        let style = style();
        let runs = line_runs(&tokens, 0..content.len(), &style, &theme, None);
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), content.len());
        assert_eq!(runs.len(), 7);

        let marked = 7..10;
        let runs = line_runs(&tokens, 0..content.len(), &style, &theme, Some(&marked));
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), content.len());
        assert_eq!(
            runs.iter()
                .filter(|run| run.underline.is_some())
                .map(|run| run.len)
                .sum::<usize>(),
            marked.len()
        );
    }

    #[test]
    fn keyword_uses_the_fyne_palette() {
        let theme = Theme::light();
        assert_ne!(
            token_color(TokenKind::Keyword, false, &theme),
            token_color(TokenKind::Plain, false, &theme)
        );
        assert!(!is_dark(&Theme::light()));
        assert!(is_dark(&Theme::dark()));
    }
}
