use super::*;
use superlink_drivers::TableEntry;

impl SuperLinkApp {
    pub(super) fn tree_fold_button(
        &self,
        id: SharedString,
        expanded: bool,
        toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .size(px(12.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(icons::icon(
                if expanded {
                    "shell/chevron-down.svg"
                } else {
                    "shell/chevron-right.svg"
                },
                12.,
                self.theme.muted,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                toggle(this, cx);
            }))
            .into_any_element()
    }

    pub(super) fn grouped_objects(
        &self,
        conn: usize,
        database: &str,
        tables: &[TableEntry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut groups: BTreeMap<String, Vec<&TableEntry>> = BTreeMap::new();
        let needle = self.sql_search.read(cx).text().to_lowercase();
        let connection_match = self.profiles.get(conn).is_some_and(|p| {
            p.name.to_lowercase().contains(&needle)
                || p.group.to_lowercase().contains(&needle)
                || p.sql_dialect().to_lowercase().contains(&needle)
        });
        for entry in tables.iter().filter(|entry| {
            self.matches_filter(entry)
                && (needle.is_empty()
                    || connection_match
                    || entry.name.to_lowercase().contains(&needle))
        }) {
            let label = match entry.kind.as_str() {
                "table" | "" => "表",
                "view" => "视图",
                "function" => "函数与存储过程",
                "sequence" => "序列",
                "trigger" => "触发器",
                "event" => "事件",
                _ => "其他对象",
            };
            groups.entry(label.into()).or_default().push(entry);
        }
        let db = database.to_owned();
        let mut tree = div().flex().flex_col().flex_shrink_0().child(
            div()
                .id(SharedString::from(format!("saved-in-{conn}-{database}")))
                .flex()
                .items_center()
                .h(px(tree_style::ROW))
                .flex_shrink_0()
                .gap(px(tree_style::GAP))
                .pl(px(tree_style::object_indent(database)
                    + 12.
                    + tree_style::GAP))
                .text_size(px(tree_style::FONT))
                .text_color(theme.muted)
                .cursor_pointer()
                .hover(move |s| s.bg(theme.border.opacity(0.22)))
                .child(icons::icon(
                    "superlink/folder-open.svg",
                    tree_style::ICON,
                    theme.muted,
                ))
                .child("已存查询")
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !this.park_workbench(cx) {
                        return;
                    }
                    if let Some(profile) = this.profiles.get(conn).cloned() {
                        this.query_page.update(cx, |page, cx| {
                            page.open_saved_for(profile, db.clone(), window, cx)
                        });
                    }
                })),
        );
        for (label, entries) in groups {
            let key = format!("{conn}/{database}/{label}");
            let collapsed = self.collapsed_object_groups.contains(&key);
            let fold_key = key.clone();
            let preview_db = database.to_owned();
            let preview_tables = label == "表";
            tree = tree.child(
                div()
                    .id(SharedString::from(format!("object-group-{key}")))
                    .flex()
                    .items_center()
                    .h(px(tree_style::ROW))
                    .flex_shrink_0()
                    .gap(px(tree_style::GAP))
                    .pl(px(tree_style::object_indent(database)))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(theme.muted)
                    .child(self.tree_fold_button(
                        SharedString::from(format!("fold-object-group-{key}")),
                        !collapsed,
                        move |this, cx| {
                            if !this.collapsed_object_groups.remove(&fold_key) {
                                this.collapsed_object_groups.insert(fold_key.clone());
                            }
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(icons::icon(
                        match label.as_str() {
                            "表" => "superlink/table.svg",
                            "视图" => "superlink/view.svg",
                            "函数与存储过程" => "superlink/function.svg",
                            _ => "superlink/folder.svg",
                        },
                        tree_style::ICON,
                        theme.muted,
                    ))
                    .child(label.clone())
                    .child(
                        div()
                            .px(px(4.))
                            .rounded_full()
                            .bg(theme.border.opacity(0.3))
                            .text_size(px(9.))
                            .child(entries.len().to_string()),
                    )
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        if event.click_count() != 2 {
                            if preview_tables {
                                this.open_object_preview(conn, preview_db.clone(), cx);
                            }
                            return;
                        }
                        if !this.collapsed_object_groups.remove(&key) {
                            this.collapsed_object_groups.insert(key.clone());
                        }
                        cx.notify();
                    })),
            );
            if !collapsed {
                for entry in entries.iter().take(200) {
                    tree = tree.child(self.table_block(conn, database.into(), entry, cx));
                }
                if entries.len() > 200 {
                    tree = tree.child(
                        div()
                            .pl(px(40.))
                            .text_size(px(11.))
                            .text_color(theme.muted)
                            .child("已显示前 200 个对象，请使用类型过滤缩小范围"),
                    );
                }
            }
        }
        tree.into_any_element()
    }
}
