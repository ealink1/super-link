use super::*;

impl TerminalPage {
    /// Card grid shown while no session is open, mirroring the Go layout.
    pub(super) fn host_cards(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let matches: Vec<(usize, &ShellHost)> = self
            .host_list
            .iter()
            .enumerate()
            .filter(|(_, host)| self.host_matches(host))
            .collect();

        if matches.is_empty() {
            return div()
                .id("host-cards")
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .overflow_y_scroll()
                .child(icons::icon("shell/server.svg", 44.0, theme.muted))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text)
                        .child("暂无主机"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.muted)
                        .child("新建主机，或调整搜索与筛选条件"),
                );
        }

        div()
            .id("host-cards")
            .flex()
            .flex_wrap()
            .content_start()
            .items_start()
            .gap_4()
            .p_4()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .children(
                matches
                    .into_iter()
                    .map(|(index, host)| self.host_card(index, host, cx)),
            )
    }

    pub(super) fn host_card(
        &self,
        index: usize,
        host: &ShellHost,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        let name = SharedString::from(host.name.clone());
        let address = SharedString::from(if self.privacy {
            "••••@••••••".to_string()
        } else {
            format!("{}@{}:{}", host.user, host.host, host.port)
        });
        let notes = SharedString::from(
            host.notes
                .lines()
                .next()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.to_string())
                .unwrap_or_else(|| "暂无备注".into()),
        );
        let tags = SharedString::from(if host.tags.trim().is_empty() {
            "无标签".to_string()
        } else {
            host.tags.clone()
        });
        let card = div()
            .id(("host-card", index))
            .relative()
            .flex()
            .flex_col()
            .p_4()
            .rounded_lg()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .hover(move |style| style.border_color(theme.accent))
            .children((self.host_menu == Some(index)).then(|| self.host_actions_menu(index, cx)));
        if self.list_mode {
            return card.w_full().h(px(120.0)).child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(40.0))
                            .rounded_md()
                            .bg(theme.button)
                            .child(icons::icon("shell/terminal.svg", 20.0, theme.text)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text)
                                            .child(name),
                                    )
                                    .child(protocol_badge(theme)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.muted)
                                    .child(address),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.muted)
                                    .child(notes),
                            ),
                    )
                    .child(self.host_menu_button(index, cx))
                    .child(
                        div()
                            .id(("quick-connect", index))
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(120.0))
                            .h(px(30.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme.button)
                            .border_1()
                            .border_color(theme.border)
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.connect_host_at(index, window, cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text)
                                    .child("快速连接"),
                            ),
                    ),
            );
        }
        card.w(px(300.0))
            .h(px(260.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(44.0))
                            .rounded_md()
                            .bg(theme.button)
                            .child(icons::icon("shell/terminal.svg", 22.0, theme.text)),
                    )
                    .child(self.host_menu_button(index, cx)),
            )
            .child(div().h(px(14.0)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(name),
                    )
                    .child(protocol_badge(theme)),
            )
            .child(div().h(px(6.0)))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(address),
            )
            .child(div().h(px(8.0)))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child(notes),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child(tags),
            )
            .child(div().h(px(14.0)))
            .child(
                div()
                    .id(("quick-connect", index))
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(30.0))
                    .w_full()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.button)
                    .border_1()
                    .border_color(theme.border)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.connect_host_at(index, window, cx)
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("快速连接"),
                    ),
            )
    }
}
