use super::*;

pub(super) struct ScrollbarDrag {
    handle: gpui::ScrollHandle,
    origin: gpui::Pixels,
    offset: f32,
    max: f32,
    travel: f32,
}

#[derive(Clone, Copy)]
struct Geometry {
    thumb: f32,
    left: f32,
    travel: f32,
    max: f32,
}
impl Geometry {
    fn new(track: f32, viewport: f32, max: f32, offset: f32) -> Self {
        let track = track.max(0.);
        let max = max.max(0.);
        let thumb = if max > 0. {
            (track * viewport / (viewport + max)).max(28.).min(track)
        } else {
            track
        };
        let travel = track - thumb;
        let left = if max > 0. {
            offset.clamp(0., max) / max * travel
        } else {
            0.
        };
        Self {
            thumb,
            left,
            travel,
            max,
        }
    }
    fn offset_at(self, left: f32) -> f32 {
        if self.travel > 0. {
            left.clamp(0., self.travel) / self.travel * self.max
        } else {
            0.
        }
    }
}

impl SuperLinkApp {
    pub(super) fn workbench_scrollbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let handle = self.workbench.as_ref().unwrap().horizontal.clone();
        let paint = handle.clone();
        let vertical = self.workbench.as_ref().unwrap().vertical.clone();
        let theme = self.theme;
        div()
            .id("wb-horizontal-scrollbar")
            .h(px(14.))
            .w_full()
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme.border.opacity(0.5))
            .bg(theme.canvas)
            .cursor(gpui::CursorStyle::Arrow)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    let bounds = handle.bounds();
                    let viewport = f32::from(bounds.size.width);
                    let track = (viewport - 8.).max(0.);
                    let current = -f32::from(handle.offset().x);
                    let g = Geometry::new(
                        track,
                        viewport,
                        f32::from(handle.max_offset().width),
                        current,
                    );
                    if g.max <= 0. || g.travel <= 0. {
                        return;
                    }
                    let x = f32::from(event.position.x - bounds.left()) - 4.;
                    let offset = if x >= g.left && x <= g.left + g.thumb {
                        current
                    } else {
                        g.offset_at(x - g.thumb / 2.)
                    };
                    handle.set_offset(point(px(-offset), handle.offset().y));
                    this.workbench_scroll_drag = Some(ScrollbarDrag {
                        handle: handle.clone(),
                        origin: event.position.x,
                        offset,
                        max: g.max,
                        travel: g.travel,
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let horizontal = paint.clone();
                        let vertical = vertical.clone();
                        // Capture before GPUI's nested scrollers can remap Y to X,
                        // or apply Shift+wheel to both axes.
                        window.on_mouse_event(
                            move |event: &gpui::ScrollWheelEvent, phase, window, cx| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !horizontal.bounds().contains(&event.position)
                                {
                                    return;
                                }
                                let delta = event.delta.pixel_delta(px(24.));
                                let (x, y) = wheel_axes(
                                    f32::from(delta.x),
                                    f32::from(delta.y),
                                    event.modifiers.shift,
                                );
                                horizontal.set_offset(point(
                                    (horizontal.offset().x + px(x))
                                        .clamp(-horizontal.max_offset().width, px(0.)),
                                    horizontal.offset().y,
                                ));
                                vertical.set_offset(point(
                                    vertical.offset().x,
                                    (vertical.offset().y + px(y))
                                        .clamp(-vertical.max_offset().height, px(0.)),
                                ));
                                cx.stop_propagation();
                                window.refresh();
                            },
                        );
                        let viewport = f32::from(paint.bounds().size.width);
                        let g = Geometry::new(
                            f32::from(bounds.size.width) - 8.,
                            viewport,
                            f32::from(paint.max_offset().width),
                            -f32::from(paint.offset().x),
                        );
                        let track = Bounds::new(
                            point(bounds.left() + px(4.), bounds.top() + px(5.)),
                            size((bounds.size.width - px(8.)).max(px(0.)), px(4.)),
                        );
                        let mut background = gpui::fill(track, theme.border.opacity(0.3));
                        background.corner_radii = px(2.).into();
                        window.paint_quad(background);
                        let thumb = Bounds::new(
                            point(track.left() + px(g.left), track.top()),
                            size(px(g.thumb), px(4.)),
                        );
                        let mut quad = gpui::fill(
                            thumb,
                            theme.muted.opacity(if g.max > 0. { 0.55 } else { 0.18 }),
                        );
                        quad.corner_radii = px(2.).into();
                        window.paint_quad(quad);
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    pub(super) fn drag_workbench_scrollbar(
        &mut self,
        event: &gpui::MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            self.workbench_scroll_drag = None;
            return;
        }
        if let Some(drag) = &self.workbench_scroll_drag {
            let offset = (drag.offset
                + f32::from(event.position.x - drag.origin) * drag.max / drag.travel)
                .clamp(0., drag.max);
            drag.handle
                .set_offset(point(px(-offset), drag.handle.offset().y));
            cx.notify();
        }
    }
}

fn wheel_axes(x: f32, y: f32, shift: bool) -> (f32, f32) {
    if shift {
        (if y != 0. { y } else { x }, 0.)
    } else {
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::{Geometry, wheel_axes};
    #[test]
    fn ordinary_wheel_is_vertical_and_shift_wheel_is_horizontal_only() {
        assert_eq!(wheel_axes(0., -48., false), (0., -48.));
        assert_eq!(wheel_axes(0., 48., false), (0., 48.));
        assert_eq!(wheel_axes(0., -48., true), (-48., 0.));
        assert_eq!(wheel_axes(0., 48., true), (48., 0.));
        // Some platforms already convert Shift+wheel into a horizontal delta.
        assert_eq!(wheel_axes(-48., 0., true), (-48., 0.));
        assert_eq!(wheel_axes(-30., -2., false), (-30., -2.));
    }
    #[test]
    fn thumb_drag_reaches_both_ends_and_clamps_outside_track() {
        let g = Geometry::new(592., 600., 1200., 600.);
        assert!((g.thumb - 592. / 3.).abs() < 0.01);
        assert!((g.offset_at(g.left) - 600.).abs() < 0.01);
        assert_eq!(g.offset_at(-100.), 0.);
        assert_eq!(g.offset_at(1000.), 1200.);
    }
    #[test]
    fn fits_and_tiny_viewports_never_divide_by_zero() {
        for (track, viewport, max) in [(592., 600., 0.), (0., 0., 0.), (12., 20., 5000.)] {
            let g = Geometry::new(track, viewport, max, 10000.);
            assert_eq!(g.offset_at(100.), 0.);
            assert!(g.left.is_finite() && g.thumb <= track);
        }
    }
}
