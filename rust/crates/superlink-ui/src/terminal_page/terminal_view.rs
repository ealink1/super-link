use super::*;

impl TerminalPage {
    pub(super) fn terminal_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut area = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .bg(theme.canvas)
            .p(px(8.))
            .text_size(px(self.settings.shell_font_size() as f32))
            .track_focus(&self.focus)
            .key_context("Terminal")
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(|this, event, _window, cx| this.on_key(event, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.start_selection(event.position, cx);
                    this.focus.focus(window);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.selecting_text {
                    this.extend_selection(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.selecting_text = false;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(
                cx.listener(|this, event: &gpui::ScrollWheelEvent, window, cx| {
                    let delta = match event.delta {
                        gpui::ScrollDelta::Lines(delta) => delta.y as isize,
                        gpui::ScrollDelta::Pixels(delta) => {
                            (delta.y / window.line_height()).round() as isize
                        }
                    };
                    this.scroll_offset = if delta >= 0 {
                        (this.scroll_offset + delta as usize).min(this.screen.scrollback_len())
                    } else {
                        this.scroll_offset.saturating_sub((-delta) as usize)
                    };
                    this.selection = None;
                    cx.notify();
                }),
            );

        if let Some((host, fingerprint)) = self.pending.clone() {
            area =
                area.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_4()
                        .m_4()
                        .rounded_md()
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(format!("尚未信任 {} 的主机密钥", host.name)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(SharedString::from(fingerprint.clone())),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .id("trust-once")
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(theme.accent_soft)
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.accent)
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.trust(false, cx)
                                        }))
                                        .child("仅本次信任"),
                                )
                                .child(
                                    div()
                                        .id("trust-remember")
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(theme.accent)
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.surface)
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.trust(true, cx)
                                        }))
                                        .child("记住并连接"),
                                ),
                        ),
                );
            return area;
        }

        if let Some(error) = self.error.clone().filter(|_| !self.connect_dialog) {
            area = area.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.danger)
                    .child(error),
            );
        }

        if let Some(composing) = self.composing.preedit().map(str::to_string) {
            area = area.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(rgb(0x1b2028))
                    .border_b_1()
                    .border_color(rgb(0x2a313b))
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(rgb(0xd19a66))
                    .child(SharedString::from(format!("输入中：{composing}"))),
            );
        }

        area.child(ScreenElement {
            page: cx.entity(),
            rows: self.screen.rows(),
            scroll_offset: self.scroll_offset,
            focused: self.session.is_some(),
        })
    }
}
