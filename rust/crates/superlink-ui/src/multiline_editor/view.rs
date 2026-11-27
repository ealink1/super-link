//! Painting for the multi-line SQL editor: text, gutter, selection, caret and
//! the completion popup. Lives in a child module so it can read the buffer
//! state directly without widening its visibility.

use super::*;
use crate::sql_highlight::text_run;

pub(super) struct EditorElement {
    editor: Entity<SqlTextEditor>,
}

pub(super) struct Prepaint {
    pub(super) lines: Vec<ShapedLine>,
    pub(super) numbers: Vec<ShapedLine>,
    pub(super) line_height: Pixels,
    pub(super) selection: Vec<PaintQuad>,
    pub(super) cursor: Option<PaintQuad>,
    pub(super) caret_origin: Option<Point<Pixels>>,
    pub(super) items: Vec<CompletionCandidate>,
    pub(super) selected: usize,
    pub(super) tops: Vec<(Pixels, Pixels)>,
    pub(super) horizontal: f32,
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
        style.size.height = relative(1.).into();
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
        if self.editor.read(cx).plain {
            return wrapping::prepaint(&self.editor, bounds, window, cx);
        }
        let (content, theme, selected, caret, marked, scroll, enabled) = {
            let editor = self.editor.read(cx);
            (
                editor.content.clone(),
                editor.theme,
                editor.selected_range.clone(),
                editor.cursor_offset(),
                editor.marked_range.clone(),
                editor.scroll_line,
                editor.enabled,
            )
        };
        let rich = self.editor.read(cx).rich_markdown;
        let gutter = self.editor.read(cx).gutter();
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let starts = self.editor.read(cx).cached_starts.as_ref().clone();
        let visible =
            (((bounds.size.height - px(PAD_Y * 2.0)) / line_height).floor()).max(1.0) as usize;
        let first = scroll.min(starts.len().saturating_sub(visible.min(starts.len())));
        let end = (first + visible).min(starts.len());
        let tokens = self.editor.read(cx).cached_tokens.clone();
        let placeholder = content.is_empty();

        let mut mappings = Vec::with_capacity(end - first);
        let mut lines = Vec::with_capacity(end - first);
        let mut numbers = Vec::with_capacity(end - first);
        for index in first..end {
            let range = line_range(&content, &starts, index);
            if placeholder {
                continue;
            }
            let mapping = rich.then(|| markdown::MarkdownLine::parse(&content[range.clone()]));
            let text = SharedString::from(
                mapping
                    .as_ref()
                    .map(|m| m.text.clone())
                    .unwrap_or_else(|| content[range.clone()].to_string()),
            );
            let runs = mapping
                .as_ref()
                .map(|m| m.text_runs(&text_style, &theme))
                .unwrap_or_else(|| line_runs(&tokens, range, &text_style, &theme, marked.as_ref()));
            mappings.push(mapping);
            lines.push(
                window
                    .text_system()
                    .shape_line(text, font_size, &runs, None),
            );
            if gutter == 0.0 {
                continue;
            }
            let label = SharedString::from(format!("{}", index + 1));
            let run = text_run(label.len(), &text_style, theme.muted, false);
            numbers.push(
                window
                    .text_system()
                    .shape_line(label, font_size, &[run], None),
            );
        }

        let mut horizontal = self.editor.read(cx).scroll_x;
        let caret_line = line_of(&starts, caret);
        if self.editor.read(cx).reveal_caret
            && caret_line >= first
            && caret_line < end
            && !placeholder
        {
            let range = line_range(&content, &starts, caret_line);
            let x = f32::from(lines[caret_line - first].x_for_index(caret - range.start));
            let width = f32::from(bounds.size.width) - gutter - PAD_X * 2.;
            if x - horizontal > width {
                horizontal = (x - width + 16.).max(0.);
            } else if x < horizontal {
                horizontal = (x - 16.).max(0.);
            }
        }
        let mut selection = Vec::new();
        if !selected.is_empty() && !placeholder {
            for (offset, shaped) in lines.iter().enumerate() {
                let index = first + offset;
                let range = line_range(&content, &starts, index);
                let from = selected.start.max(range.start);
                let to = selected.end.min(range.end);
                if to < from {
                    continue;
                }
                let top = bounds.top() + px(PAD_Y) + line_height * (offset as f32);
                let left = bounds.left()
                    + px(gutter + PAD_X - horizontal)
                    + shaped.x_for_index(
                        mappings[offset]
                            .as_ref()
                            .map(|m| m.display_offset(from - range.start))
                            .unwrap_or(from - range.start),
                    );
                let mut right = bounds.left()
                    + px(gutter + PAD_X - horizontal)
                    + shaped.x_for_index(
                        mappings[offset]
                            .as_ref()
                            .map(|m| m.display_offset(to - range.start))
                            .unwrap_or(to - range.start),
                    );
                if selected.end > range.end {
                    right += px(6.0);
                }
                if right > left {
                    selection.push(fill(
                        Bounds::from_corners(point(left, top), point(right, top + line_height)),
                        rgba(0x3311ff30),
                    ));
                }
            }
        }

