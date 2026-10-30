//! Single-line SQL input with native IME support.
//!
//! Text entry goes through EntityInputHandler so Chinese input methods compose
//! correctly; navigation and editing are handled from key events.

use std::ops::Range;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId, KeyDownEvent,
    LayoutId, MouseButton, PaintQuad, Pixels, Point, Render, ShapedLine, SharedString, Style,
    TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, point, prelude::*, px, relative,
    rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

type RunHandler = Box<dyn Fn(&mut Window, &mut App) + 'static>;
/// Tab / Shift+Tab inside one input; owners wire their own field order.
type TabHandler = Box<dyn Fn(bool, &mut Window, &mut App) + 'static>;
type ChangeHandler = Box<dyn Fn(&str, &mut App) + 'static>;

pub struct SqlEditor {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    /// 文本溢出时向左滚动的距离，点击定位需要补回来。
    last_offset: Pixels,
    enabled: bool,
    /// Invoked by Cmd+R so the editor can run without a toolbar click.
    on_run: Option<RunHandler>,
    /// Fires after the text changed, used by search boxes.
    on_change: Option<ChangeHandler>,
    /// Password fields render one dot per byte instead of the characters.
    masked: bool,
    on_tab: Option<TabHandler>,
}

impl SqlEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut focus_handle = cx.focus_handle();
        // Tab 遍历依赖 FocusHandle 上的标记，元素上的 .tab_stop 不会传过来。
        focus_handle.tab_stop = true;
        Self {
            focus_handle,
            content: SharedString::default(),
            placeholder: "输入 SQL，例如 SELECT 1;".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_offset: px(0.0),
            enabled: true,
            on_run: None,
            on_change: None,
            masked: false,
            on_tab: None,
        }
    }

    pub fn set_on_change(
        &mut self,
        handler: impl Fn(&str, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_change = Some(Box::new(handler));
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        self.masked = masked;
        cx.notify();
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    fn emit_change(&self, cx: &mut Context<Self>) {
        if let Some(handler) = self.on_change.as_ref() {
            let text = self.content.to_string();
            handler(&text, cx);
        }
    }

    pub fn set_on_run(
        &mut self,
        handler: impl Fn(&mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_run = Some(Box::new(handler));
    }

    /// Programmatic writes never fire on_change: callers often run inside an
    /// update of the page that owns this editor, and re-entering it panics.
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.marked_range = None;
        cx.notify();
    }

    pub fn set_on_tab(
        &mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_tab = Some(Box::new(handler));
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.enabled = enabled;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        match key {
            "backspace" => {
                if self.selected_range.is_empty() {
                    self.select_to(self.previous_boundary(self.cursor_offset()), cx);
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "delete" => {
                if self.selected_range.is_empty() {
                    self.select_to(self.next_boundary(self.cursor_offset()), cx);
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "left" => self.move_to(self.previous_boundary(self.cursor_offset()), cx),
            "right" => self.move_to(self.next_boundary(self.cursor_offset()), cx),
            "home" => self.move_to(0, cx),
            "end" => self.move_to(self.content.len(), cx),
            "tab" => {
                if let Some(handler) = self.on_tab.as_ref() {
                    cx.stop_propagation();
                    handler(modifiers.shift, window, cx);
                }
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
                    let text = text.replace(['\n', '\r'], " ");
                    self.replace_text_in_range(None, &text, window, cx);
                }
            }
            _ => {}
        }
    }

    fn copy(&self, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
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
}

impl Focusable for SqlEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for SqlEditor {
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
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + layout.x_for_index(range.start) - self.last_offset,
                bounds.top(),
            ),
            point(
                bounds.left() + layout.x_for_index(range.end) - self.last_offset,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let local = self.last_bounds?.localize(&point)?;
        let layout = self.last_layout.as_ref()?;
        let index = layout.index_for_x(local.x + self.last_offset)?;
        Some(self.offset_to_utf16(index))
    }
}

struct EditorElement {
    editor: Entity<SqlEditor>,
}

struct Prepaint {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    /// 文本超过输入框宽度时向左滚动的距离，保证光标始终可见。
    offset: Pixels,
    /// 滚动后实际绘制的可见片段及其横向偏移。
    paint: Option<(Pixels, ShapedLine)>,
}

/// 把布局索引夹到最近的字符边界，避免切片越界。
fn boundary(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    (0..=index)
        .rev()
        .find(|offset| text.is_char_boundary(*offset))
        .unwrap_or(0)
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let content = editor.content.clone();
        let selected_range = editor.selected_range.clone();
        let cursor = editor.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (editor.placeholder.clone(), rgba(0x8a8a8a55).into())
        } else if editor.masked {
            // One dot per byte keeps caret math identical to the real text.
            ("*".repeat(content.len()).into(), style.color)
        } else {
            (content, style.color)
        };
        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked) = editor.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.end - marked.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let full_text = display_text.clone();
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);
        let cursor_x = line.x_for_index(cursor);
        let offset = (cursor_x - bounds.size.width + px(4.0)).max(px(0.0));
        // 自绘文本不会被父级裁剪，超出输入框时只绘制可见片段。
        let paint = if line.width > bounds.size.width {
            let start = boundary(&full_text, line.index_for_x(offset).unwrap_or(0));
            let end = boundary(
                &full_text,
                line.index_for_x(offset + bounds.size.width)
                    .unwrap_or(full_text.len()),
            );
            if end > start {
                let slice: SharedString = full_text[start..end].to_string().into();
                let run = TextRun {
                    len: slice.len(),
                    font: style.font(),
                    color: text_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(slice, font_size, &[run], None);
                Some((line.x_for_index(start) - offset, shaped))
            } else {
                None
            }
        } else {
            None
        };
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_x - offset, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start) - offset,
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end) - offset,
                            bounds.bottom(),
                        ),
                    ),
                    rgba(0x3311ff30),
                )),
                None,
            )
        };
        Prepaint {
            line: Some(line),
            cursor,
            selection,
            offset,
            paint,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        let focused = focus_handle.is_focused(window);
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        if let Some((dx, shaped)) = prepaint.paint.as_ref() {
            let origin = point(bounds.left() + *dx, bounds.top());
            let _ = shaped.paint(origin, window.line_height(), window, cx);
        } else if let Some(line) = prepaint.line.as_ref() {
            let _ = line.paint(bounds.origin, window.line_height(), window, cx);
        }
        if focused {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }
        let offset = prepaint.offset;
        self.editor.update(cx, |editor, _cx| {
            editor.last_layout = prepaint.line.take();
            editor.last_bounds = Some(bounds);
            editor.last_offset = offset;
        });
    }
}

impl Render for SqlEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(cx.entity_id().as_u64() as usize)
            .key_context("SqlEditor")
            .track_focus(&self.focus_handle)
            // 输入框默认填满所在容器，否则父级 flex 行里宽度为 0，点击命中不到。
            .flex_1()
            .min_w(px(0.0))
            .tab_stop(true)
            .cursor(if self.enabled {
                CursorStyle::IBeam
            } else {
                CursorStyle::Arrow
            })
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    if this.enabled {
                        this.focus_handle.focus(window);
                        cx.notify();
                    }
                }),
            )
            .child(EditorElement {
                editor: cx.entity(),
            })
    }
}
