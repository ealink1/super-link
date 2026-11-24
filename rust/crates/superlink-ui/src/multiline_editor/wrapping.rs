//! Cached visual rows for prose. Byte mappings remain anchored to source Markdown.
use super::*;
#[derive(Clone)]
pub(super) struct Row {
    pub range: Range<usize>,
    pub display: Range<usize>,
    pub mapping: Option<std::sync::Arc<markdown::MarkdownLine>>,
    pub scale: f32,
}
impl Row {
    pub fn offset(&self, raw: usize) -> usize {
        self.mapping
            .as_ref()
            .map(|m| m.display_offset(raw.saturating_sub(self.range.start)))
            .unwrap_or(raw.saturating_sub(self.range.start))
            .saturating_sub(self.display.start)
            .min(self.display.len())
    }
    pub fn source(&self, display: usize) -> usize {
        self.range.start
            + self
                .mapping
                .as_ref()
                .map(|m| m.source_offset(self.display.start + display))
                .unwrap_or(self.display.start + display)
    }
    pub fn contains(&self, raw: usize) -> bool {
        if raw < self.range.start || raw > self.range.end {
            return false;
        }
        let display = self
            .mapping
            .as_ref()
            .map(|m| m.display_offset(raw - self.range.start))
            .unwrap_or(raw - self.range.start);
        display >= self.display.start
            && (display < self.display.end
                || display == self.display.end
                    && (raw == self.range.end || self.display.is_empty()))
    }
}
pub(super) fn max_scroll(rows: &[Row], viewport: usize) -> usize {
    let mut cost = 0.;
    let count = rows
        .iter()
        .rev()
        .take_while(|row| {
            cost += row.scale;
            cost <= viewport.max(1) as f32
        })
        .count()
        .max(1);
    rows.len().saturating_sub(count)
}
pub(super) fn vertical(editor: &SqlTextEditor, delta: isize) -> Option<usize> {
    let index = editor
        .wrapped_rows
        .iter()
        .position(|r| r.contains(editor.cursor_offset()))?;
    let current = &editor.wrapped_rows[index];
    let target = (index as isize + delta)
        .clamp(0, editor.wrapped_rows.len().saturating_sub(1) as isize) as usize;
    let target = &editor.wrapped_rows[target];
    let text = current
        .mapping
        .as_ref()
        .map(|m| m.text.as_str())
        .unwrap_or(&editor.content[current.range.clone()]);
    let column = text
        [current.display.start..current.display.start + current.offset(editor.cursor_offset())]
        .chars()
        .count();
    let target_text = target
        .mapping
        .as_ref()
        .map(|m| m.text.as_str())
        .unwrap_or(&editor.content[target.range.clone()]);
    let byte = char_to_byte(&target_text[target.display.clone()], column);
    Some(target.source(byte).min(editor.content.len()))
}
fn runs_slice(runs: &[gpui::TextRun], range: Range<usize>) -> Vec<gpui::TextRun> {
    let mut offset = 0;
    let mut selected = Vec::new();
    for run in runs {
        let start = offset;
        offset += run.len;
        let len = offset.min(range.end).saturating_sub(start.max(range.start));
        if len > 0 {
            let mut run = run.clone();
            run.len = len;
            selected.push(run);
        }
    }
    selected
}
pub(super) fn prepaint(
    entity: &Entity<SqlTextEditor>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> view::Prepaint {
    let editor = entity.read(cx);
    let content = editor.content.clone();
    let rich = editor.rich_markdown;
    let theme = editor.theme;
    let selected = editor.selected_range.clone();
    let caret = editor.cursor_offset();
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let height = window.line_height();
    let width = (bounds.size.width - px(PAD_X * 2.)).max(px(32.));
    let key = (
        editor.source_revision,
        f32::from(width).to_bits(),
        f32::from(font_size).to_bits(),
        rich,
    );
    let mut rows = editor.wrapped_rows.clone();
    if editor.wrap_key != Some(key) {
        let mut built = Vec::new();
        let mut code = false;
        for (logical, _) in editor.cached_starts.iter().enumerate() {
            let range = line_range(&content, &editor.cached_starts, logical);
            let source = &content[range.clone()];
            let fence = source.trim_start().starts_with("```");
            let mapping = rich
                .then(|| std::sync::Arc::new(markdown::MarkdownLine::parse_context(source, code)));
            if fence {
                code = !code;
            }
            let text = mapping.as_ref().map(|m| m.text.as_str()).unwrap_or(source);
            let heading = source.bytes().take_while(|b| *b == b'#').count();
            let scale = if rich
                && !code
                && (1..=3).contains(&heading)
                && source.as_bytes().get(heading) == Some(&b' ')
            {
                match heading {
                    1 => 1.65,
                    2 => 1.4,
                    _ => 1.2,
                }
            } else {
                1.
            };
            let runs = mapping
                .as_ref()
                .map(|m| m.text_runs(&style, &theme))
                .unwrap_or_else(|| {
                    vec![crate::sql_highlight::text_run(
                        text.len(),
                        &style,
                        theme.text,
                        false,
                    )]
                });
            let shaped = window.text_system().shape_line(
                SharedString::from(text.to_string()),
                font_size * scale,
                &runs,
                None,
            );
            if text.is_empty() {
                built.push(Row {
                    range,
                    display: 0..0,
                    mapping,
                    scale,
                });
                continue;
            }
            let mut start = 0;
            while start < text.len() {
                let x = shaped.x_for_index(start);
                let mut end = shaped.index_for_x(x + width).unwrap_or(text.len());
                while end > start && shaped.x_for_index(end) > x + width {
                    end = text[..end]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(start);
                }
                if end <= start {
                    end = start + text[start..].chars().next().unwrap().len_utf8();
                }
                end = end.min(text.len());
                // Keep words intact when a nearby boundary fits; CJK wraps by glyph.
                if end < text.len()
                    && let Some((index, _)) = text[start..end]
                        .char_indices()
                        .rev()
                        .find(|(i, c)| c.is_whitespace() && *i > 0)
                    && end - (start + index) < 40
                {
                    end = start + index + text[start + index..].chars().next().unwrap().len_utf8();
                }
                built.push(Row {
                    range: range.clone(),
                    display: start..end,
                    mapping: mapping.clone(),
                    scale,
                });
                start = end;
            }
        }
        rows = std::sync::Arc::new(built);
    }
    let mut first = editor.scroll_line.min(rows.len().saturating_sub(1));
    let visible = ((bounds.size.height - px(PAD_Y * 2.)) / height)
        .floor()
        .max(1.) as usize;
    if editor.reveal_caret
        && let Some(index) = rows.iter().position(|r| r.contains(caret))
        && (index < first || index >= first + visible.saturating_sub(2).max(1))
    {
        first = index.saturating_sub(visible.saturating_sub(3));
    }
    let mut lines = Vec::new();
    let mut placed = Vec::new();
    let mut tops = Vec::new();
    let mut y = px(0.);
    let mut selection = Vec::new();
    let mut cursor = None;
    let mut caret_origin = None;
    for row in rows.iter().skip(first) {
        let row_height = height * row.scale;
        if y + row_height > bounds.size.height - px(PAD_Y * 2.) && !lines.is_empty() {
            break;
        }
        let source = &content[row.range.clone()];
        let text = row
            .mapping
            .as_ref()
            .map(|m| m.text.as_str())
            .unwrap_or(source);
        let runs = row
            .mapping
            .as_ref()
            .map(|m| m.text_runs(&style, &theme))
            .unwrap_or_else(|| {
                vec![crate::sql_highlight::text_run(
                    text.len(),
                    &style,
                    theme.text,
                    false,
                )]
            });
        let shaped = window.text_system().shape_line(
            SharedString::from(text[row.display.clone()].to_string()),
            font_size * row.scale,
            &runs_slice(&runs, row.display.clone()),
            None,
        );
        let top = bounds.top() + px(PAD_Y) + y;
        let left = bounds.left() + px(PAD_X);
        let from = selected.start.max(row.range.start);
        let to = selected.end.min(row.range.end);
        if !selected.is_empty() && from <= to {
            let x1 = shaped.x_for_index(row.offset(from));
            let x2 = shaped.x_for_index(row.offset(to));
            if x2 > x1 {
                selection.push(fill(
                    Bounds::new(point(left + x1, top), size(x2 - x1, row_height)),
                    rgba(0x3311ff30),
                ));
            }
        }
        if selected.is_empty() && editor.enabled && row.contains(caret) {
            let origin = point(left + shaped.x_for_index(row.offset(caret)), top);
            caret_origin = Some(origin);
            cursor = Some(fill(
                Bounds::new(origin, size(px(2.), row_height)),
                theme.text,
            ));
        }
        tops.push((y, row_height));
        lines.push(shaped);
        placed.push(row.clone());
        y += row_height;
    }
    let layout = Layout {
        bounds,
        starts: Vec::new(),
        first,
        line_height: height,
        lines: lines.clone(),
        mappings: Vec::new(),
        visual: placed,
        tops: tops.clone(),
    };
    entity.update(cx, |editor, _| {
        editor.scroll_line = first;
        editor.viewport_lines = visible;
        editor.reveal_caret = false;
        editor.wrap_key = Some(key);
        editor.wrapped_rows = rows;
        editor.last = Some(layout);
    });
    view::Prepaint {
        lines,
        numbers: Vec::new(),
        line_height: height,
        selection,
        cursor,
        caret_origin,
        items: Vec::new(),
        selected: 0,
        tops,
        horizontal: 0.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapped_selection_and_ime_map_to_the_original_utf8_source() {
        let source = "**中文😀文字**";
        let mapping = std::sync::Arc::new(markdown::MarkdownLine::parse(source));
        let first = Row {
            range: 10..10 + source.len(),
            display: 0..6,
            mapping: Some(mapping.clone()),
            scale: 1.,
        };
        let second = Row {
            range: 10..10 + source.len(),
            display: 6..mapping.text.len(),
            mapping: Some(mapping),
            scale: 1.,
        };
        assert_eq!(first.source(0), 12);
        assert_eq!(first.offset(15), 3);
        assert!(!first.contains(18));
        assert!(second.contains(18));
        assert_eq!(second.offset(18), 0);
        assert_eq!(second.source(4), 22);
    }
    #[test]
    fn fenced_code_keeps_literal_markers_and_hides_the_language_fence() {
        assert!(
            markdown::MarkdownLine::parse_context("```sql", false)
                .text
                .is_empty()
        );
        assert_eq!(
            markdown::MarkdownLine::parse_context("SELECT '**literal**';", true).text,
            "SELECT '**literal**';"
        );
    }
}
