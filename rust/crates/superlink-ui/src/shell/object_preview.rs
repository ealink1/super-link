use super::*;

/// 对象页的列定义：表头与数据行共用同一份宽度，避免两侧各自收缩导致错位。
/// 名称列是复合单元格（图标 + 文本），单独定义。
const ROW_HEIGHT: f32 = 30.0;
const HEADER_HEIGHT: f32 = 32.0;
const CELL_GAP: f32 = 12.0;
const ROW_PADDING_X: f32 = 12.0;
const NAME_WIDTH: f32 = 220.0;
const NAME_ICON: f32 = 13.0;
const NAME_ICON_GAP: f32 = 8.0;

const COLUMNS: [(&str, f32); 8] = [
    ("类型", 90.0),
    ("行数", 100.0),
    ("数据长度", 100.0),
    ("引擎", 90.0),
    ("创建日期", 210.0),
    ("修改日期", 210.0),
    ("排序规则", 130.0),
    ("备注", 160.0),
];

/// 表格内容宽度 = 名称列 + 各固定列 + 列间距 + 左右内边距。
const fn object_grid_width() -> f32 {
    let mut total = NAME_WIDTH + ROW_PADDING_X * 2.;
    let mut index = 0;
    while index < COLUMNS.len() {
        total += COLUMNS[index].1;
        index += 1;
    }
    // 名称列之后每列（含名称列）之间的间距。
    total + CELL_GAP * COLUMNS.len() as f32
}

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
        self.object_search
            .update(cx, |editor, cx| editor.set_text("", cx));
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

    pub(super) fn object_preview_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
                        "{} · {}",
                        self.profiles
                            .get(conn)
                            .map(|p| p.name.as_str())
                            .unwrap_or(""),
                        database
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .w(px(220.))
                                    .h(px(crate::theme::CONTROL_MIN))
                                    .px_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.surface)
                                    .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                                    .child(self.object_search.clone()),
                            )
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
                                        this.object_search
                                            .update(cx, |editor, cx| editor.set_text("", cx));
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
        match self.db_tables.get(&key) {
            Some(Some(entries)) => {
                let needle = self.object_search.read(cx).text().to_lowercase();
                let tables: Vec<_> = entries
                    .iter()
                    .filter(|entry| {
                        (entry.kind.is_empty() || entry.kind == "table")
                            && (needle.is_empty() || entry.name.to_lowercase().contains(&needle))
                    })
                    .take(10000)
                    .collect();
                panel = panel.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_size(px(11.))
                        .text_color(theme.muted)
                        .child(format!("{} 个表 · 双击打开数据", tables.len())),
                );
                panel = panel.child(self.object_grid(conn, &database, &tables, cx));
            }
            _ => panel = panel.child(div().p_3().child("加载中…")),
        }
        panel.into_any_element()
    }

    /// 纵向滚动 + 横向滚动的列信息表：表头与行都由 `COLUMNS` 生成，列宽一致。
    fn object_grid(
        &self,
        conn: usize,
        database: &str,
        tables: &[&superlink_drivers::TableEntry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut body = div()
            .flex()
            .flex_col()
            .w(px(object_grid_width()))
            .flex_shrink_0()
            .text_size(px(12.))
            .child(self.object_header_row());
        if tables.is_empty() {
            body = body.child(
                div()
                    .px(px(ROW_PADDING_X))
                    .py_2()
                    .text_color(theme.muted)
                    .child("没有表对象"),
            );
        }
        for (row, entry) in tables.iter().enumerate() {
            body = body.child(self.object_row(conn, database, entry, row, cx));
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .child(
                div()
                    .id("object-preview-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_scroll()
                    .scrollbar_width(px(8.))
                    .child(body),
            )
            .into_any_element()
    }

    fn object_header_row(&self) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .h(px(HEADER_HEIGHT))
            .px(px(ROW_PADDING_X))
            .gap(px(CELL_GAP))
            .flex_shrink_0()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .text_size(px(12.))
            .text_color(theme.muted)
            .child(
                div()
                    .w(px(NAME_WIDTH))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child("名称"),
            )
            .children(COLUMNS.iter().map(|(title, width)| {
                div()
                    .w(px(*width))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(*title)
            }))
            .into_any_element()
    }

    fn object_row(
        &self,
        conn: usize,
        database: &str,
        entry: &superlink_drivers::TableEntry,
        row: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let name = entry.name.clone();
        let open_name = entry.name.clone();
        let open_db = database.to_owned();
        let stats = self.table_stats(conn, database, &name);
        let striped = row % 2 == 1;
        let values = [
            "表".to_string(),
            stats
                .and_then(|stats| stats.rows)
                .map(|rows| rows.to_string())
                .unwrap_or_default(),
            stats
                .and_then(|stats| stats.data_length)
                .and_then(crate::shell::table_counts::format_bytes)
                .unwrap_or_default(),
            stats
                .and_then(|stats| stats.engine.clone())
                .unwrap_or_default(),
            stats
                .and_then(|stats| stats.created.clone())
                .unwrap_or_default(),
            stats
                .and_then(|stats| stats.updated.clone())
                .unwrap_or_default(),
            stats
                .and_then(|stats| stats.collation.clone())
                .unwrap_or_default(),
            stats
                .and_then(|stats| stats.comment.clone())
                .unwrap_or_default(),
        ];
        div()
            .id(SharedString::from(format!(
                "preview-{conn}-{database}-{name}"
            )))
            .flex()
            .items_center()
            .h(px(ROW_HEIGHT))
            .px(px(ROW_PADDING_X))
            .gap(px(CELL_GAP))
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border.opacity(0.4))
            .bg(if striped {
                theme.canvas.opacity(0.6)
            } else {
                gpui::transparent_black()
            })
            .cursor_pointer()
            .hover(move |style| style.bg(theme.border.opacity(0.22)))
            .child(
                div()
                    .w(px(NAME_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(NAME_ICON_GAP))
                    .overflow_hidden()
                    .child(icons::icon("superlink/table.svg", NAME_ICON, theme.muted))
                    .child(div().min_w(px(0.)).truncate().child(name.clone())),
            )
            .children(COLUMNS.iter().enumerate().map(|(index, (_, width))| {
                let text = &values[index];
                div()
                    .w(px(*width))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(if index == 0 { theme.muted } else { theme.text })
                    .child(if text.is_empty() {
                        "—".to_string()
                    } else {
                        text.clone()
                    })
            }))
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                    if event.click_count() == 2 {
                        this.open_workbench(conn, open_db.clone(), open_name.clone(), cx);
                    }
                }),
            )
            .into_any_element()
    }
}