        let mut caret_origin = None;
        let cursor = if selected.is_empty() && enabled && !placeholder {
            let line = line_of(&starts, caret);
            if line >= first && line < end {
                let range = line_range(&content, &starts, line);
                let shaped = &lines[line - first];
                let x = bounds.left()
                    + px(gutter + PAD_X - horizontal)
                    + shaped.x_for_index(
                        mappings[line - first]
                            .as_ref()
                            .map(|m| m.display_offset(caret.min(range.end) - range.start))
                            .unwrap_or(caret.min(range.end) - range.start),
                    );
                let top = bounds.top() + px(PAD_Y) + line_height * ((line - first) as f32);
                caret_origin = Some(point(x, top));
                Some(fill(
                    Bounds::new(point(x, top), size(px(2.0), line_height)),
                    theme.text,
                ))
            } else {
                None
            }
        } else {
            None
        };

        let (items, selected_item) = {
            let editor = self.editor.read(cx);
            match editor.completion.as_ref() {
                Some(completion) => (completion.items.clone(), completion.selected),
                None => (Vec::new(), 0),
            }
        };

        self.editor.update(cx, |editor, _cx| {
            editor.viewport_lines = visible;
            editor.scroll_x = horizontal;
            editor.reveal_caret = false;
            editor.last = Some(Layout {
                visual: Vec::new(),
                tops: Vec::new(),
                bounds,
                starts,
                first,
                line_height,
                lines: lines.clone(),
                mappings,
            });
            if placeholder {
                editor.last = Some(Layout {
                    visual: Vec::new(),
                    tops: Vec::new(),
                    bounds,
                    starts: vec![0],
                    first: 0,
                    line_height,
                    lines: Vec::new(),
                    mappings: Vec::new(),
                });
            }
        });

