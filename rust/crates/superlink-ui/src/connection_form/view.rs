//! 连接表单主体：窗口式标题栏、居中标签页、居中窄表单与底部按钮栏。
//!
//! 版式对齐 Navicat 的连接对话框：标签页在顶部居中，字段纵向排列、
//! 标签右对齐，底部左侧是 URI 与测试连接，右侧是取消 / 上一步 / 保存。

use super::*;
use crate::theme::TEXT_SIZE;
use gpui::relative;
use widgets::{ButtonKind, Toggle};

impl ConnectionForm {
    pub(super) fn render_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.open {
            return div().into_any_element();
        }
        if self.picker.read(cx).open {
            return self.picker_overlay(window, cx);
        }
        let theme = self.theme;
        let editors = self.editors();
        div()
            .id("connection-form")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .text_color(theme.text)
            .text_size(px(TEXT_SIZE))
            .child(self.title_bar(cx))
            .child(self.tabs(cx))
            .child(
                div()
                    .id("connection-form-scroll")
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .scrollbar_width(px(8.0))
                    .child(
                        div().flex().w_full().justify_center().py_6().child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_5()
                                .w(px(560.0))
                                .max_w(relative(0.92))
                                .child(self.section_body(&editors, cx)),
                        ),
                    ),
            )
            .child(self.footer(cx))
            .into_any_element()
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let action = if self.editing() {
            "编辑连接"
        } else {
            "新建连接"
        };
        let name = self
            .kind_descriptor()
            .map(|descriptor| descriptor.name.to_string())
            .unwrap_or_else(|| self.kind().to_string());
        div()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .h(px(38.0))
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icons::icon(
                        icons::datasource_icon(self.kind()),
                        14.0,
                        theme.accent,
                    ))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{action}（{name}）")),
                    ),
            )
            .child(
                div().absolute().right(px(10.0)).top(px(7.0)).child(
                    div()
                        .id("close-form")
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(24.0))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.accent_soft))
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
                        .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                ),
            )
            .into_any_element()
    }

    fn tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .h(px(40.0))
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .children(Section::ALL.map(|item| {
                let active = item == self.current_section();
                div()
                    .id(SharedString::from(format!("section-{}", item.label())))
                    .px_4()
                    .py(px(5.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.surface })
                    .hover(move |style| {
                        style.bg(if active {
                            theme.accent
                        } else {
                            theme.accent_soft
                        })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.set_section(item, cx)))
                    .child(
                        div()
                            .text_size(px(TEXT_SIZE))
                            .font_weight(if active {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if active { theme.surface } else { theme.muted })
                            .child(item.label()),
                    )
            }))
            .into_any_element()
    }

    fn section_body(&self, editors: &Editors, cx: &mut Context<Self>) -> AnyElement {
        match self.current_section() {
            Section::Basic => div()
                .flex()
                .flex_col()
                .gap_4()
                .child(self.identity_block())
                .child(self.basic_section(editors, cx))
                .into_any_element(),
            Section::Advanced => self.advanced_section(editors, cx),
            Section::Database => self.database_section(editors, cx),
            Section::Ssl => self.ssl_section(editors, cx),
            Section::Ssh => self.ssh_section(cx),
            Section::Proxy => self.proxy_section(cx),
            Section::Appearance => self.appearance_section(cx),
        }
    }

    /// 顶部「SuperLink —— 数据源」示意，与 Navicat 的图标链条对应。
    fn identity_block(&self) -> AnyElement {
        let theme = self.theme;
        let name = self
            .kind_descriptor()
            .map(|descriptor| descriptor.name.to_string())
            .unwrap_or_else(|| self.kind().to_string());
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(icons::icon("superlink/database.svg", 44.0, theme.muted))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.muted)
                                    .child("SuperLink"),
                            ),
                    )
                    .child(div().w(px(96.0)).h(px(1.0)).mt(px(22.0)).bg(theme.border))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(icons::icon(
                                icons::datasource_icon(self.kind()),
                                44.0,
                                theme.text,
                            ))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(name),
                            ),
                    ),
            )
            .child(
                div()
                    .max_w(px(430.0))
                    .text_center()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child(self.protocol_hint()),
            )
            .into_any_element()
    }

    fn basic_section(&self, editors: &Editors, cx: &mut Context<Self>) -> AnyElement {
        let local = matches!(self.kind(), "sqlite" | "duckdb");
        let custom = self.kind() == "custom";
        let flags = self.flags();
        let mut form = div().flex().flex_col().gap_1();
        form = form.child(self.input_row("连接名称", &editors.name, cx));
        if local {
            let browse = div()
                .id("pick-database-file")
                .flex()
                .items_center()
                .justify_center()
                .h(px(30.0))
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(self.theme.border)
                .cursor_pointer()
                .text_size(px(12.0))
                .hover(move |style| style.bg(self.theme.accent_soft))
                .on_click(cx.listener(|this, _, _, cx| this.pick_database_file(cx)))
                .child("选择文件");
            let control = div()
                .flex()
                .items_center()
                .gap_2()
                .child(self.input_box(&editors.database).w(px(220.0)))
                .child(browse);
            form = form.child(self.form_row("数据库文件", control));
        } else {
            form = form
                .child(self.input_row("主机", &editors.host, cx))
                .child(self.input_row_w("端口", &editors.port, 120.0, cx))
                .child(self.input_row("用户名", &editors.user, cx))
                .child(self.input_row("密码", &editors.password, cx));
        }
        if custom {
            form = form
                .child(self.input_row("驱动", &editors.driver, cx))
                .child(self.input_row("DSN", &editors.dsn, cx));
        }
        form.child(
            div()
                .ml(px(92.0))
                .flex()
                .flex_col()
                .gap_1()
                .child(self.check_row(
                    "connection-password-remember",
                    "保存密码",
                    flags.remember_password,
                    Toggle::Remember,
                    cx,
                ))
                .child(self.check_row(
                    "connection-connect-after-save",
                    "保存后展开",
                    flags.connect_after_save,
                    Toggle::ConnectAfter,
                    cx,
                )),
        )
        .into_any_element()
    }

    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let mut bar = div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .h(px(48.0))
            .child(
                self.button("URI", ButtonKind::Ghost)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_section(Section::Advanced, cx);
                    })),
            )
            .child(
                self.button(
                    if self.testing() {
                        "测试中…"
                    } else {
                        "测试连接"
                    },
                    ButtonKind::Ghost,
                )
                .on_click(cx.listener(|this, _, _, cx| this.test(cx))),
            )
            .child(div().flex_1())
            .child(
                self.button("取消", ButtonKind::Ghost)
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            );
        if !self.editing() {
            bar = bar.child(
                self.button("上一步", ButtonKind::Ghost)
                    .on_click(cx.listener(|this, _, window, cx| this.back_to_picker(window, cx))),
            );
        }
        let mut messages = div().flex().flex_col().gap_1().px_4().pb_2();
        if let Some(error) = self.error() {
            messages = messages.child(
                div()
                    .text_size(px(TEXT_SIZE))
                    .text_color(theme.danger)
                    .child(error),
            );
        }
        if let Some(notice) = self.notice() {
            messages = messages.child(
                div()
                    .text_size(px(TEXT_SIZE))
                    .text_color(theme.success)
                    .child(notice),
            );
        }
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme.border)
            .children((self.error().is_some() || self.notice().is_some()).then_some(messages))
            .child(
                bar.child(
                    self.button("保存", ButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                ),
            )
            .into_any_element()
    }
}

/// 把 #RRGGBB 转成 gpui 颜色；非法值退回灰色。
pub(super) fn hex_color(value: &str) -> gpui::Hsla {
    let text = value.trim().trim_start_matches('#');
    u32::from_str_radix(text, 16)
        .ok()
        .filter(|_| text.len() == 6)
        .map(|rgb| gpui::rgb(rgb).into())
        .unwrap_or_else(|| gpui::rgb(0x9ca3af).into())
}
