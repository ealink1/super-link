use super::*;

impl TerminalPage {
    pub(super) fn rail_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        let expanded = self.rail_expanded;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .when(expanded, |style| style.w_full().px_2().justify_start())
            .when(!expanded, |style| style.size(px(40.0)).justify_center())
            .h(px(40.0))
            .rounded_md()
            .cursor_pointer()
            .bg(if active {
                theme.accent_soft
            } else {
                theme.sidebar
            })
            .hover(move |style| style.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _event, window, cx| handler(this, window, cx)))
            .child(icons::icon(
                icon,
                22.0,
                if active { theme.accent } else { theme.muted },
            ))
            .children(if label.is_empty() || !expanded {
                None
            } else {
                Some(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child(label),
                )
            })
    }

    pub(super) fn toggle_rail(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.rail_expanded = !self.rail_expanded;
        cx.notify();
    }

    pub(super) fn show_hosts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.form_open = false;
        self.auxiliary.mode = auxiliary::Mode::Terminal;
        self.refresh_hosts(cx);
        if let Some(handler) = &self.back_handler {
            handler(cx);
        }
        cx.notify();
    }

    pub(super) fn focus_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handler) = &self.terminal_handler {
            handler(cx);
            return;
        }
        self.auxiliary.mode = auxiliary::Mode::Terminal;
        if self.target.is_none() {
            self.connect_local(cx);
        }
        self.focus.focus(window);
        cx.notify();
    }

    pub(super) fn shell_settings_notice(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.auxiliary.mode = auxiliary::Mode::Settings;
        cx.notify();
    }

    /// 主机管理页：头部 + 分组侧栏 + 搜索工具栏 + 卡片网格。
    pub(super) fn hosts_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .h(px(56.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(icons::icon("shell/server.svg", 20.0, theme.text))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("主机管理"),
                    )
                    .child(div().flex_1())
                    .children(self.notice.as_ref().map(|notice| {
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child(notice.clone())
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .id("privacy")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_3()
                            .h(px(32.0))
                            .rounded_md()
                            .cursor_pointer()
                            .border_1()
                            .border_color(theme.border)
                            .bg(if self.privacy {
                                theme.accent_soft
                            } else {
                                theme.surface
                            })
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.privacy = !this.privacy;
                                this.settings
                                    .set("shell.privacy", serde_json::json!(this.privacy));
                                cx.notify();
                            }))
                            .child(icons::icon("shell/eye-off.svg", 14.0, theme.muted))
                            .child("隐私模式"),
                    )
                    .child(
                        div()
                            .id("new-host")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_3()
                            .h(px(32.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme.accent)
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.surface)
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.open_host_form(cx)),
                            )
                            .child(icons::icon("shell/plus.svg", 14.0, theme.surface))
                            .child("新建主机"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.group_sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.hosts_toolbar(cx))
                            .child(self.host_cards(cx)),
                    ),
            )
    }

    pub(super) fn group_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let groups = self.group_counts();
        let mut rows = div().flex().flex_col().gap_1().w_full();
        rows = rows.child(
            div()
                .id("group-all")
                .flex()
                .items_center()
                .gap_2()
                .h(px(30.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .bg(if self.group.is_empty() {
                    theme.accent_soft
                } else {
                    theme.canvas
                })
                .hover(move |style| style.bg(theme.accent_soft))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.group.clear();
                    cx.notify();
                }))
                .child(icons::icon("shell/layout-grid.svg", 14.0, theme.muted))
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child("全部主机"),
                ),
        );
        for (name, count) in groups {
            let selected = self.group == name;
            let value = name.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("group-{name}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.0))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected {
                        theme.accent_soft
                    } else {
                        theme.canvas
                    })
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.group = value.clone();
                        cx.notify();
                    }))
                    .child(icons::icon("shell/folder.svg", 14.0, theme.muted))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(name)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child(format!("{count}")),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .w(px(224.0))
            .h_full()
            .px_3()
            .py_3()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .pb_2()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.muted)
                    .child("分组导航"),
            )
            .child(rows)
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child("标签筛选"),
                    )
                    .child(
                        div()
                            .id("host-tag-all")
                            .cursor_pointer()
                            .text_size(px(12.))
                            .child("全部标签")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.tag.clear();
                                cx.notify();
                            })),
                    )
                    .children(self.host_tags().into_iter().map(|tag| {
                        let selected = self.tag == tag;
                        div()
                            .id(SharedString::from(format!("host-tag-{tag}")))
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .bg(if selected {
                                theme.accent_soft
                            } else {
                                theme.canvas
                            })
                            .text_size(px(12.))
                            .child(tag.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tag = tag.clone();
                                cx.notify();
                            }))
                    })),
            )
    }

    pub(super) fn hosts_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(52.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .w(px(224.0))
                    .h(px(30.0))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                    .child(div().flex_1().min_w(px(0.0)).child(self.search.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w(px(92.0))
                    .h(px(30.0))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child("SSH")
                    .child(icons::icon("shell/chevron-down.svg", 12.0, theme.muted)),
            )
            .child(
                div()
                    .id("reload-hosts")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(30.0))
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.reload_hosts(cx)))
                    .child(icons::icon("shell/refresh-cw.svg", 14.0, theme.muted)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id("hosts-grid")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if self.list_mode {
                                theme.surface
                            } else {
                                theme.accent_soft
                            })
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.list_mode = false;
                                this.settings
                                    .set("shell.listMode", serde_json::json!(false));
                                cx.notify();
                            }))
                            .child(icons::icon("shell/layout-grid.svg", 14.0, theme.muted)),
                    )
                    .child(
                        div()
                            .id("hosts-list")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if self.list_mode {
                                theme.accent_soft
                            } else {
                                theme.surface
                            })
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.list_mode = true;
                                this.settings.set("shell.listMode", serde_json::json!(true));
                                cx.notify();
                            }))
                            .child(icons::icon("shell/list.svg", 14.0, theme.muted)),
                    ),
            )
    }

    pub(super) fn group_counts(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        counts.insert("默认分组".into(), 0);
        for host in &self.host_list {
            *counts.entry(group_label(&host.group)).or_insert(0) += 1;
        }
        counts.into_iter().collect()
    }
}
