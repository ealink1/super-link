use super::*;

impl Render for ConnectionForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let title = if self.editing.is_some() {
            "编辑连接"
        } else {
            "新建连接"
        };
        let kind = self.kind.clone();
        let section = self.section;
        let chips = catalog()
            .iter()
            .map(|descriptor| {
                let key = descriptor.key;
                let active = key == kind;
                div()
                    .id(SharedString::from(format!("kind-{key}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_kind(key.to_string(), cx)
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(descriptor.name),
                    )
            })
            .collect::<Vec<_>>();

        let mut dialog = div()
            .id("connection-form-scroll")
            .max_h(window.viewport_size().height - px(48.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .mt_6()
            .w(px(680.0))
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
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .id("close-form")
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| this.close(cx)))
                            .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .max_w(px(620.0))
                    .children(chips),
            )
            .child(
                div().flex().items_center().gap_1().children(
                    [
                        Section::Basic,
                        Section::Tls,
                        Section::Network,
                        Section::Advanced,
                    ]
                    .map(|item| {
                        let active = item == section;
                        div()
                            .id(SharedString::from(format!("section-{:?}", item)))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if active { theme.button } else { theme.canvas })
                            .border_1()
                            .border_color(if active { theme.border } else { theme.canvas })
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.section = item;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(if active { theme.text } else { theme.muted })
                                    .child(item.label()),
                            )
                    }),
                ),
            );

        match section {
            Section::Basic => {
                dialog = dialog
                    .child(self.row("连接名称", &self.name))
                    .child(self.row("主机", &self.host))
                    .child(self.row("端口", &self.port))
                    .child(self.row("用户名", &self.user))
                    .child(self.row("数据库", &self.database))
                    .child(self.row("密码", &self.password))
                    .child(self.check(
                        "remember-connection-password",
                        "记住密码（加密保存）",
                        self.remember_password,
                        |this| this.remember_password = !this.remember_password,
                        cx,
                    ));
            }
            Section::Tls => {
                dialog = dialog
                    .child(self.toggle("启用 SSL/TLS", self.use_ssl, "toggle-ssl", cx))
                    .children(self.use_ssl.then(|| {
                        div().flex().items_center().gap_1().children(SSL_MODES.map(
                            |(key, label)| {
                                let active = self.ssl_mode == key;
                                div()
                                    .id(SharedString::from(format!("ssl-{key}")))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if active { theme.accent } else { theme.canvas })
                                    .border_1()
                                    .border_color(if active { theme.accent } else { theme.border })
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.ssl_mode = key.to_string();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(if active {
                                                theme.surface
                                            } else {
                                                theme.text
                                            })
                                            .child(label),
                                    )
                            },
                        ))
                    }))
                    .children(self.use_ssl.then(|| self.row("CA 证书", &self.ssl_ca)))
                    .children(self.use_ssl.then(|| self.row("客户端证书", &self.ssl_cert)))
                    .children(self.use_ssl.then(|| self.row("客户端私钥", &self.ssl_key)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child("「加密」只验证连通性；「校验 CA」需要填写 CA 证书路径。"),
                    );
            }
            Section::Network => {
                dialog = dialog.child(self.network_view(cx));
            }
            Section::Advanced => {
                dialog = dialog
                    .child(self.row("连接超时（秒）", &self.timeout))
                    .child(self.row("查询超时（秒）", &self.query_timeout))
                    .child(self.row("连接分组", &self.group))
                    .child(self.row("环境", &self.environment))
                    .child(self.check(
                        "connection-readonly",
                        "只读连接",
                        self.read_only,
                        |this| this.read_only = !this.read_only,
                        cx,
                    ))
                    .child(self.row("高级配置 JSON", &self.extra))
                    .child(div().text_size(px(12.0)).text_color(theme.muted).child(
                        "连接超时留空使用默认值；查询超时 0 表示不自动停止。最大 3600 秒。",
                    ));
            }
        }

        dialog = dialog.children(self.error.clone().map(|error| {
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.danger)
                .child(error)
        }));
        dialog = dialog.children(self.notice.clone().map(|notice| {
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.success)
                .child(notice)
        }));

        dialog.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("test-connection")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(theme.border)
                        .on_click(cx.listener(|this, _event, _window, cx| this.test(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if self.testing {
                                    theme.muted
                                } else {
                                    theme.text
                                })
                                .child(if self.testing {
                                    "测试中…"
                                } else {
                                    "测试连接"
                                }),
                        ),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id("cancel-connection")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(theme.border)
                        .on_click(cx.listener(|this, _event, _window, cx| this.close(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child("取消"),
                        ),
                )
                .child(
                    div()
                        .id("save-connection")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(cx.listener(|this, _event, _window, cx| this.save(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("保存"),
                        ),
                ),
        )
    }
}

impl ConnectionForm {
    pub(super) fn row(&self, label: &str, editor: &Entity<SqlEditor>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(96.0))
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

    pub(super) fn toggle(
        &self,
        label: &str,
        on: bool,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.use_ssl = !this.use_ssl;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(if on { theme.accent } else { theme.border })
                    .bg(if on { theme.accent } else { theme.canvas })
                    .children(on.then(|| {
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.surface)
                            .child("✓")
                    })),
            )
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(label.to_string()),
            )
    }
}
