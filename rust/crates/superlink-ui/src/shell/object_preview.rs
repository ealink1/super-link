use super::*;

impl SuperLinkApp {
    pub(super) fn open_object_preview(
        &mut self,
        conn: usize,
        database: String,
        cx: &mut Context<Self>,
    ) {
        if self.object_preview.as_ref() == Some(&(conn, database.clone())) {
            return;
        }
        if !self.park_workbench(cx) {
            return;
        }
        self.last_object_scope = Some((conn, database.clone()));
        self.object_preview = Some((conn, database.clone()));
        self.selected = conn;
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.db_tables.entry((conn, database.clone()))
        {
            entry.insert(None);
            self.fetch_tables(conn, database, cx);
        }
        cx.notify();
    }

    pub(super) fn show_object_tab(&mut self, cx: &mut Context<Self>) {
        let scope = self
            .workbench
            .as_ref()
            .map(|bench| (bench.conn, bench.database.clone()))
            .or_else(|| {
                self.last_object_scope
                    .clone()
                    .filter(|(conn, _)| *conn == self.selected)
            })
            .unwrap_or_else(|| {
                let database = self
                    .conn_dbs
                    .get(&self.selected)
                    .and_then(|databases| databases.as_ref())
                    .and_then(|databases| databases.first())
                    .cloned()
                    .unwrap_or_default();
                (self.selected, database)
            });
        self.open_object_preview(scope.0, scope.1, cx);
    }

    pub(super) fn object_preview_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((conn, database)) = &self.object_preview else {
            return div().into_any_element();
        };
        let conn = *conn;
        if self.profiles.get(conn).is_none() {
            return div()
                .flex_1()
                .p_3()
                .text_size(px(12.))
                .text_color(self.theme.muted)
                .child("请选择连接与数据库查看对象")
                .into_any_element();
        }
        let database = database.clone();
        let theme = self.theme;
        let refresh_db = database.clone();
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .bg(theme.surface)
            .text_color(theme.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(38.))
                    .px_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(px(12.))
                    .child(format!(
                        "表对象 · {} · {}",
                        self.profiles
                            .get(conn)
                            .map(|p| p.name.as_str())
                            .unwrap_or(""),
                        database
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .id("refresh-object-preview")
                                    .cursor_pointer()
                                    .child("刷新")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let key = (conn, refresh_db.clone());
                                        if !matches!(this.db_tables.get(&key), Some(None)) {
                                            this.db_tables.insert(key, None);
                                            this.fetch_tables(conn, refresh_db.clone(), cx);
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                div()
                                    .id("close-object-preview")
                                    .cursor_pointer()
                                    .child("返回查询")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.object_preview = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        let key = (conn, database.clone());
        if let Some(error) = self
            .column_errors
            .get(&(conn, database.clone(), String::new()))
        {
            panel = panel.child(div().p_3().child(error.clone()));
        }
        let mut list = div()
            .id("object-preview-list")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .text_size(px(12.));
        match self.db_tables.get(&key) {
            Some(Some(entries)) => {
                let needle = self.sql_search.read(cx).text().to_lowercase();
                let tables: Vec<_> = entries
                    .iter()
                    .filter(|entry| {
                        (entry.kind.is_empty() || entry.kind == "table")
                            && (needle.is_empty() || entry.name.to_lowercase().contains(&needle))
                    })
                    .collect();
                list = list.child(
                    div()
                        .flex()
                        .h(px(30.))
                        .items_center()
                        .px_3()
                        .gap_3()
                        .bg(theme.canvas)
                        .border_b_1()
                        .border_color(theme.border)
                        .child(div().flex_1().child("名称"))
                        .child(div().w(px(80.)).child("类型"))
                        .child(div().w(px(100.)).child("行数")),
                );
                if tables.is_empty() {
                    list = list.child(div().p_3().text_color(theme.muted).child("没有表对象"));
                }
                for entry in tables.iter().take(10000) {
                    let name = entry.name.clone();
                    let db = database.clone();
                    list = list.child(
                        div()
                            .id(SharedString::from(format!(
                                "preview-{conn}-{database}-{name}"
                            )))
                            .flex()
                            .items_center()
                            .h(px(28.))
                            .px_3()
                            .gap_3()
                            .border_b_1()
                            .border_color(theme.border.opacity(0.4))
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.border.opacity(0.22)))
                            .child(icons::icon("superlink/table.svg", 13., theme.muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(name.clone()),
                            )
                            .child(div().w(px(80.)).child("表"))
                            .child(
                                div().w(px(100.)).text_color(theme.muted).child(
                                    self.table_row_label(conn, &database, &name)
                                        .unwrap_or_else(|| "—".into()),
                                ),
                            )
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, _window, cx| {
                                    if event.click_count() == 2 {
                                        this.open_workbench(conn, db.clone(), name.clone(), cx);
                                    }
                                },
                            )),
                    );
                }
                panel = panel.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_size(px(11.))
                        .text_color(theme.muted)
                        .child(format!(
                            "{} 个表 · 双击打开数据 · 左侧搜索可查找对象",
                            tables.len()
                        )),
                );
            }
            _ => list = list.child(div().p_3().child("加载中…")),
        }
        panel.child(list).into_any_element()
    }
}
