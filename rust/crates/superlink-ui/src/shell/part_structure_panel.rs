use super::*;

impl SuperLinkApp {
    pub(super) fn structure_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(edit) = self.structure_edit.as_ref() else {
            return div().into_any_element();
        };
        let table = edit.table.clone();
        let confirmed = edit.confirmed;
        let nullable = edit.nullable;

        // 顶部：标题 + 关闭
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(format!("结构编辑 · {table}")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("close-structure")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.close_structure_editor(cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("关闭"),
                            ),
                    ),
            );

        // 错误提示
        if let Some(error) = &edit.error {
            panel = panel.child(
                div()
                    .mx_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }

        // 添加列表单
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(150.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(150.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.data_type.clone()),
                )
                .child(
                    div()
                        .id("toggle-nullable")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(if edit.nullable {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .bg(if nullable {
                            theme.accent_soft
                        } else {
                            theme.surface
                        })
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            if let Some(edit) = this.structure_edit.as_mut() {
                                edit.nullable = !edit.nullable;
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if nullable { theme.accent } else { theme.muted })
                                .child(if nullable { "可空" } else { "非空" }),
                        ),
                )
                .child(
                    div()
                        .id("queue-add-column")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.queue_add_column(cx)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("加入变更集"),
                        ),
                ),
        );

        // 索引：现有索引（可删）+ 添加索引表单
        let mut index_section = div().flex().flex_col().gap_1().px_3();
        index_section = index_section.child(
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child("索引"),
        );
        for index in &edit.indexes {
            let name = index.name.clone();
            let deletable =
                !name.eq_ignore_ascii_case("PRIMARY") && !name.starts_with("sqlite_autoindex");
            let deleting = edit.pending_delete.as_deref() == Some(format!("index:{name}").as_str());
            let mut row = div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.canvas)
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child(SharedString::from(index.name.clone())),
                )
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .child(SharedString::from(index.columns.join(", "))),
                );
            if deletable {
                row = row.child(
                    div()
                        .id(SharedString::from(format!("index-drop-{name}")))
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(if deleting { theme.danger } else { theme.muted })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.queue_drop_index(name.clone(), cx)
                        }))
                        .child(if deleting { "确认删除" } else { "删除" }),
                );
            }
            index_section = index_section.child(row);
        }
        index_section = index_section.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(140.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.index_name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(180.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.index_columns.clone()),
                )
                .child(
                    div()
                        .id("toggle-index-unique")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(if edit.index_unique {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .bg(if edit.index_unique {
                            theme.accent_soft
                        } else {
                            theme.surface
                        })
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.toggle_index_unique(cx)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if edit.index_unique {
                                    theme.accent
                                } else {
                                    theme.muted
                                })
                                .child("唯一"),
                        ),
                )
                .child(
                    div()
                        .id("queue-add-index")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(cx.listener(|this, _event, _window, cx| this.queue_add_index(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("添加索引"),
                        ),
                ),
        );
        panel = panel.child(index_section);

        // 变更集
        let mut changes = div().flex().flex_col().gap_1().px_3();
        changes = changes.child(
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child(format!("变更集 ({})", edit.pending.len())),
        );
        for (position, change) in edit.pending.iter().enumerate() {
            let summary = match change.kind.as_str() {
                "addColumn" => format!("+ 添加列 {}", change.column.name),
                "dropColumn" => format!("- 删除列 {}", change.original_name),
                "alterColumn" => {
                    format!("~ 重命名 {} → {}", change.original_name, change.column.name)
                }
                other => format!("… {other}"),
            };
            changes = changes.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.canvas)
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(summary)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("pending-x-{position}")))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.remove_pending_change(position, cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("×"),
                            ),
                    ),
            );
        }
        panel = panel.child(changes);

        // 预览与受保护提交
        panel =
            panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .child(
                        div()
                            .id("build-preview")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .border_1()
                            .border_color(theme.border)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.build_structure_preview(cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child("生成 SQL 预览"),
                            ),
                    )
                    .child(
                        div()
                            .id("commit-structure")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if edit.preview.is_some() {
                                theme.accent
                            } else {
                                theme.border
                            })
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.commit_structure(cx)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if edit.preview.is_some() {
                                        theme.surface
                                    } else {
                                        theme.muted
                                    })
                                    .child(if confirmed { "确认提交" } else { "提交" }),
                            ),
                    ),
            );
        if let Some(preview) = &edit.preview {
            panel = panel.child(
                div()
                    .mx_3()
                    .p_2()
                    .rounded_md()
                    .bg(theme.canvas)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(SharedString::from(preview.clone())),
            );
        }

        // 已执行
        if !edit.statements.is_empty() {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mx_3()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("已执行"),
                    )
                    .children(edit.statements.iter().map(|statement| {
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(statement.clone()))
                    })),
            );
        }

        // 当前列 + 每列的删除/改名
        let mut columns = div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .flex_1()
            .min_h(px(0.0));
        columns = columns.child(
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child("当前列"),
        );
        for column in &edit.columns {
            let name = column.name.clone();
            let rename_name = name.clone();
            let drop_name = name.clone();
            let renaming = edit.rename_from.as_deref() == Some(name.as_str());
            let deleting = edit.pending_delete.as_deref() == Some(name.as_str());
            let meta = if column.primary_key {
                format!("{} · PK", column.data_type)
            } else if column.nullable {
                column.data_type.clone()
            } else {
                format!("{} · NOT NULL", column.data_type)
            };
            columns = columns.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.canvas)
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(column.name.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child(SharedString::from(meta)),
                    )
                    .children(column.comment.as_ref().map(|comment| {
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child(SharedString::from(comment.clone()))
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(SharedString::from(format!("rename-{}", name)))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.accent)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.queue_rename_column(rename_name.clone(), cx)
                            }))
                            .child("改名"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("drop-{}", name.clone())))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if deleting { theme.danger } else { theme.muted })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.queue_drop_column(drop_name.clone(), cx)
                            }))
                            .child(if deleting { "确认删除" } else { "删除" }),
                    ),
            );
            if renaming && let Some(editor) = &edit.rename_editor {
                columns = columns.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl(px(20.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .w(px(150.0))
                                .h(px(crate::theme::CONTROL_MIN))
                                .px_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(editor.clone()),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("rename-ok-{}", name.clone())))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(theme.accent)
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.queue_rename_column(name.clone(), cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.surface)
                                        .child("确认改名"),
                                ),
                        ),
                );
            }
        }

        panel.child(columns).into_any_element()
    }

    /// 当前连接的表结构；只有原生 SQLite 会填充，其余显示说明。
    /// 一张表的行 + 展开后的列/索引/外键。
    pub(super) fn matches_filter(&self, entry: &superlink_drivers::TableEntry) -> bool {
        self.filter_kind.is_empty() || entry.kind == self.filter_kind
    }
}
