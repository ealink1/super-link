use super::*;
use superlink_terminal::{ConnectEvent, ConnectStage};

/// 侧栏开合时长：太短会显得生硬，200ms 左右滑动才看得清。
const PANEL_DURATION: Duration = Duration::from_millis(200);

pub(super) fn this_progress(page: &mut TerminalPage, event: ConnectEvent) {
    let index = match event.stage {
        ConnectStage::Tcp => 0,
        ConnectStage::Auth => 1,
        ConnectStage::Channel => 2,
        ConnectStage::Ready => 3,
    };
    page.connect_steps[index] = Some(event.state);
    if page.connect_log.len() < 32 {
        page.connect_log.push(event);
    }
    page.status = event.stage.label().into();
}

impl TerminalPage {
    pub(super) fn reconnect(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else {
            return;
        };
        self.disconnect(cx);
        self.auto_reconnect = true;
        self.begin(target, false, cx);
    }

    pub(super) fn session_footer(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let remote = self.transport.is_some();
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(36.))
            .px_2()
            .border_t_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .text_size(px(12.))
            .child(
                div()
                    .id("session-files")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .text_color(if remote { theme.text } else { theme.muted })
                    .child(icons::icon(
                        "shell/folder.svg",
                        14.,
                        if remote { theme.text } else { theme.muted },
                    ))
                    .child("SFTP")
                    .when(remote, |button| button.cursor_pointer())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.transport.is_some() {
                            this.toggle_panel(auxiliary::Mode::Files, cx);
                        }
                    })),
            )
            .child(self.session_button("session-reconnect", "重连", Self::reconnect, cx))
            .child(
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(if self.session.is_some() {
                        theme.success
                    } else if self.connecting {
                        theme.accent
                    } else {
                        theme.danger
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(self.status.clone()),
            )
            .child(self.session_button(
                "session-font-minus",
                "−",
                |this, cx| this.adjust_font(-1, cx),
                cx,
            ))
            .child(div().text_color(theme.muted).child("字号"))
            .child(self.session_button(
                "session-font-plus",
                "+",
                |this, cx| this.adjust_font(1, cx),
                cx,
            ))
            .into_any_element()
    }
    pub(crate) fn indicator_color(&self) -> gpui::Hsla {
        if self.session.is_some() {
            self.theme.success
        } else if self.connecting {
            self.theme.accent
        } else {
            self.theme.danger
        }
    }
    pub(super) fn hide_panel(&mut self, cx: &mut Context<Self>) {
        if self.auxiliary.mode == auxiliary::Mode::Terminal {
            return;
        }
        // 收起时先留着旧模式，动画播完再切回终端，否则面板会直接消失。
        self.auxiliary.collapsed = Some(self.auxiliary.mode);
        self.needs_focus = true;
        self.animate_panel(0., cx);
    }
    pub(super) fn restore_panel(&mut self, cx: &mut Context<Self>) {
        let Some(mode) = self.auxiliary.collapsed else {
            return;
        };
        self.open_panel(mode, cx);
    }
    /// 打开的侧栏模式并带缓动展开；重复调用不会重放动画。
    pub(super) fn open_panel(&mut self, mode: auxiliary::Mode, cx: &mut Context<Self>) {
        self.auxiliary.collapsed = None;
        self.auxiliary.mode = mode;
        if mode == auxiliary::Mode::Files {
            self.list_files(cx);
        }
        if mode == auxiliary::Mode::Monitor {
            self.refresh_monitor(cx);
        }
        self.animate_panel(1., cx);
    }
    /// 每帧推进一点开合进度，直到到达目标；动画结束时再落定模式。
    fn animate_panel(&mut self, target: f32, cx: &mut Context<Self>) {
        if self.panel_target == target && self.panel_start.is_none() {
            return;
        }
        self.panel_from = self.panel_current();
        self.panel_target = target;
        self.panel_start = Some(Instant::now());
        cx.notify();
    }
    /// 按真实时间连续插值，而不是每帧跳固定比例，否则动画会一卡一卡。
    /// 没有动画在跑时，开合状态直接由当前模式决定。
    fn panel_current(&self) -> f32 {
        let Some(start) = self.panel_start else {
            return if self.auxiliary.mode == auxiliary::Mode::Terminal {
                0.
            } else {
                1.
            };
        };
        let elapsed = start.elapsed().as_secs_f32() / PANEL_DURATION.as_secs_f32();
        if elapsed >= 1. {
            return self.panel_target;
        }
        let eased = panel_ease(elapsed);
        self.panel_from + (self.panel_target - self.panel_from) * eased
    }
    pub(super) fn panel_progress(&mut self) -> f32 {
        let progress = self.panel_current();
        if let Some(start) = self.panel_start
            && start.elapsed() >= PANEL_DURATION
        {
            self.panel_start = None;
            self.panel_from = self.panel_target;
            self.settle_panel();
        }
        progress
    }
    fn settle_panel(&mut self) {
        if self.panel_target <= 0. {
            self.auxiliary.mode = auxiliary::Mode::Terminal;
        } else {
            self.auxiliary.collapsed = None;
        }
    }
    fn session_button(
        &self,
        id: &'static str,
        title: &'static str,
        action: fn(&mut Self, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .hover(move |s| s.bg(theme.accent_soft))
            .flex()
            .items_center()
            .gap_1()
            .when(id == "session-reconnect", |button| {
                button.child(icons::icon("shell/refresh-cw.svg", 15., theme.muted))
            })
            .child(title)
            .on_click(cx.listener(move |this, _, _, cx| action(this, cx)))
            .into_any_element()
    }
    fn adjust_font(&mut self, delta: i64, cx: &mut Context<Self>) {
        let size = (self.settings.shell_font_size() as i64 + delta).clamp(
            superlink_app::MIN_SHELL_FONT_SIZE as i64,
            superlink_app::MAX_SHELL_FONT_SIZE as i64,
        );
        self.settings.set("shell.fontSize", serde_json::json!(size));
        cx.notify();
    }
    pub(super) fn toggle_panel(&mut self, mode: auxiliary::Mode, cx: &mut Context<Self>) {
        if self.auxiliary.mode == mode && self.auxiliary.collapsed.is_none() {
            self.hide_panel(cx);
            return;
        }
        self.open_panel(mode, cx);
    }
}
