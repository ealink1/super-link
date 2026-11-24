use super::*;

impl TerminalPage {
    pub(super) fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if bytes.len() > 1 << 20 {
            self.error = Some("单次粘贴不能超过 1 MiB".into());
            cx.notify();
            return;
        }
        if let Some(queue) = &self.write_queue
            && queue.try_send(bytes.to_vec()).is_err()
        {
            self.error = Some("终端输入队列已满，请稍后重试".into());
            cx.notify();
        }
    }

    pub(super) fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        // While an IME owns the composition, its keys belong to the candidate
        // window: forwarding Enter or arrows here would corrupt the input.
        if self.composing.is_active() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        if modifiers.platform {
            if key == "v" {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
                    && text.len() <= (1 << 20) - 12
                {
                    let bytes = self.screen.paste(&text);
                    self.send(&bytes, cx);
                }
                return;
            }
            if key == "c" {
                self.copy_selection(cx);
                return;
            }
        }
        if modifiers.shift && key == "pageup" {
            self.scroll_offset = (self.scroll_offset + 5).min(self.screen.scrollback_len());
            cx.notify();
            return;
        }
        if modifiers.shift && key == "pagedown" {
            self.scroll_offset = self.scroll_offset.saturating_sub(5);
            cx.notify();
            return;
        }
        if modifiers.control && key == "tab" {
            return;
        }
        if let Some(bytes) = super::input::encode(
            key,
            modifiers.control,
            modifiers.shift,
            modifiers.alt,
            self.screen.application_cursor(),
        ) {
            self.send(&bytes, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let width = if self.rail_expanded { 144.0 } else { 64.0 };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .w(px(width))
            .h_full()
            .py_3()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(self.rail_button(
                "rail-toggle",
                "shell/chevron-right.svg",
                "",
                false,
                cx,
                Self::toggle_rail,
            ))
            .child(div().h(px(8.0)))
            .child(self.rail_button(
                "rail-hosts",
                "shell/server.svg",
                "主机",
                self.session.is_none(),
                cx,
                Self::show_hosts,
            ))
            .child(self.rail_button(
                "rail-terminal",
                "shell/terminal.svg",
                "终端",
                self.session.is_some(),
                cx,
                Self::focus_terminal,
            ))
            .child(div().flex_1())
            .child(self.rail_button(
                "rail-settings",
                "shell/settings.svg",
                "设置",
                false,
                cx,
                Self::shell_settings_notice,
            ))
    }
}
