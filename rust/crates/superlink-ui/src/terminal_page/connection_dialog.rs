use super::*;
use superlink_terminal::{ConnectStage, ConnectState};

fn stage_index(stage: ConnectStage) -> usize {
    match stage {
        ConnectStage::Tcp => 0,
        ConnectStage::Auth => 1,
        ConnectStage::Channel => 2,
        ConnectStage::Ready => 3,
    }
}

impl TerminalPage {
    pub(super) fn connection_progress(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let blue = gpui::rgb(if theme.canvas.l > 0.5 {
            0x4b70cf
        } else {
            0x7d9def
        });
        let green = gpui::rgb(if theme.canvas.l > 0.5 {
            0x4c8127
        } else {
            0x9ecb66
        });
        let pink = gpui::rgb(if theme.canvas.l > 0.5 {
            0xd9486a
        } else {
            0xf57094
        });
        let labels = ["建立连接", "身份验证", "打开通道", "连接就绪"];
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000044))
            .occlude()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(512.))
                    .h(px(400.))
                    .max_w(relative(0.94))
                    .max_h(relative(0.94))
                    .rounded_lg()
                    .bg(theme.canvas)
                    .border_1()
                    .border_color(theme.border)
                    .text_color(theme.text)
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .h(px(58.))
                            .flex_shrink_0()
                            .child(icons::icon(
                                "shell/network.svg",
                                24.,
                                if self.connecting {
                                    theme.accent
                                } else {
                                    theme.danger
                                },
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(15.))
                                    .child(if self.connecting {
                                        "SSH 连接中…"
                                    } else {
                                        "SSH 连接失败"
                                    }),
                            )
                            .child(
                                div()
                                    .id("close-shell-connect-dialog")
                                    .cursor_pointer()
                                    .p_1()
                                    .child(icons::icon("shell/x.svg", 16., theme.muted))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.dismiss_connection(cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .relative()
                            .px_5()
                            .pb_4()
                            .flex_shrink_0()
                            .child(
                                div()
                                    .absolute()
                                    .top(px(15.))
                                    .left(px(40.))
                                    .right(px(40.))
                                    .h(px(1.))
                                    .bg(theme.border),
                            )
                            .children(labels.into_iter().enumerate().map(|(i, label)| {
                                let color = match self.connect_steps[i] {
                                    Some(ConnectState::Completed) => green.into(),
                                    Some(ConnectState::Started) => blue.into(),
                                    Some(ConnectState::Failed) => pink.into(),
                                    None => theme.muted,
                                };
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .size(px(30.))
                                            .rounded_full()
                                            .bg(if self.connect_steps[i].is_some() {
                                                color
                                            } else {
                                                theme.button
                                            })
                                            .child(match self.connect_steps[i] {
                                                Some(ConnectState::Completed) => icons::icon(
                                                    "shell/check.svg",
                                                    16.,
                                                    gpui::rgb(0xffffff).into(),
                                                )
                                                .into_any_element(),
                                                _ => div()
                                                    .text_color(
                                                        if self.connect_steps[i].is_some() {
                                                            gpui::rgb(0xffffff).into()
                                                        } else {
                                                            color
                                                        },
                                                    )
                                                    .child((i + 1).to_string())
                                                    .into_any_element(),
                                            }),
                                    )
                                    .child(div().text_color(color).child(label))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.))
                            .m_4()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(icons::icon("shell/terminal.svg", 14., theme.muted))
                                    .child("连接日志"),
                            )
                            .child(
                                div()
                                    .id("ssh-connection-log")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h(px(0.))
                                    .overflow_y_scroll()
                                    .gap_2()
                                    .p_3()
                                    .children(self.connect_log.iter().map(|event| {
                                        let (message, color): (&str, gpui::Hsla) = match event.state
                                        {
                                            ConnectState::Started => (
                                                descriptions(stage_index(event.stage))[0],
                                                blue.into(),
                                            ),
                                            ConnectState::Completed => (
                                                descriptions(stage_index(event.stage))[1],
                                                green.into(),
                                            ),
                                            ConnectState::Failed => {
                                                ("此阶段未完成，连接已停止", pink.into())
                                            }
                                        };
                                        div().text_color(color).child(format!(
                                            "[{}] {message}",
                                            labels[stage_index(event.stage)]
                                        ))
                                    }))
                                    .children(
                                        self.error
                                            .clone()
                                            .map(|e| div().text_color(theme.danger).child(e)),
                                    )
                                    .children(self.changed_key.as_ref().map(
                                        |(expected, observed)| {
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .text_size(px(11.))
                                                .text_color(theme.muted)
                                                .child(format!("已保存指纹：{expected}"))
                                                .child(format!("服务器指纹：{observed}"))
                                                .child("连接未获授权，已保存的指纹保持不变。")
                                        },
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .h(px(52.))
                            .flex_shrink_0()
                            .border_t_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(11.))
                                    .text_color(theme.muted)
                                    .child(if self.connecting {
                                        "连接日志仅显示阶段说明"
                                    } else {
                                        "请检查主机、网络、认证方式或指纹"
                                    }),
                            )
                            .when(!self.connecting, |footer| {
                                footer.child(
                                    div()
                                        .id("retry-shell-connect")
                                        .cursor_pointer()
                                        .px_3()
                                        .py_2()
                                        .rounded_md()
                                        .bg(blue)
                                        .text_color(gpui::rgb(0xffffff))
                                        .child("重新连接")
                                        .on_click(cx.listener(|this, _, _, cx| this.reconnect(cx))),
                                )
                            })
                            .child(
                                div()
                                    .id("dismiss-shell-connect")
                                    .cursor_pointer()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .bg(pink)
                                    .text_color(gpui::rgb(0xffffff))
                                    .child(if self.connecting {
                                        "取消连接"
                                    } else {
                                        "关闭"
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.dismiss_connection(cx)),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn dismiss_connection(&mut self, cx: &mut Context<Self>) {
        if self.connecting {
            self.disconnect(cx);
        }
        self.connect_dialog = false;
        cx.notify();
    }
}

fn descriptions(stage: usize) -> [&'static str; 2] {
    [
        ["正在建立 TCP 连接", "TCP 连接已建立"],
        ["正在核验主机身份并验证凭据", "SSH 身份验证已通过"],
        ["正在申请终端通道与交互式 Shell", "终端通道已打开"],
        ["正在准备终端会话", "SSH 连接就绪"],
    ][stage]
}
