use super::*;

impl TerminalPage {
    pub(super) fn host_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_1()
            .text_size(px(13.))
            .text_color(theme.text)
            .items_start()
            .justify_center()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .max_h(gpui::relative(0.9))
                    .id("host-form-scroll")
                    .overflow_y_scroll()
                    .gap_3()
                    .p_6()
                    .mt_6()
                    .w(px(560.0))
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(icons::icon("shell/server.svg", 16.0, theme.muted))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text)
                                            .child(if self.editing_host.is_some() {
                                                "编辑主机"
                                            } else {
                                                "新建主机"
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("close-host-form")
                                    .px_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.close_host_form(cx)
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .children(self.editing_host.as_ref().map(|_| {
                        div()
                            .id("delete-edited-host")
                            .cursor_pointer()
                            .text_color(theme.danger)
                            .child(if self.confirm_host_delete {
                                "确认删除主机配置"
                            } else {
                                "删除主机配置"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.delete_edited_host(cx)))
                    }))
                    .child(self.form_row("名称", &self.form_name))
                    .child(self.form_row("主机", &self.form_host))
                    .child(self.form_row("端口", &self.form_port))
                    .child(self.form_row("用户名", &self.form_user))
                    .child(div().flex().gap_3().children(
                        [(false, "密码认证"), (true, "密钥认证")].into_iter().map(
                            |(key_mode, label)| {
                                div()
                                    .id(("host-auth-mode", key_mode as usize))
                                    .cursor_pointer()
                                    .px_3()
                                    .py_2()
                                    .rounded_md()
                                    .bg(if self.form_key_mode == key_mode {
                                        theme.accent_soft
                                    } else {
                                        theme.button
                                    })
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.form_key_mode = key_mode;
                                        cx.notify();
                                    }))
                            },
                        ),
                    ))
                    .when(!self.form_key_mode, |form| {
                        form.child(self.form_row("密码", &self.form_password))
                    })
                    .when(self.form_key_mode, |form| {
                        form.child(self.form_row("私钥路径", &self.form_key_path))
                            .child(
                                div()
                                    .id("host-pick-key")
                                    .cursor_pointer()
                                    .text_color(theme.accent)
                                    .child("选择私钥文件…")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.pick_private_key(cx)),
                                    ),
                            )
                            .child(self.form_row("私钥口令", &self.form_passphrase))
                    })
                    .children(self.editing_host.as_ref().map(|_| {
                        div()
                            .text_size(px(11.))
                            .text_color(theme.muted)
                            .child("编辑时留空会保留已保存的密码或私钥口令")
                    }))
                    .child(
                        div()
                            .id("host-remember")
                            .cursor_pointer()
                            .child(if self.form_remember {
                                "☑ 记住密码和私钥口令（加密保存在本机）"
                            } else {
                                "☐ 记住密码和私钥口令（否则仅本次运行可用）"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.form_remember = !this.form_remember;
                                cx.notify();
                            })),
                    )
                    .child(
                        div().flex().gap_2().children(
                            ["Linux", "macOS", "Windows", "其他"]
                                .into_iter()
                                .map(|system| {
                                    div()
                                        .id(SharedString::from(format!("host-system-{system}")))
                                        .cursor_pointer()
                                        .px_2()
                                        .py_1()
                                        .bg(if self.form_system == system {
                                            theme.accent_soft
                                        } else {
                                            theme.button
                                        })
                                        .child(system)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.form_system = system.into();
                                            cx.notify();
                                        }))
                                }),
                        ),
                    )
                    .child(self.form_row("分组", &self.form_group))
                    .child(self.form_row("标签", &self.form_tags))
                    .child(self.form_row("备注", &self.form_notes))
                    .children(self.form_error.clone().map(|error| {
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if error.starts_with("测试成功") {
                                theme.success
                            } else if error.starts_with("正在") {
                                theme.muted
                            } else {
                                theme.danger
                            })
                            .child(error)
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .id("test-host")
                                    .cursor_pointer()
                                    .px_3()
                                    .py_1()
                                    .child(if self.form_busy {
                                        "测试 / 保存中…"
                                    } else {
                                        "测试连接"
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.test_host(false, cx)),
                                    ),
                            )
                            .children(self.probe_pending.as_ref().map(|_| {
                                div()
                                    .id("confirm-probe-fingerprint")
                                    .cursor_pointer()
                                    .text_color(theme.accent)
                                    .child("已核对指纹，仅本次信任并测试")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.test_host(true, cx)),
                                    )
                            }))
                            .child(
                                div()
                                    .id("cancel-host")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.close_host_form(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child("取消"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("save-host")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .on_click(
                                        cx.listener(|this, _event, _window, cx| this.save_host(cx)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.surface)
                                            .child("保存"),
                                    ),
                            ),
                    ),
            )
    }

    pub(super) fn form_row(&self, label: &str, editor: &Entity<SqlEditor>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(64.0))
                    .flex_shrink_0()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .px_2()
                    .h(px(30.0))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.canvas)
                    .text_color(theme.text)
                    .child(editor.clone()),
            )
    }
}
