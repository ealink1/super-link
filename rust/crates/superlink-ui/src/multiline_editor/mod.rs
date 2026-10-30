//! Multi-line SQL editor with line numbers, syntax coloring and IME support.
//!
//! The buffer is one string and the caret is a byte offset; only the visible
//! lines are laid out, so long scripts stay responsive while scrolling.

use std::ops::Range;

use gpui::{
    App, BorderStyle, Bounds, ClipboardItem, Context, Corners, CursorStyle, Edges, Element,
    ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, InspectorElementId, IntoElement, KeyDownEvent, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, PaintQuad, Pixels, Point, Render, ScrollDelta,
    ScrollWheelEvent, ShapedLine, SharedString, Style, UTF16Selection, Window, div, fill, point,
    prelude::*, px, relative, rgba, size,
};
use superlink_sql::lexer::tokenize;
use superlink_sql::{CompletionCandidate, CompletionTable, analyze_completion, complete};

mod view;

use crate::sql_highlight::{line_of, line_range, line_runs, line_starts};
use crate::theme::Theme;

const GUTTER: f32 = 44.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 6.0;

type RunHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;
type ChangeHandler = Box<dyn Fn(&str, &mut App) + 'static>;

pub struct SqlTextEditor {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    enabled: bool,
    theme: Theme,
    /// First visible line.
    scroll_line: usize,
    viewport_lines: usize,
    selecting: bool,
    last: Option<Layout>,
    on_run: Option<RunHandler>,
    on_change: Option<ChangeHandler>,
    completion: Option<Completion>,
    dialect: String,
    /// Cached metadata snapshot; empty until the object tree loads tables.
    tables: Vec<CompletionTable>,
}

/// Open completion popup: byte range to replace plus the ranked candidates.
struct Completion {
    items: Vec<CompletionCandidate>,
    selected: usize,
    start: usize,
    end: usize,
}

/// Geometry from the last frame, reused for hit testing and IME placement.
struct Layout {
    bounds: Bounds<Pixels>,
    starts: Vec<usize>,
    first: usize,
    line_height: Pixels,
    lines: Vec<ShapedLine>,
}

