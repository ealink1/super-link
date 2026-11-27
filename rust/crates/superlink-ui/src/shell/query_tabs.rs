use super::*;

impl SuperLinkApp {
    pub(super) fn query_workspace_tabs(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme;
        let query_visible = self.workbench.is_none()
            && self.object_preview.is_none()
            && self.structure_edit.is_none();
        let mut tabs = Vec::new();
        for (id, title, selected) in self.query_page.read(cx).workspace_tabs() {
            let active = query_visible && selected;
            let close_id = id.clone();
            let menu_id = tab_menu::TabId::Query(id.clone());
            tabs.push(
                div()
                    .id(SharedString::from(format!("query-workspace-{id}")))
                    .group("sql-tab")
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .h(px(28.))
                    .w(px(150.))
                    .px_3()
                    .mr_2()
                    .rounded_lg()
                    .border_1()
                    .bg(if active {
                        theme.accent.opacity(0.10)
                    } else {
                        theme.muted.opacity(0.10)
                    })
                    .border_color(if active {
                        theme.accent.opacity(0.38)
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(if active {
                            theme.accent.opacity(0.14)
                        } else {
                            theme.muted.opacity(0.16)
                        })
                    })
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.show_sql_tab_menu(menu_id.clone(), event.position, window, cx);
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.park_workbench(cx) {
                            this.query_page
                                .update(cx, |page, cx| page.activate_workspace_tab(&id, cx));
                            cx.notify();
                        }
                    }))
                    .child(icons::icon(
                        "superlink/table.svg",
                        12.,
                        if active { theme.accent } else { theme.muted },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(11.))
                            .text_color(if active { theme.text } else { theme.muted })
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "close-query-workspace-{close_id}"
                            )))
                            .opacity(if active { 1.0 } else { 0.0 })
                            .group_hover("sql-tab", |style| style.opacity(1.0))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.query_page
                                    .update(cx, |page, cx| page.close_workspace_tab(&close_id, cx));
                                cx.notify();
                            }))
                            .child(icons::icon("shell/x.svg", 10., theme.muted)),
                    )
                    .into_any_element(),
            );
        }
        tabs
    }
}
