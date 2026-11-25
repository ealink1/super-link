use super::*;

impl SuperLinkApp {
    /// 连接侧栏：标题与工具行、对象类型过滤行、分组树。
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut column = div()
            .flex()
            .flex_col()
            .w(px(self.sidebar_width))
            .flex_shrink_0()
            .min_h(px(0.))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .pl(px(6.))
                    .pr_2()
                    .child(self.sidebar_icon_button(
                        "conn-search",
                        "superlink/search.svg",
                        self.search_open,
                        cx,
                        Self::toggle_search,
                    ))
                    .child(self.sidebar_icon_button(
                        "conn-locate",
                        "superlink/locate.svg",
                        false,
                        cx,
                        Self::locate_connection,
                    ))
                    .child(self.sidebar_icon_button(
                        "refresh-connections",
                        "superlink/refresh.svg",
                        false,
                        cx,
                        Self::refresh_connections,
                    ))
                    .child(self.sidebar_icon_button(
                        "connection-menu",
                        "superlink/connection-menu.svg",
                        false,
                        cx,
                        Self::open_connection_menu,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(6.))
                    .h(px(28.))
                    .flex_shrink_0()
                    .child(self.sidebar_icon_button(
                        "filter-all",
                        "superlink/all-objects.svg",
                        self.filter_kind.is_empty(),
                        cx,
                        Self::filter_all,
                    ))
                    .child(self.sidebar_icon_button(
                        "filter-table",
                        "superlink/table.svg",
                        self.filter_kind == "table",
                        cx,
                        Self::filter_tables,
                    ))
                    .child(self.sidebar_icon_button(
                        "filter-view",
                        "superlink/view.svg",
                        self.filter_kind == "view",
                        cx,
                        Self::filter_views,
                    ))
                    .child(self.sidebar_icon_button(
                        "filter-function",
                        "superlink/function.svg",
                        self.filter_kind == "function",
                        cx,
                        Self::filter_functions,
                    ))
                    .child(self.sidebar_icon_button(
                        "saved-queries-icon",
                        "superlink/sql-doc.svg",
                        self.saved_open_indicator(cx),
                        cx,
                        Self::saved_queries_action,
                    ))
                    .child(self.sidebar_icon_button(
                        "execution-history",
                        "superlink/history.svg",
                        false,
                        cx,
                        Self::history_action,
                    )),
            );

