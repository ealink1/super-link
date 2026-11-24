use super::*;

impl SuperLinkApp {
    pub(super) fn table_block(
        &self,
        index: usize,
        database: String,
        entry: &superlink_drivers::TableEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let object_indent = tree_style::object_indent(&database);
        let table_indent = object_indent + tree_style::INDENT;
        let name = entry.name.clone();
        let is_view = entry.kind == "view";
        let active = self.workbench.as_ref().is_some_and(|b| {
            b.conn == index
                && b.database == database
                && b.table == entry.name
                && b.tab == crate::workbench::WorkbenchTab::Data
        });
        let expanded = self
            .expanded_tables
            .contains(&(index, database.clone(), name.clone()));
        let toggle_name = entry.name.clone();
        let edit_database = database.clone();
        let open_name = entry.name.clone();
        let open_database = database.clone();
        let query_database = database.clone();
        let mut block = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w(px(0.))
            .overflow_hidden();
        block = block.child(
            div()
                .id(SharedString::from(format!("table-{index}-{}", entry.name)))
                .flex()
                .items_center()
                .gap(px(tree_style::GAP))
                .pl(px(table_indent))
                .pr_2()
                .h(px(tree_style::ROW))
                .flex_shrink_0()
                .bg(if active {
                    gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.18)
                } else {
                    gpui::transparent_black()
                })
                .rounded_md()
                .hover(move |style| style.bg(theme.border.opacity(0.22)))
                .child(
                    div()
                        .id(SharedString::from(format!("fold-{index}-{}", entry.name)))
                        .w(px(12.0))
                        .flex_shrink_0()
                        .flex()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.toggle_table(index, edit_database.clone(), toggle_name.clone(), cx)
                        }))
                        .child(icons::icon(
                            if expanded {
                                "shell/chevron-down.svg"
                            } else {
                                "shell/chevron-right.svg"
                            },
                            12.,
                            theme.muted,
                        )),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("open-{index}-{}", entry.name)))
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap(px(tree_style::GAP))
                        .min_w(px(0.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .on_click(cx.listener(
                            move |this, event: &gpui::ClickEvent, _window, cx| {
                                if event.click_count() >= 2 {
                                    this.open_workbench(
                                        index,
                                        open_database.clone(),
                                        open_name.clone(),
                                        cx,
                                    );
                                    cx.notify();
                                    return;
                                }
                                if this.open_protocol_object(index, &query_database, &name, cx) {
                                    return;
                                }
                                if !this.park_workbench(cx) {
                                    return;
                                }
                                if let Some(profile) = this.profiles.get(index).cloned() {
                                    let object = crate::workbench::Workbench::new(
                                        index,
                                        query_database.clone(),
                                        name.clone(),
                                        Vec::new(),
                                    );
                                    if let Ok((sql, _)) =
                                        object.page_statements(profile.sql_dialect())
                                    {
                                        this.workspace = Workspace::Sql;
                                        this.query_page.update(cx, |page, cx| {
                                            page.load_table_query(
                                                profile,
                                                query_database.clone(),
                                                sql,
                                                cx,
                                            )
                                        });
                                    }
                                }
                                cx.notify();
                            },
                        ))
                        .child(icons::icon(
                            if is_view {
                                "superlink/view.svg"
                            } else {
                                "superlink/table.svg"
                            },
                            tree_style::ICON,
                            theme.muted,
                        ))
                        .child(
                            div()
                                .text_size(px(tree_style::FONT))
                                .text_color(theme.text)
                                .child(SharedString::from(entry.name.clone())),
                        )
                        .children(self.table_row_label(index, &database, &entry.name).map(
                            |count| {
                                div()
                                    .text_size(px(10.))
                                    .text_color(theme.muted)
                                    .child(count)
                            },
                        )),
                ),
        );

        if expanded {
            block =
                block.child(self.table_sections(index, &database, &entry.name, table_indent, cx));
        }
        block.into_any_element()
    }

    pub(super) fn conn_subtree(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let mut subtree = div().flex().flex_col().flex_shrink_0();
        match self.conn_dbs.get(&index) {
            None | Some(None) => {
                subtree = subtree.child(
                    div()
                        .pl(px(30.))
                        .text_size(px(tree_style::FONT))
                        .text_color(theme.muted)
                        .child("加载中…"),
                );
            }
            Some(Some(databases)) if databases.is_empty() => {
                if let Some(note) = self.conn_db_note.get(&index) {
                    subtree = subtree.child(
                        div()
                            .pl(px(30.))
                            .text_size(px(tree_style::FONT))
                            .text_color(theme.muted)
                            .child(note.clone()),
                    );
                }
                match self.db_tables.get(&(index, String::new())) {
                    Some(Some(tables)) => {
                        if tables.is_empty() {
                            subtree = subtree.child(
                                div()
                                    .pl(px(30.))
                                    .text_size(px(tree_style::FONT))
                                    .text_color(theme.muted)
                                    .child("没有表"),
                            );
                        }
                        subtree = subtree.child(self.grouped_objects(index, "", tables, cx));
                    }
                    _ => {
                        subtree = subtree.child(
                            div()
                                .pl(px(30.))
                                .text_size(px(tree_style::FONT))
                                .text_color(theme.muted)
                                .child("加载中…"),
                        )
                    }
                }
            }
            Some(Some(databases)) => {
                if let Some(note) = self.conn_db_note.get(&index) {
                    subtree = subtree.child(
                        div()
                            .pl(px(30.))
                            .text_size(px(tree_style::FONT))
                            .text_color(theme.muted)
                            .child(note.clone()),
                    );
                }
                for database in databases.iter().take(100) {
                    let expanded = self.expanded_dbs.contains(&(index, database.clone()));
                    let toggle_db = database.clone();
                    let fold_db = database.clone();
                    subtree = subtree.child(
                        div()
                            .id(SharedString::from(format!("db-{index}-{database}")))
                            .flex()
                            .items_center()
                            .gap(px(tree_style::GAP))
                            .h(px(tree_style::ROW))
                            .flex_shrink_0()
                            .pl(px(tree_style::INSET + tree_style::INDENT * 2.))
                            .pr_2()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.border.opacity(0.22)))
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                if event.click_count() == 2 {
                                    this.toggle_db(index, toggle_db.clone(), cx);
                                } else {
                                    this.open_object_preview(index, toggle_db.clone(), cx);
                                }
                            }))
                            .child(self.tree_fold_button(
                                SharedString::from(format!("fold-db-{index}-{database}")),
                                expanded,
                                move |this, cx| this.toggle_db(index, fold_db.clone(), cx),
                                cx,
                            ))
                            .child(icons::icon(
                                "superlink/database.svg",
                                tree_style::ICON,
                                gpui::Hsla::from(gpui::rgb(0x087ba3)),
                            ))
                            .child(
                                div()
                                    .text_size(px(tree_style::FONT))
                                    .text_color(theme.text)
                                    .child(SharedString::from(database.clone())),
                            ),
                    );
                    if !expanded {
                        continue;
                    }
                    match self.db_tables.get(&(index, database.clone())) {
                        Some(Some(tables)) => {
                            if tables.is_empty() {
                                subtree = subtree.child(
                                    div()
                                        .pl(px(44.))
                                        .text_size(px(tree_style::FONT))
                                        .text_color(theme.muted)
                                        .child("没有表"),
                                );
                            }
                            subtree =
                                subtree.child(self.grouped_objects(index, database, tables, cx));
                        }
                        _ => {
                            subtree = subtree.child(
                                div()
                                    .pl(px(44.))
                                    .text_size(px(tree_style::FONT))
                                    .text_color(theme.muted)
                                    .child("加载中…"),
                            )
                        }
                    }
                }
            }
        }
        subtree.into_any_element()
    }
    pub(super) fn sidebar_icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .w(px(26.0))
            .h(px(24.0))
            .flex_shrink_0()
            .rounded_md()
            .cursor_pointer()
            .bg(if active {
                gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.12)
            } else {
                theme.sidebar
            })
            .hover(move |style| style.bg(theme.border.opacity(0.22)))
            .on_click(cx.listener(move |this, _event, window, cx| handler(this, window, cx)))
            .child(icons::icon(
                icon,
                12.0,
                if active {
                    gpui::Hsla::from(gpui::rgb(0x17823b))
                } else {
                    theme.muted
                },
            ))
    }

    pub(super) fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            use gpui::Focusable as _;
            let handle = self.sql_search.read(cx).focus_handle(cx);
            window.focus(&handle);
        } else {
            self.sql_search
                .update(cx, |editor, cx| editor.set_text("", cx));
        }
        cx.notify();
    }

    pub(super) fn locate_connection(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let index = self.selected;
        self.expanded_conns.insert(index);
        if !self.conn_dbs.contains_key(&index) {
            self.fetch_databases(index, cx);
        }
        self.workspace = Workspace::Sql;
        self.sync_selection(cx);
        cx.notify();
    }

    pub(super) fn refresh_connections(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_profiles(None, cx);
        cx.notify();
    }

    pub(super) fn open_connection_menu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_groups(cx);
    }

    pub(super) fn filter_all(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind.clear();
        cx.notify();
    }

    pub(super) fn filter_tables(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind = "table".into();
        cx.notify();
    }

    pub(super) fn filter_views(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind = "view".into();
        cx.notify();
    }

    pub(super) fn filter_functions(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind = "function".into();
        cx.notify();
    }

    pub(super) fn saved_queries_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.park_workbench(cx) {
            return;
        }
        self.workspace = Workspace::Sql;
        self.query_page
            .update(cx, |page, cx| page.open_saved(window, cx));
        cx.notify();
    }

    pub(super) fn saved_open_indicator(&self, cx: &Context<Self>) -> bool {
        self.query_page.read(cx).saved_open()
    }
}
