use super::*;

impl SuperLinkApp {
    /// All SQL surfaces share the object tree; opening a table never replaces it.
    pub(super) fn sql_workspace(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_1()
            .w_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .child(self.sidebar(cx))
            .child(
                div()
                    .id("sidebar-divider")
                    .w(px(5.))
                    .flex_shrink_0()
                    .h_full()
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .bg(self.theme.border)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.sidebar_drag = Some((event.position.x, this.sidebar_width));
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .child(self.sql_table_tabs(cx))
                    .child(if self.structure_edit.is_some() {
                        self.structure_panel(cx).into_any_element()
                    } else if self.object_preview.is_some() {
                        self.object_preview_panel(cx)
                    } else if self.workbench.is_some() {
                        self.workbench_panel(cx)
                    } else {
                        self.query_page.clone().into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn sql_table_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        // 标签栏没有内容时收起，布局与 Shell 一致：空工作区不占一条空条。
        let query_tabs = self.query_page.read(cx).workspace_tabs();
        if query_tabs.is_empty() && self.table_tab_order.is_empty() && self.object_preview.is_none()
        {
            return div().h(px(0.)).overflow_hidden().into_any_element();
        }
        let mut strip = div()
            .id("sql-table-tabs")
            .flex()
            .items_center()
            .h(px(34.))
            .flex_shrink_0()
            .overflow_x_scroll()
            .gap_1()
            .px_2()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border);
        strip = strip.children(self.query_workspace_tabs(cx));
        for id in &self.table_tab_order {
            let Some(bench) = self
                .workbench
                .iter()
                .chain(self.parked_workbenches.iter())
                .find(|b| &b.id == id)
            else {
                continue;
            };
            let active = self.workbench.as_ref().is_some_and(|b| b.id == *id);
            let switch = id.clone();
            let close = id.clone();
            let menu_id = tab_menu::TabId::Table(id.clone());
            let profile = self
                .profiles
                .get(bench.conn)
                .map(|p| p.name.as_str())
                .unwrap_or("已移除连接");
            strip = strip.child(
                div()
                    .id(SharedString::from(format!("table-tab-{id}")))
                    .group("sql-tab")
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .w(px(170.))
                    .h(px(28.))
                    .px_3()
                    .mr_2()
                    .gap(px(6.))
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
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.activate_table_tab(&switch, cx)),
                    )
                    .child(icons::icon(
                        "superlink/table.svg",
                        12.,
                        if active { theme.accent } else { theme.muted },
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(11.))
                            .text_color(theme.text)
                            .child(crate::text::shorten(
                                &format!(
                                    "{}{}@{profile}.{}",
                                    bench.table,
                                    if bench.edits.is_empty()
                                        && !bench
                                            .designer
                                            .as_ref()
                                            .is_some_and(|d| d.read(cx).is_dirty())
                                    {
                                        ""
                                    } else {
                                        " *"
                                    },
                                    bench.database
                                ),
                                22,
                            )),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("close-table-tab-{id}")))
                            .opacity(if active { 1.0 } else { 0.0 })
                            .group_hover("sql-tab", |style| style.opacity(1.0))
                            .child(icons::icon("shell/x.svg", 10., theme.muted))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_table_tab(&close, cx);
                            })),
                    ),
            );
        }
        div()
            .flex()
            .w_full()
            .min_w(px(0.))
            .h(px(34.))
            .flex_shrink_0()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .gap_1()
            .items_center()
            .pr_2()
            .child(
                div()
                    .id("sql-object-tab")
                    .ml_2()
                    .mr_2()
                    .px_3()
                    .h(px(28.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .border_1()
                    .text_size(px(12.))
                    .text_color(if self.object_preview.is_some() {
                        theme.accent
                    } else {
                        theme.muted
                    })
                    .bg(if self.object_preview.is_some() {
                        theme.accent.opacity(0.10)
                    } else {
                        theme.muted.opacity(0.10)
                    })
                    .border_color(if self.object_preview.is_some() {
                        theme.accent.opacity(0.38)
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .hover(move |style| {
                        style.bg(if self.object_preview.is_some() {
                            theme.accent.opacity(0.14)
                        } else {
                            theme.muted.opacity(0.16)
                        })
                    })
                    .child("对象")
                    .on_click(cx.listener(|this, _, _, cx| this.show_object_tab(cx))),
            )
            .child(strip.flex_1().min_w(px(0.)).h_full())
            .child(self.tab_overflow_button(cx))
            .into_any_element()
    }

    pub(super) fn park_workbench(&mut self, cx: &mut Context<Self>) -> bool {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|e| !e.pending.is_empty() || e.busy)
        {
            self.error = Some("请先完成结构变更".into());
            cx.notify();
            return false;
        }
        if let Some(bench) = self.workbench.as_mut()
            && (bench.loading
                || bench.ddl_loading
                || bench
                    .designer
                    .as_ref()
                    .is_some_and(|d| d.read(cx).is_busy()))
        {
            bench.error = Some("请等待当前操作完成后切换标签".into());
            cx.notify();
            return false;
        }
        self.object_preview = None;
        self.structure_edit = None;
        if let Some(mut bench) = self.workbench.take() {
            // A draft editor is shared; staged changes and row selection belong to the table.
            bench.cell = None;
            self.parked_workbenches.push(bench);
        }
        true
    }

    pub(super) fn activate_table_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.workbench.as_ref().is_some_and(|b| b.id == id) {
            return;
        }
        if !self.park_workbench(cx) {
            return;
        }
        if let Some(position) = self.parked_workbenches.iter().position(|b| b.id == id) {
            self.workbench = Some(self.parked_workbenches.remove(position));
            if self
                .workbench
                .as_ref()
                .is_some_and(|b| b.result.is_none() && !b.stale)
            {
                self.fetch_workbench_data(cx);
            }
        }
        cx.notify();
    }

    pub(super) fn close_table_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|e| e.busy || !e.pending.is_empty())
        {
            self.error = Some("请先完成或撤销结构变更，再关闭标签".into());
            cx.notify();
            return;
        }
        if self.workbench.as_ref().is_some_and(|b| b.id == id) {
            if self
                .workbench
                .as_ref()
                .is_some_and(|b| b.loading || b.ddl_loading)
            {
                return;
            }
            self.structure_edit = None;
            self.close_workbench(cx);
        } else if let Some(index) = self.parked_workbenches.iter().position(|b| b.id == id) {
            if !self.parked_workbenches[index].edits.is_empty()
                || self.parked_workbenches[index]
                    .designer
                    .as_ref()
                    .is_some_and(|d| d.read(cx).is_dirty() || d.read(cx).is_busy())
            {
                self.error = Some("该标签有暂存变更，请打开后提交或撤销".into());
            } else {
                self.parked_workbenches.remove(index);
                self.table_tab_order.retain(|key| key != id);
            }
        }
        cx.notify();
    }
}
