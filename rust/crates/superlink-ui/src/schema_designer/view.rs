use super::*;
impl SchemaDesigner {
    pub(super) fn button(
        &self,
        id: &'static str,
        label: &'static str,
        enabled: bool,
        handler: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .flex_shrink_0()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .text_color(if enabled { theme.text } else { theme.muted })
            .bg(if enabled && matches!(id, "schema-save" | "schema-apply") {
                gpui::Hsla::from(gpui::rgb(0x17823b))
            } else {
                theme.surface
            })
            .when(
                enabled && matches!(id, "schema-save" | "schema-apply"),
                |s| s.text_color(gpui::white()),
            )
            .cursor_pointer()
            .whitespace_nowrap()
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                if enabled {
                    handler(this, cx);
                }
            }))
            .into_any_element()
    }
    pub(super) fn panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let editable = self.editable();
        let selected = !self.selected.is_empty();
        let mut panel = div()
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, _| {
                if event.pressed_button.is_none() {
                    this.dragging = None;
                }
            }))
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .bg(theme.surface)
            .text_size(px(12.))
            .text_color(theme.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .px_3()
                    .h(px(32.))
                    .flex_shrink_0()
                    .gap_2()
                    .child(format!(
                        "结构设计器 · {}{}",
                        self.table,
                        if self.dirty { " *" } else { "" }
                    ))
                    .child(div().text_color(theme.muted).child(format!(
                        "{} · {} 个字段",
                        self.database,
                        self.rows.len()
                    ))),
            )
            .child(
                div()
                    .id("schema-toolbar-scroll")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .flex_shrink_0()
                    .overflow_x_scroll()
                    .child(self.button(
                        "schema-save",
                        "保存",
                        editable,
                        |this, cx| this.prepare(cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-refresh",
                        "刷新",
                        !self.busy && !self.stale,
                        |this, cx| this.reload(cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-table-comment",
                        "表备注",
                        editable,
                        |this, cx| {
                            this.comment_open = !this.comment_open;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.button(
                        "schema-add",
                        "＋ 添加字段",
                        editable,
                        |this, cx| this.add_field(false, cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-insert",
                        "在选中字段后添加",
                        editable && selected,
                        |this, cx| this.add_field(true, cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-copy",
                        "复制字段",
                        editable && selected,
                        |this, cx| this.copy_fields(cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-paste",
                        "粘贴字段",
                        editable,
                        |this, cx| this.paste_fields(cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-copy-table",
                        "复制到另一张表",
                        editable && selected,
                        |this, cx| {
                            this.copy_open = !this.copy_open;
                            this.plan = None;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.button(
                        "schema-delete",
                        "删除字段",
                        editable && selected,
                        |this, cx| this.delete_fields(cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-up",
                        "↑",
                        editable && selected,
                        |this, cx| this.move_fields(false, cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-down",
                        "↓",
                        editable && selected,
                        |this, cx| this.move_fields(true, cx),
                        cx,
                    )),
            );
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .h(px(32.))
                .px_3()
                .flex_shrink_0()
                .children(crate::workbench::TableSection::ALL.map(|section| {
                    div()
                        .id(SharedString::from(format!("designer-section-{section:?}")))
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .text_color(if section == crate::workbench::TableSection::Columns {
                            gpui::Hsla::from(gpui::rgb(0x17823b))
                        } else {
                            theme.muted
                        })
                        .child(if section == crate::workbench::TableSection::Columns {
                            "字段"
                        } else {
                            section.label()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.busy {
                                cx.emit(DesignEvent::SelectSection(section));
                            }
                        }))
                })),
        );
        if self.dirty {
            panel = panel.child(
                div()
                    .flex()
                    .px_3()
                    .py_1()
                    .flex_shrink_0()
                    .child(self.button(
                        "schema-discard",
                        "放弃草稿",
                        !self.busy,
                        |this, cx| this.discard(cx),
                        cx,
                    )),
            );
        }
        if self.comment_open {
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(34.))
                    .flex_shrink_0()
                    .gap_2()
                    .px_3()
                    .child("表备注")
                    .child(div().flex_1().child(self.table_comment.clone())),
            );
        }
        if self.copy_open {
            panel = panel.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .flex_shrink_0()
                    .child(self.button(
                        "schema-copy-existing",
                        if self.copy_new {
                            "○ 已有表"
                        } else {
                            "● 已有表"
                        },
                        editable,
                        |this, cx| {
                            this.copy_new = false;
                            this.plan = None;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.button(
                        "schema-copy-new",
                        if self.copy_new {
                            "● 新建表"
                        } else {
                            "○ 新建表"
                        },
                        editable,
                        |this, cx| {
                            this.copy_new = true;
                            this.plan = None;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(
                        div()
                            .w(px(200.))
                            .h(px(30.))
                            .border_1()
                            .border_color(theme.border)
                            .px_2()
                            .child(self.target.clone()),
                    )
                    .child(self.button(
                        "schema-copy-preview",
                        "预览目标表变更",
                        editable && selected,
                        |this, cx| this.prepare_copy(cx),
                        cx,
                    ))
                    .child(div().text_color(theme.muted).child(if self.copy_new {
                        "将所选字段复制到新表"
                    } else {
                        "已有表只追加字段，不复制主键/自增约束"
                    })),
            );
        }
        if let Some(note) = &self.note {
            panel = panel.child(
                div()
                    .px_3()
                    .py_1()
                    .flex_shrink_0()
                    .text_color(theme.muted)
                    .child(note.clone()),
            );
        }
        if self.discard_confirm {
            panel = panel.child(
                div()
                    .px_3()
                    .flex()
                    .gap_2()
                    .flex_shrink_0()
                    .child(self.button(
                        "schema-discard-refresh",
                        "放弃草稿并刷新",
                        !self.busy,
                        |this, cx| this.reload(cx),
                        cx,
                    ))
                    .child(self.button(
                        "schema-keep-draft",
                        "保留草稿",
                        true,
                        |this, cx| {
                            this.discard_confirm = false;
                            cx.notify();
                        },
                        cx,
                    )),
            );
        }
        if self.busy {
            panel = panel.child(
                div()
                    .px_3()
                    .py_1()
                    .flex_shrink_0()
                    .text_color(theme.muted)
                    .child("正在处理结构…"),
            );
        }
        panel = panel.child(self.grid(cx));
        if let Some(plan) = &self.plan {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .h(px(200.))
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .child("SQL 预览")
                            .child(self.button(
                                "schema-apply",
                                "确认执行",
                                editable,
                                |this, cx| this.apply(cx),
                                cx,
                            ))
                            .child(self.button(
                                "schema-cancel-preview",
                                "取消",
                                !self.busy,
                                |this, cx| {
                                    this.plan = None;
                                    cx.notify();
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .id("schema-sql-preview")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .px_3()
                            .text_size(px(11.))
                            .child(plan.statements.join(";\n")),
                    ),
            );
        }
        panel.child(self.picker(cx)).into_any_element()
    }
}