        if self.search_open {
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .mx_2()
                    .mb_2()
                    .px_2()
                    .h(px(crate::theme::CONTROL_MIN))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                    .child(self.sql_search.clone()),
            );
        }

        if self.profiles.is_empty() {
            column = column.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("还没有连接。点击上方「新建连接」开始。"),
            );
        }

        let mut tree = div()
            .id("sql-object-tree")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll();
        let needle = self.sql_search.read(cx).text().to_lowercase();
        let mut groups: BTreeMap<String, Vec<(usize, &Profile)>> = BTreeMap::new();
        for (index, profile) in self.profiles.iter().enumerate() {
            if !needle.is_empty()
                && !profile.name.to_lowercase().contains(&needle)
                && !profile.sql_dialect().to_lowercase().contains(&needle)
                && !profile.group.to_lowercase().contains(&needle)
                && !self.db_tables.iter().any(|((conn, _), tables)| {
                    *conn == index
                        && tables.as_ref().is_some_and(|tables| {
                            tables
                                .iter()
                                .any(|t| t.name.to_lowercase().contains(&needle))
                        })
                })
            {
                continue;
            }
            let key = if profile.group.trim().is_empty() {
                "未分组".to_string()
            } else {
                profile.group.clone()
            };
            groups.entry(key).or_default().push((index, profile));
        }
        for name in &self.declared_groups {
            groups.entry(name.clone()).or_default();
        }
        if groups.is_empty() {
            groups.entry("未分组".to_string()).or_default();
        }

        for (group, entries) in groups {
            let collapsed = self.collapsed_groups.contains(&group);
            let group_key = group.clone();
            let fold_group = group.clone();
            tree = tree.child(
                div()
                    .id(SharedString::from(format!("group-{group}")))
                    .flex()
                    .items_center()
                    .gap(px(tree_style::GAP))
                    .mx(px(4.))
                    .h(px(tree_style::ROW))
                    .flex_shrink_0()
                    .pl(px(4.))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(gpui::transparent_black())
                    .hover(move |style| style.bg(theme.border.opacity(0.22)))
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, _window, cx| {
                            if event.click_count() != 2 {
                                return;
                            }
                            if !this.collapsed_groups.remove(&group_key) {
                                this.collapsed_groups.insert(group_key.clone());
                            }
                            cx.notify();
                        }),
                    )
                    .child(self.tree_fold_button(
                        SharedString::from(format!("fold-group-{group}")),
                        !collapsed,
                        move |this, cx| {
                            if !this.collapsed_groups.remove(&fold_group) {
                                this.collapsed_groups.insert(fold_group.clone());
                            }
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(icons::icon(
                        "superlink/folder.svg",
                        tree_style::ICON,
                        gpui::Hsla::from(gpui::rgb(0xc88a24)),
                    ))
                    .child(
                        div()
                            .text_size(px(tree_style::FONT))
                            .text_color(theme.text)
                            .child(SharedString::from(group.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .px(px(4.))
                            .rounded_full()
                            .bg(theme.border.opacity(0.3))
                            .text_color(theme.muted)
                            .child(format!("{}", entries.len())),
                    ),
            );
            if collapsed {
                continue;
            }
            for (index, profile) in entries {
                let active = index == self.selected;
                // 连接名没有长度上限，超长时截断，避免撑出侧栏。
                let name = SharedString::from(crate::text::shorten(&profile.name, 16));
                let expanded = self.expanded_conns.contains(&index);
                tree = tree.child(
                    div()
                        .id(("profile", index))
                        .flex()
                        .items_center()
                        .gap(px(tree_style::GAP))
                        .mx(px(4.))
                        .h(px(tree_style::ROW))
                        .flex_shrink_0()
                        .pl(px(tree_style::INSET + tree_style::INDENT - 4.))
                        .pr_2()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if active {
                            theme.border.opacity(0.22)
                        } else {
                            theme.sidebar
                        })
                        .hover(move |style| style.bg(theme.border.opacity(0.22)))
                        .on_click(cx.listener(
                            move |this, event: &gpui::ClickEvent, _window, cx| {
                                this.connection_actions = None;
                                this.selected = index;
                                this.workspace = Workspace::Sql;
                                this.sync_selection(cx);
                                if event.click_count() == 2 {
                                    this.toggle_conn(index, cx);
                                }
                                cx.notify();
                            },
                        ))
                        .child(
                            div()
                                .id(SharedString::from(format!("fold-conn-{index}")))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(12.0))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    cx.stop_propagation();
                                    this.toggle_conn(index, cx);
                                }))
                                .child(icons::icon(
                                    if expanded {
                                        "shell/chevron-down.svg"
                                    } else {
                                        "shell/chevron-right.svg"
                                    },
                                    12.0,
                                    theme.muted,
                                )),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                this.connection_actions = if this.connection_actions == Some(index)
                                {
                                    None
                                } else {
                                    Some(index)
                                };
                                this.connection_delete_confirm = false;
                                cx.notify();
                            }),
                        )
                        .child(icons::icon(
                            icons::connection_icon(profile),
                            tree_style::ICON,
                            theme.muted,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(px(tree_style::FONT))
                                .text_color(theme.text)
                                .child(name),
                        )
                        .children(
                            self.conn_dbs
                                .get(&index)
                                .is_some_and(|dbs| dbs.is_some())
                                .then(|| {
                                    div()
                                        .size(px(6.0))
                                        .rounded_full()
                                        .bg(gpui::rgb(0x188038))
                                        .into_any_element()
                                }),
                        ),
                );
                if self.connection_actions == Some(index) {
                    tree = tree.child(self.connection_actions_panel(index, cx));
                }
                if expanded {
                    tree = tree.child(self.conn_subtree(index, cx));
                }
            }
        }

        column = column.child(tree);

        column = column.child(
            div()
                .id("saved-queries")
                .flex()
                .items_center()
                .gap_2()
                .mx_2()
                .my_1()
                .h(px(28.0))
                .flex_shrink_0()
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .hover(move |style| style.bg(theme.border.opacity(0.22)))
                .on_click(cx.listener(|this, _event, window, cx| {
                    if !this.park_workbench(cx) {
                        return;
                    }
                    this.workspace = Workspace::Sql;
                    this.query_page
                        .update(cx, |page, cx| page.open_saved(window, cx));
                    cx.notify();
                }))
                .child(icons::icon("superlink/folder-open.svg", 14.0, theme.muted))
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .text_size(px(tree_style::FONT))
                        .child("全部已存查询"),
                ),
        );

        column
    }
    pub(super) fn global_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        // 顶部工具栏：Fyne header 高 56，图标在上、标题 12 号
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .h(px(HEADER_HEIGHT))
            .w_full()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .child(self.header_action(
                "global-new-query",
                "superlink/new-query.svg",
                "新建查询",
                cx,
                Self::new_query_action,
            ))
            .child(self.header_action(
                "global-new-connection",
                "superlink/link.svg",
                "新建连接",
                cx,
                Self::new_connection_action,
            ))
            .child(self.header_action(
                "global-groups",
                "superlink/all-objects.svg",
                "管理连接分组",
                cx,
                Self::open_groups_action,
            ))
            .child(div().w(px(1.0)).h(px(24.0)).mx(px(6.0)).bg(theme.border))
            .child(self.header_action(
                "global-history",
                "superlink/history.svg",
                "执行历史",
                cx,
                Self::history_action,
            ))
            .into_any_element()
    }

    pub(super) fn workspace_tab(
        &self,
        workspace: Workspace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.workspace == workspace;
        let theme = self.theme;
        let mut line = div()
            .pb(px(3.0))
            .border_b_2()
            .border_color(gpui::transparent_black())
            .text_size(px(14.0))
            .text_color(if active { theme.text } else { theme.muted })
            .child(workspace.label());
        if active {
            line = line.border_color(theme.accent);
        }
        div()
            .id(("workspace", workspace as usize))
            .flex()
            .items_center()
            .justify_center()
            .w(px(MODE_TAB_WIDTH))
            .h_full()
            .cursor_pointer()
            .child(line)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.notice = None;
                this.workspace = workspace;
                cx.notify();
            }))
    }

    /// 工具栏按钮：图标在上、12 号标题在下，最小宽度 64（对齐 Fyne headerAction）。
    pub(super) fn header_action(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .min_w(px(64.0))
            .h_full()
            .px(px(8.0))
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.border.opacity(0.22)))
            .on_click(cx.listener(move |this, _event, window, cx| handler(this, window, cx)))
            .child(icons::icon(icon, HEADER_ICON, theme.text))
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(HEADER_CAPTION))
                    .text_color(theme.text)
                    .child(label),
            )
    }

    /// 标题栏右侧的系统图标按钮：32×30，无边框，悬停给一层浅底。
    pub(super) fn utility_button(
        &self,
        id: &'static str,
        icon: &'static str,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .w(px(32.0))
            .h(px(30.0))
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.border.opacity(0.22)))
            .on_click(cx.listener(move |this, _event, window, cx| handler(this, window, cx)))
            .child(icons::icon(icon, 16.0, theme.muted))
    }

    pub(super) fn new_query_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        if !self.park_workbench(cx) {
            return;
        }

        if self
            .structure_edit
            .as_ref()
            .is_some_and(|e| !e.pending.is_empty())
        {
            self.error = Some("请先完成结构变更".into());
            cx.notify();
            return;
        }
        self.structure_edit = None;
        self.workspace = Workspace::Sql;
        self.query_page.update(cx, |page, cx| page.add_tab(cx));
        cx.notify();
    }

    pub(super) fn new_connection_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.workspace = Workspace::Sql;
        self.query_page
            .update(cx, |page, cx| page.open_connection_form(window, cx));
        cx.notify();
    }

    pub(super) fn open_groups_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_groups(cx);
    }

    pub(super) fn history_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.workspace = Workspace::Sql;
        self.query_page
            .update(cx, |page, cx| page.toggle_history(cx));
        cx.notify();
    }

    pub(super) fn open_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.settings_open = true;
        let task = cx
            .background_executor()
            .spawn(async { superlink_app::transfer::diagnostics() });
        cx.spawn(async move |this, cx| {
            let rows = task.await;
            let _ = this.update(cx, |this, cx| {
                this.diagnostics = rows;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn open_assistant(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.chat_open = !self.chat_open;
        cx.notify();
    }
}
