use super::*;

pub(super) struct ScreenElement {
    pub(super) page: Entity<TerminalPage>,
    pub(super) rows: usize,
    pub(super) scroll_offset: usize,
    pub(super) focused: bool,
}

pub(super) struct Prepaint {
    selection: Vec<PaintQuad>,
    lines: Vec<ShapedLine>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for ScreenElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ScreenElement {
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
        let style = window.text_style();
        let mut font = style.font();
        font.family = "Menlo".into();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let cell_width = window
            .text_system()
            .shape_line(
                "M".into(),
                font_size,
                &[TextRun {
                    len: 1,
                    font: font.clone(),
                    color: rgb(0xffffff).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width;
        let cols = (bounds.size.width / cell_width.max(px(1.0)))
            .floor()
            .clamp(2.0, 500.0) as usize;
        let rows = (bounds.size.height / window.line_height())
            .floor()
            .clamp(2.0, 200.0) as usize;
        self.rows = rows;
        self.page.update(cx, |page, cx| {
            page.screen_bounds = Some(bounds);
            if page.screen.cols() != cols || page.screen.rows() != rows {
                page.screen.resize(cols, rows);
                if let Some(handle) = page.session.clone() {
                    cx.background_executor()
                        .spawn(async move {
                            let _ = handle.resize(cols as u16, rows as u16);
                        })
                        .detach();
                }
            }
        });
        let mut lines = Vec::with_capacity(self.rows);
        let mut cursor = None;
        let theme = self.page.read(cx).theme;
        let screen = &self.page.read(cx).screen;
        for row in 0..self.rows {
            let (text, runs) = screen.visible_row(row, self.scroll_offset);
            let runs: Vec<TextRun> = runs
                .into_iter()
                .map(|(range, cell)| {
                    let mut font = font.clone();
                    if cell.bold {
                        font.weight = gpui::FontWeight::BOLD;
                    }
                    if cell.italic {
                        font.style = gpui::FontStyle::Italic;
                    }
                    let foreground = if cell.fg == Color::Default {
                        theme.text
                    } else {
                        cell_color(cell.fg)
                    };
                    let background = if cell.bg == Color::Default {
                        theme.canvas
                    } else {
                        cell_color(cell.bg)
                    };
                    TextRun {
                        len: range.len(),
                        font,
                        color: visible_ink(
                            if cell.reverse { background } else { foreground },
                            if cell.reverse { foreground } else { background },
                            theme,
                        ),
                        background_color: if cell.reverse {
                            Some(foreground)
                        } else {
                            (cell.bg != Color::Default).then_some(background)
                        },
                        underline: cell.underline.then_some(gpui::UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: false,
                        }),
                        strikethrough: None,
                    }
                })
                .collect();
            let shaped =
                window
                    .text_system()
                    .shape_line(SharedString::from(text), font_size, &runs, None);
            if self.focused && self.scroll_offset == 0 && screen.cursor_visible() {
                let (row_index, col) = screen.cursor();
                if row_index == row {
                    let x = shaped.x_for_index(byte_index(screen, row, col));
                    cursor = Some(fill(
                        Bounds::new(
                            point(
                                bounds.left() + x,
                                bounds.top() + window.line_height() * row as f32,
                            ),
                            size(px(8.0), window.line_height()),
                        ),
                        theme.muted,
                    ));
                }
            }
            lines.push(shaped);
        }
        let selection = self
            .page
            .read(cx)
            .selection_quads(&lines, bounds, window.line_height());
        self.page
            .update(cx, |page, _| page.screen_lines = lines.clone());
        Prepaint {
            lines,
            cursor,
            selection,
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
        let focus = self.page.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.page.clone()),
            cx,
        );
        let line_height = window.line_height();
        for (row, line) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * row as f32);
            let _ = line.paint_background(origin, line_height, window, cx);
        }
        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }
        for (row, line) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * row as f32);
            let _ = line.paint(origin, line_height, window, cx);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

fn byte_index(screen: &Screen, row: usize, col: usize) -> usize {
    screen.byte_index(row, col)
}

fn cell_color(color: Color) -> gpui::Hsla {
    match color {
        Color::Default => rgb(0xd0d7de).into(),
        Color::Indexed(index) => rgb(palette(index)).into(),
        Color::Rgb(r, g, b) => rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into(),
    }
}

fn palette(index: u8) -> u32 {
    const ANSI: [u32; 16] = [
        0x1b1f23, 0xe06c75, 0x98c379, 0xd19a66, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf, 0x5c6370,
        0xe06c75, 0x98c379, 0xd19a66, 0x61afef, 0xc678dd, 0x56b6c2, 0xffffff,
    ];
    if index < 16 {
        return ANSI[index as usize];
    }
    if index >= 232 {
        let level = 8 + (index as u32 - 232) * 10;
        return (level << 16) | (level << 8) | level;
    }
    let index = index as u32 - 16;
    let steps = [0u32, 95, 135, 175, 215, 255];
    let r = steps[(index / 36) as usize];
    let g = steps[((index % 36) / 6) as usize];
    let b = steps[(index % 6) as usize];
    (r << 16) | (g << 8) | b
}

fn luminance(color: gpui::Hsla) -> f32 {
    let rgb = color.to_rgb();
    let linear = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
}
fn contrast(a: gpui::Hsla, b: gpui::Hsla) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn visible_ink(ink: gpui::Hsla, background: gpui::Hsla, theme: Theme) -> gpui::Hsla {
    // Fyne adjusts ANSI ink only in day mode, preserving its hue and saturation.
    if theme.canvas.l < 0.5 || contrast(ink, background) >= 4.5 {
        return ink;
    }
    let target = if luminance(background) > 0.18 { 0. } else { 1. };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..24 {
        let fraction = (low + high) / 2.;
        let candidate = gpui::Hsla {
            l: ink.l + (target - ink.l) * fraction,
            ..ink
        };
        if contrast(candidate, background) >= 4.6 {
            high = fraction;
        } else {
            low = fraction;
        }
    }
    gpui::Hsla {
        l: ink.l + (target - ink.l) * high,
        ..ink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn day_mode_ansi_ink_stays_readable_and_night_colors_are_preserved() {
        let light = Theme::light();
        for color in [0xffffff, 0x56b6c2, 0x98c379, 0xd19a66] {
            let ink = rgb(color).into();
            assert!(contrast(visible_ink(ink, light.canvas, light), light.canvas) >= 4.5);
            assert_eq!(visible_ink(ink, Theme::dark().canvas, Theme::dark()), ink);
        }
    }
}