impl SqlTextEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut focus_handle = cx.focus_handle();
        focus_handle.tab_stop = true;
        Self {
            focus_handle,
            content: SharedString::default(),
            placeholder: "输入 SQL，例如 SELECT 1;".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            enabled: true,
            theme: Theme::light(),
            scroll_line: 0,
            viewport_lines: 1,
            selecting: false,
            last: None,
            on_run: None,
            on_change: None,
            completion: None,
            dialect: "postgresql".into(),
            tables: Vec::new(),
        }
    }

    pub fn set_on_change(
        &mut self,
        handler: impl Fn(&str, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_change = Some(Box::new(handler));
    }

    pub fn set_on_run(
        &mut self,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_run = Some(Box::new(handler));
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.scroll_line = 0;
        cx.notify();
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn set_dialect(&mut self, dialect: &str, _cx: &mut Context<Self>) {
        self.dialect = dialect.to_string();
    }

    /// Replaces the metadata snapshot used for table and column completion.
    pub fn set_tables(&mut self, tables: Vec<CompletionTable>, cx: &mut Context<Self>) {
        self.tables = tables;
        self.completion = None;
        cx.notify();
    }

    /// Recomputes the popup from the token under the caret. Metadata tables are
    /// not cached yet, so this offers keywords only.
    pub(crate) fn refresh_completion(&mut self, cx: &mut Context<Self>) {
        if !self.enabled {
            self.completion = None;
            return;
        }
        let caret = self.cursor_offset();
        match candidates(&self.content, caret, &self.dialect, &self.tables) {
            Some((start, end, items)) => {
                self.completion = Some(Completion {
                    items,
                    selected: 0,
                    start,
                    end,
                });
            }
            None => self.completion = None,
        }
        cx.notify();
    }

    fn move_completion(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(completion) = self.completion.as_mut() {
            let last = completion.items.len().saturating_sub(1);
            let next = (completion.selected as isize + delta).clamp(0, last as isize) as usize;
            completion.selected = next;
            cx.notify();
        }
    }

    fn accept_completion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(completion) = self.completion.take() else {
            return false;
        };
        let Some(item) = completion.items.get(completion.selected) else {
            return false;
        };
        self.content = (self.content[0..completion.start].to_owned()
            + &item.insert
            + &self.content[completion.end..])
            .into();
        let caret = completion.start + item.insert.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.ensure_caret_visible();
        cx.notify();
        self.emit_change(cx);
        true
    }

    fn close_completion(&mut self, cx: &mut Context<Self>) {
        if self.completion.take().is_some() {
            cx.notify();
        }
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.completion = None;
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.ensure_caret_visible();
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.completion = None;
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.ensure_caret_visible();
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn caret_line(&self) -> usize {
        let starts = line_starts(&self.content);
        line_of(&starts, self.cursor_offset())
    }

    fn ensure_caret_visible(&mut self) {
        let line = self.caret_line();
        let viewport = self.viewport_lines.max(1);
        if line < self.scroll_line {
            self.scroll_line = line;
        } else if line >= self.scroll_line + viewport {
            self.scroll_line = line + 1 - viewport;
        }
    }

    fn move_vertical(&mut self, delta: isize, extend: bool, cx: &mut Context<Self>) {
        let starts = line_starts(&self.content);
        let cursor = self.cursor_offset();
        let line = line_of(&starts, cursor);
        let current = line_range(&self.content, &starts, line);
        let column = self.content[current.start..cursor].chars().count();
        let target_line = (line as isize + delta).clamp(0, starts.len() as isize - 1) as usize;
        let target = line_range(&self.content, &starts, target_line);
        let text = &self.content[target.clone()];
        let offset = text
            .char_indices()
            .nth(column)
            .map(|(index, _)| target.start + index)
            .unwrap_or(target.end);
        if extend {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn move_line_edge(&mut self, start: bool, extend: bool, cx: &mut Context<Self>) {
        let starts = line_starts(&self.content);
        let line = line_of(&starts, self.cursor_offset());
        let target = if start {
            starts[line]
        } else {
            line_range(&self.content, &starts, line).end
        };
        if extend {
            self.select_to(target, cx);
        } else {
            self.move_to(target, cx);
        }
    }

    fn scroll_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let lines = line_starts(&self.content).len();
        let viewport = self.viewport_lines.max(1);
        let max_first = lines.saturating_sub(viewport);
        self.scroll_line =
            (self.scroll_line as isize + delta).clamp(0, max_first as isize) as usize;
        cx.notify();
    }

    fn copy(&self, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn emit_change(&self, cx: &mut Context<Self>) {
        if let Some(handler) = self.on_change.as_ref() {
            handler(&self.content, cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let extend = modifiers.shift;
        if self.completion.is_some() {
            match key {
                "escape" => {
                    self.close_completion(cx);
                    return;
                }
                "down" => {
                    self.move_completion(1, cx);
                    return;
                }
                "up" => {
                    self.move_completion(-1, cx);
                    return;
                }
                "enter" | "tab" if self.accept_completion(cx) => return,
                _ => {}
            }
        }
        match key {
            "enter" => self.replace_text_in_range(None, "\n", window, cx),
            "tab" => {
                // 代码编辑器里 Tab 用于缩进，阻止根节点把焦点移走。
                cx.stop_propagation();
                self.replace_text_in_range(None, "  ", window, cx)
            }
            "backspace" => {
                if self.selected_range.is_empty() {
                    let target = self.previous_boundary(self.cursor_offset());
                    self.selected_range = target..self.cursor_offset();
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "delete" => {
                if self.selected_range.is_empty() {
                    let target = self.next_boundary(self.cursor_offset());
                    self.selected_range = self.cursor_offset()..target;
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "left" if modifiers.platform => self.move_to(0, cx),
            "right" if modifiers.platform => self.move_to(self.content.len(), cx),
            "up" if modifiers.platform => self.move_to(0, cx),
            "down" if modifiers.platform => self.move_to(self.content.len(), cx),
            "left" => {
                let target = self.previous_boundary(self.cursor_offset());
                if extend {
                    self.select_to(target, cx);
                } else {
                    self.move_to(target, cx);
                }
            }
            "right" => {
                let target = self.next_boundary(self.cursor_offset());
                if extend {
                    self.select_to(target, cx);
                } else {
                    self.move_to(target, cx);
                }
            }
            "up" => self.move_vertical(-1, extend, cx),
            "down" => self.move_vertical(1, extend, cx),
            "home" => self.move_line_edge(true, extend, cx),
            "end" => self.move_line_edge(false, extend, cx),
            "pageup" => {
                let step = self.viewport_lines.saturating_sub(1).max(1) as isize;
                self.move_vertical(-step, extend, cx);
            }
            "pagedown" => {
                let step = self.viewport_lines.saturating_sub(1).max(1) as isize;
                self.move_vertical(step, extend, cx);
            }
            "r" if modifiers.platform => {
                if let Some(handler) = self.on_run.as_ref() {
                    handler(window, cx);
                }
            }
            "a" if modifiers.platform => self.select_all(cx),
            "c" if modifiers.platform => self.copy(cx),
            "x" if modifiers.platform => {
                self.copy(cx);
                self.replace_text_in_range(None, "", window, cx);
            }
            "v" if modifiers.platform => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_text_in_range(None, &text, window, cx);
                }
            }
            _ => {}
        }
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| offset + index)
            .unwrap_or(self.content.len())
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        let mut utf8 = 0;
        for ch in self.content.chars() {
            if utf8 >= offset {
                break;
            }
            utf8 += ch.len_utf8();
            utf16 += ch.len_utf16();
        }
        utf16
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn index_for_position(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.last.as_ref()?;
        let local = layout.bounds.localize(&position)?;
        let row = ((local.y - px(PAD_Y)) / layout.line_height).floor();
        let index = (row.max(0.0) as usize).min(layout.lines.len().saturating_sub(1));
        let line = layout.first + index;
        let shaped = layout.lines.get(index)?;
        let range = line_range(&self.content, &layout.starts, line);
        let offset = shaped
            .index_for_x(local.x - px(GUTTER + PAD_X))
            .map(|index| (range.start + index).min(range.end))
            .unwrap_or(range.start);
        Some(offset)
    }
}

/// Candidates for the token under the caret plus the byte range to replace.
fn candidates(
    content: &str,
    caret: usize,
    dialect: &str,
    tables: &[CompletionTable],
) -> Option<(usize, usize, Vec<CompletionCandidate>)> {
    let chars = content[..caret].chars().count();
    let query = analyze_completion(content, chars);
    if !query.active || (query.prefix.is_empty() && query.qualifier.is_empty()) {
        return None;
    }
    let items = complete(&query, tables, dialect);
    if items.is_empty() {
        return None;
    }
    Some((
        char_to_byte(content, query.start),
        char_to_byte(content, query.end),
        items,
    ))
}

/// Converts a character offset (used by the completion analyzer) to bytes.
fn char_to_byte(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

impl Focusable for SqlTextEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for SqlTextEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        self.ensure_caret_visible();
        self.refresh_completion(cx);
        cx.notify();
        self.emit_change(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .map(|range| range.start + range.start..range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let line = line_of(&layout.starts, range.start);
        let index = line.saturating_sub(layout.first);
        let shaped = layout.lines.get(index)?;
        let line_start = layout.starts[line];
        let top = bounds.top() + px(PAD_Y) + layout.line_height * (index as f32);
        Some(Bounds::from_corners(
            point(
                bounds.left() + px(GUTTER + PAD_X) + shaped.x_for_index(range.start - line_start),
                top,
            ),
            point(
                bounds.left() + px(GUTTER + PAD_X) + shaped.x_for_index(range.end - line_start),
                top + layout.line_height,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self.index_for_position(point)?;
        Some(self.offset_to_utf16(index))
    }
}

#[cfg(test)]
mod tests {
    use super::{candidates, char_to_byte};
    use superlink_sql::CompletionTable;

    fn catalog() -> Vec<CompletionTable> {
        vec![CompletionTable {
            schema: "public".into(),
            name: "orders".into(),
            columns: vec![
                "id".into(),
                "amount".into(),
                "customer_id".into(),
                "created_at".into(),
            ],
        }]
    }

    #[test]
    fn keywords_complete_without_metadata() {
        let text = "select * from orders where in";
        let (start, end, items) = candidates(text, text.len(), "postgresql", &[]).unwrap();
        assert_eq!(&text[start..end], "in");
        assert!(items.iter().any(|item| item.text == "INNER"));
        assert!(items.iter().all(|item| item.kind == "关键字"));
    }

    #[test]
    fn tables_complete_after_from() {
        let text = "select * from ord";
        let (start, end, items) = candidates(text, text.len(), "postgresql", &catalog()).unwrap();
        assert_eq!(&text[start..end], "ord");
        let table = items.iter().find(|item| item.text == "orders").unwrap();
        assert_eq!(table.kind, "表");
    }

    #[test]
    fn qualifiers_complete_columns() {
        let text = "select o. from orders o";
        let caret = "select o.".len();
        let (start, end, items) = candidates(text, caret, "postgresql", &catalog()).unwrap();
        assert_eq!(&text[start..end], "");
        assert!(
            items
                .iter()
                .any(|item| item.text == "amount" && item.kind == "字段")
        );
        assert!(items.iter().all(|item| item.text != "orders"));
    }

    #[test]
    fn column_prefixes_filter_the_qualified_list() {
        let text = "select o.cu from orders o";
        let caret = "select o.cu".len();
        let (start, end, items) = candidates(text, caret, "postgresql", &catalog()).unwrap();
        assert_eq!(&text[start..end], "cu");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "customer_id");
    }

    #[test]
    fn inside_a_string_nothing_is_offered() {
        let text = "select * from orders where name = 'ord";
        assert!(candidates(text, text.len(), "postgresql", &catalog()).is_none());
    }

    #[test]
    fn char_offsets_convert_to_byte_offsets() {
        assert_eq!(char_to_byte("", 0), 0);
        assert_eq!(char_to_byte("select", 0), 0);
        assert_eq!(char_to_byte("select", 3), 3);
        assert_eq!(char_to_byte("select", 6), 6);
        assert_eq!(char_to_byte("select", 99), 6);
    }

    #[test]
    fn multi_byte_identifiers_stay_on_char_boundaries() {
        let text = "-- 注释\nselect 1";
        assert_eq!(char_to_byte(text, 0), 0);
        assert_eq!(char_to_byte(text, 1), 1);
        assert_eq!(char_to_byte(text, 2), 2);
        assert_eq!(char_to_byte(text, 4), 6);
        assert!(text.is_char_boundary(char_to_byte(text, 5)));
    }
}
