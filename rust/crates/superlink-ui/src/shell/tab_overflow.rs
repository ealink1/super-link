use super::*;

impl SuperLinkApp {
    pub(super) fn tab_overflow_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .id("sql-tab-overflow")
            .w(px(24.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(18.))
            .text_color(theme.muted)
            .border_l_1()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(move |style| style.bg(theme.accent_soft))
            .child("»")
            .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                this.sql_tab_menu = None;
                this.tab_overflow_position = if this.tab_overflow_position.is_some() {
                    None
                } else {
                    Some(gpui::point(
                        (window.viewport_size().width - px(300.)).max(px(0.)),
                        event.position().y + px(16.),
                    ))
                };
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn tab_overflow_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let position = self.tab_overflow_position?;
        if self.workspace != Workspace::Sql {
            return None;
        }
        let theme = self.theme;
        let query_visible = self.object_preview.is_none()
            && self.workbench.is_none()
            && self.structure_edit.is_none();
        let mut entries: Vec<_> = self
            .query_page
            .read(cx)
            .workspace_tabs()
            .into_iter()
            .map(|(id, title, active)| (tab_menu::TabId::Query(id), title, query_visible && active))
            .collect();
        for id in &self.table_tab_order {
            if let Some(bench) = self
                .workbench
                .iter()
                .chain(self.parked_workbenches.iter())
                .find(|bench| &bench.id == id)
            {
                let profile = self
                    .profiles
                    .get(bench.conn)
                    .map(|p| p.name.as_str())
                    .unwrap_or("");
                entries.push((
                    tab_menu::TabId::Table(id.clone()),
                    format!("{}@{profile}.{}", bench.table, bench.database),
                    self.workbench.as_ref().is_some_and(|bench| &bench.id == id),
                ));
            }
        }
        let mut list = div()
            .id("sql-tab-overflow-list")
            .max_h(px(360.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        if entries.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_color(theme.muted)
                    .child("没有打开的标签"),
            );
        }
        for (index, (id, title, active)) in entries.into_iter().enumerate() {
            list = list.child(
                div()
                    .id(("sql-tab-overflow-item", index))
                    .h(px(28.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .bg(if active {
                        theme.accent_soft
                    } else {
                        theme.surface
                    })
                    .hover(move |style| style.bg(theme.accent_soft))
                    .child(div().w(px(12.)).text_color(theme.accent).child(if active {
                        "✓"
                    } else {
                        ""
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(title)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        match &id {
                            tab_menu::TabId::Query(id) => {
                                if this.park_workbench(cx) {
                                    this.query_page
                                        .update(cx, |page, cx| page.activate_workspace_tab(id, cx));
                                }
                            }
                            tab_menu::TabId::Table(id) => this.activate_table_tab(id, cx),
                        }
                        this.tab_overflow_position = None;
                        cx.notify();
                    })),
            );
        }
        Some(
            div()
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(292.))
                .p_1()
                .rounded_md()
                .shadow_md()
                .occlude()
                .bg(theme.surface)
                .text_size(px(12.))
                .text_color(theme.text)
                .border_1()
                .border_color(theme.border)
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.tab_overflow_position = None;
                    cx.notify();
                }))
                .child(list)
                .into_any_element(),
        )
    }
}
