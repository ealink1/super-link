use super::*;
impl TerminalPage {
    pub(super) fn monitor_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let state = &self.auxiliary.monitor;
        let options = [
            "立即刷新".to_string(),
            if state.paused {
                "恢复自动刷新"
            } else {
                "暂停自动刷新"
            }
            .to_string(),
        ]
        .into_iter()
        .chain([1, 3, 5, 10].map(|seconds| {
            format!(
                "{} {seconds} 秒刷新",
                if state.interval == seconds {
                    "✓"
                } else {
                    " "
                }
            )
        }));
        div()
            .absolute()
            .top(px(40.))
            .right(px(8.))
            .w(px(168.))
            .p_1()
            .flex()
            .flex_col()
            .rounded_md()
            .occlude()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .children(options.enumerate().map(|(index, label)| {
                div()
                    .id(("monitor-context", index))
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.hover))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.auxiliary.monitor.menu = false;
                        match index {
                            0 => this.refresh_monitor(cx),
                            1 => {
                                this.auxiliary.monitor.paused = !this.auxiliary.monitor.paused;
                                if !this.auxiliary.monitor.paused {
                                    this.refresh_monitor(cx);
                                }
                            }
                            _ => {
                                this.auxiliary.monitor.interval = [1, 3, 5, 10][index - 2];
                                if !this.auxiliary.monitor.paused {
                                    this.refresh_monitor(cx);
                                }
                            }
                        }
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }
}