        Prepaint {
            tops: Vec::new(),
            horizontal,
            lines,
            numbers,
            line_height,
            selection,
            cursor,
            caret_origin,
            items,
            selected: selected_item,
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
        let (focus_handle, theme, placeholder) = {
            let editor = self.editor.read(cx);
            (
                editor.focus_handle.clone(),
                editor.theme,
                editor.placeholder.clone(),
            )
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }
        let gutter = self.editor.read(cx).gutter();
        let text_left = bounds.left() + px(gutter + PAD_X);
        for (offset, shaped) in prepaint.lines.iter().enumerate() {
            let (offset_top, height) = prepaint
                .tops
                .get(offset)
                .copied()
                .unwrap_or((prepaint.line_height * (offset as f32), prepaint.line_height));
            let top = bounds.top() + px(PAD_Y) + offset_top;
            if let Some(number) = prepaint.numbers.get(offset) {
                let x = bounds.left() + px(gutter) - px(8.0) - number.width;
                let _ = number.paint(point(x, top), prepaint.line_height, window, cx);
            }
            window.with_content_mask(
                Some(gpui::ContentMask {
                    bounds: Bounds::new(
                        point(text_left, bounds.top()),
                        size(
                            (bounds.size.width - px(gutter + PAD_X)).max(px(1.)),
                            bounds.size.height,
                        ),
                    ),
                }),
                |window| {
                    let _ = shaped.paint(
                        point(text_left - px(prepaint.horizontal), top),
                        height,
                        window,
                        cx,
                    );
                },
            );
        }
        if prepaint.lines.is_empty() {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let run = text_run(placeholder.len(), &style, theme.placeholder, false);
            let line =
                window
                    .text_system()
                    .shape_line(placeholder.clone(), font_size, &[run], None);
            let _ = line.paint(
                point(text_left, bounds.top() + px(PAD_Y)),
                prepaint.line_height,
                window,
                cx,
            );
        }
        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        // Completion popup: anchored under the caret and flipped above when the
        // remaining space is too small.
        if !prepaint.items.is_empty()
            && let Some(origin) = prepaint.caret_origin
        {
            let style = window.text_style();
            let font_size = style.font_size.to_pixels(window.rem_size());
            let row_height = prepaint.line_height;
            let width = px(240.0);
            let rows = prepaint.items.len().min(8);
            let height = row_height * (rows as f32) + px(8.0);
            let mut x = origin.x;
            if x + width > bounds.right() {
                x = bounds.right() - width - px(4.0);
            }
            let min_x = bounds.left() + px(gutter);
            if x < min_x {
                x = min_x;
            }
            let mut y = origin.y + row_height + px(2.0);
            if y + height > bounds.bottom() {
                y = origin.y - height - px(2.0);
            }
            if y < bounds.top() {
                y = bounds.top();
            }
            let rect = Bounds::new(point(x, y), size(width, height));
            window.paint_quad(PaintQuad {
                bounds: rect,
                corner_radii: Corners::all(px(6.0)),
                background: theme.surface.into(),
                border_widths: Edges::all(px(1.0)),
                border_color: theme.border,
                border_style: BorderStyle::Solid,
            });
            let first_row = prepaint.selected.saturating_sub(rows.saturating_sub(1));
            for (offset, item) in prepaint.items.iter().enumerate().skip(first_row).take(rows) {
                let row_top = rect.top() + px(4.0) + row_height * ((offset - first_row) as f32);
                if offset == prepaint.selected {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(rect.left() + px(2.0), row_top),
                            size(width - px(4.0), row_height),
                        ),
                        theme.accent_soft,
                    ));
                }
                let name_run = text_run(item.text.len(), &style, theme.text, false);
                let name_line = window.text_system().shape_line(
                    SharedString::from(item.text.clone()),
                    font_size,
                    &[name_run],
                    None,
                );
                let _ = name_line.paint(
                    point(rect.left() + px(8.0), row_top),
                    row_height,
                    window,
                    cx,
                );
                let kind_run = text_run(item.kind.len(), &style, theme.muted, false);
                let kind_line = window.text_system().shape_line(
                    SharedString::from(item.kind.clone()),
                    font_size,
                    &[kind_run],
                    None,
                );
                let _ = kind_line.paint(
                    point(rect.right() - px(8.0) - kind_line.width, row_top),
                    row_height,
                    window,
                    cx,
                );
            }
        }
    }
}

impl Render for SqlTextEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id("sql-text-editor")
            .key_context("SqlTextEditor")
            .track_focus(&self.focus_handle)
            .tab_stop(true)
            .size_full()
            .bg(if self.plain {
                theme.surface
            } else {
                theme.canvas
            })
            .text_size(px(if self.plain { 16.0 } else { 12.0 }))
            .cursor(if self.enabled {
                CursorStyle::IBeam
            } else {
                CursorStyle::Arrow
            })
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if !this.enabled {
                        return;
                    }
                    this.focus_handle.focus(window);
                    this.selecting = true;
                    this.completion = None;
                    if let Some(offset) = this.index_for_position(event.position) {
                        this.select_by_click(offset, event.click_count, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.selecting
                    && this.enabled
                    && let Some(offset) = this.index_for_position(event.position)
                {
                    this.select_to(offset, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.selecting = false;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                if !this.plain {
                    let (x, y) = match event.delta {
                        ScrollDelta::Lines(p) => (p.x * 24., p.y * 24.),
                        ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
                    };
                    let x = if event.modifiers.shift { y } else { x };
                    if x.abs() > 0. {
                        this.scroll_x = (this.scroll_x - x).max(0.);
                        this.reveal_caret = false;
                        cx.notify();
                        return;
                    }
                }
                // Downward wheel movement reports a positive delta and moves
                // the viewport toward later lines.
                let lines = match event.delta {
                    ScrollDelta::Lines(delta) => delta.y.round() as isize,
                    ScrollDelta::Pixels(delta) => {
                        let height = this
                            .last
                            .as_ref()
                            .map(|l| l.line_height)
                            .unwrap_or(px(16.0));
                        (delta.y.to_f64() / height.to_f64()).round() as isize
                    }
                };
                if lines != 0 {
                    this.scroll_by(-lines, cx);
                }
            }))
            .child(EditorElement {
                editor: cx.entity(),
            })
    }
}
