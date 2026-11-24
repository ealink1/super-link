use super::*;
impl ShellWorkspace {
    pub(super) fn welcome(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_1()
            .min_h(px(0.))
            .text_color(theme.text)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(if self.rail_expanded { 144. } else { 64. }))
                    .items_center()
                    .gap_2()
                    .py_3()
                    .child(
                        div()
                            .id("welcome-hosts")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .w(px(if self.rail_expanded { 120. } else { 40. }))
                            .h(px(40.))
                            .rounded_lg()
                            .child(crate::icons::icon("shell/server.svg", 20., theme.muted))
                            .children(self.rail_expanded.then_some("主机"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.home = false;
                                this.picker.update(cx, |p, cx| p.open_hosts(cx));
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("welcome-terminal")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .w(px(if self.rail_expanded { 120. } else { 40. }))
                            .h(px(40.))
                            .rounded_lg()
                            .bg(theme.accent_soft)
                            .child(crate::icons::icon("shell/terminal.svg", 20., theme.accent))
                            .children(self.rail_expanded.then_some("终端")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("welcome-settings")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .w(px(if self.rail_expanded { 120. } else { 40. }))
                            .h(px(40.))
                            .rounded_lg()
                            .child(crate::icons::icon(
                                "superlink/settings.svg",
                                20.,
                                theme.muted,
                            ))
                            .children(self.rail_expanded.then_some("设置"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.home = false;
                                this.picker
                                    .update(cx, |page, cx| page.open_terminal_settings(cx));
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().flex().flex_1().items_center().justify_center().child(
                    div()
                        .flex()
                        .flex_col()
                        .w(px(768.))
                        .max_w(gpui::relative(0.92))
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(
                            div()
                                .p_4()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .border_b_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .min_w(px(0.))
                                        .child(
                                            div()
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_size(px(14.))
                                                .child("创建你的第一个终端会话"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(theme.muted)
                                                .child("支持本地壳与 SSH，每个标签独立运行"),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .text_size(px(11.))
                                        .text_color(theme.muted)
                                        .children(["本地与远程支持", "多标签会话"].map(|label| {
                                            div()
                                                .px_2()
                                                .py_2()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(theme.border)
                                                .child(label)
                                        })),
                                ),
                        )
                        .child(
                            div().flex().gap_4().p_4().text_size(px(12.)).children(
                                [
                                    (
                                        "default",
                                        "本地终端（默认）",
                                        "使用系统默认壳，自动选择本机 Shell",
                                    ),
                                    ("program", "选择本地壳", "从常用壳中选择并立即启动一个会话"),
                                    (
                                        "remote",
                                        "远程 SSH 会话",
                                        "从主机列表中选择目标，快速创建 SSH 会话",
                                    ),
                                ]
                                .into_iter()
                                .map(|(id, title, description)| {
                                    let mut card = div()
                                        .flex()
                                        .flex_col()
                                        .items_start()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .gap_3()
                                        .p_4()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme.canvas)
                                        .child(
                                            div().font_weight(gpui::FontWeight::BOLD).child(title),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .text_color(theme.muted)
                                                .child(description),
                                        );
                                    match id {
                                        "default" => {
                                            card = card.child(
                                                div()
                                                    .id("welcome-local-default")
                                                    .cursor_pointer()
                                                    .text_color(theme.text)
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_sm()
                                                    .bg(theme.accent_soft)
                                                    .border_1()
                                                    .border_color(theme.accent)
                                                    .child("新建本地终端")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open(None, cx)
                                                    })),
                                            )
                                        }
                                        "program" => {
                                            let programs: Vec<&str> = if cfg!(windows) {
                                                vec!["powershell.exe", "cmd.exe"]
                                            } else {
                                                vec!["/bin/zsh", "/bin/bash"]
                                            };
                                            card = card.child(div().flex().gap_3().children(
                                                programs.into_iter().map(|program| {
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "welcome-{program}"
                                                        )))
                                                        .cursor_pointer()
                                                        .px_2()
                                                        .py_1()
                                                        .rounded_sm()
                                                        .bg(theme.button)
                                                        .child(program)
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.open_program(
                                                                    None,
                                                                    Some(program.into()),
                                                                    cx,
                                                                )
                                                            },
                                                        ))
                                                }),
                                            ));
                                        }
                                        _ => {
                                            card = card.child(
                                                div()
                                                    .id("welcome-choose-ssh")
                                                    .cursor_pointer()
                                                    .px_2()
                                                    .py_1()
                                                    .rounded_sm()
                                                    .bg(theme.button)
                                                    .child("选择主机并创建会话")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.home = false;
                                                        cx.notify();
                                                    })),
                                            )
                                        }
                                    }
                                    card
                                }),
                            ),
                        ),
                ),
            )
            .into_any_element()
    }
}
