use super::*;

impl SuperLinkApp {
    pub(super) fn workbench_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(bench) = self.workbench.as_ref() else {
            return div().into_any_element();
        };
        let tab = bench.tab;
        let operator = crate::workbench::OPERATORS[bench.draft.operator];
        let join_and = bench.draft.join_and;

        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .bg(theme.surface)
            .text_size(px(12.))
            .text_color(theme.text)
            .children(
                (bench.tab != crate::workbench::WorkbenchTab::Structure
                    || bench.section != crate::workbench::TableSection::Columns)
                    .then(|| self.workbench_toolbar(cx)),
            );

        if let Some(error) = &bench.error {
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

        match tab {
            crate::workbench::WorkbenchTab::Data => {
                if bench.tools_open {
                    panel = panel.child(
                        div().flex().gap_2().px_3().children(
                            [
                                ("clear", "清空表 SQL"),
                                ("truncate", "截断表 SQL"),
                                ("drop", "删除表 SQL"),
                            ]
                            .into_iter()
                            .map(|(action, label)| {
                                div()
                                    .id(SharedString::from(format!("wb-operation-{action}")))
                                    .cursor_pointer()
                                    .text_size(px(11.))
                                    .text_color(theme.muted)
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.preview_table_operation(action, cx)
                                    }))
                            }),
                        ),
                    );
                    // 条件构建行
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
                                    .child(self.wb_column.clone()),
                            )
                            .child(
                                div()
                                    .id("wb-operator")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.cycle_workbench_operator(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.accent)
                                            .child(operator),
                                    ),
                            )
                            .child(
                                div()
                                    .id("wb-join")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.toggle_workbench_join(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.muted)
                                            .child(if join_and { "且" } else { "或" }),
                                    ),
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
                                    .child(self.wb_value.clone()),
                            )
                            .child(
                                div()
                                    .id("wb-apply-filter")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.apply_workbench_filter(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.surface)
                                            .child("添加条件"),
                                    ),
                            ),
                    );

                    if !bench.filters.is_empty() {
                        let mut chips = div().flex().flex_wrap().items_center().gap_1().px_3();
                        for (position, filter) in bench.filters.iter().enumerate() {
                            let position_copy = position;
                            chips = chips.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme.canvas)
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child(SharedString::from(format!(
                                                "{} {} {}",
                                                filter.column, filter.operator, filter.value
                                            ))),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("filter-x-{position}")))
                                            .cursor_pointer()
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.remove_workbench_filter(position_copy, cx)
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(crate::theme::TEXT_SIZE))
                                                    .text_color(theme.muted)
                                                    .child("×"),
                                            ),
                                    ),
                            );
                        }
                        panel = panel.child(chips);
                    }

                    panel = panel.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("文件导入"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w(px(320.0))
                                    .h(px(crate::theme::CONTROL_MIN))
                                    .px_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.surface)
                                    .child(self.wb_import.clone()),
                            )
                            .child(
                                div()
                                    .id("wb-export-file")
                                    .cursor_pointer()
                                    .px_2()
                                    .child("导出当前页")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.export_table_file(false, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("wb-import")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.import_workbench_csv(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child("导入"),
                                    ),
                            ),
                    );

                    panel = panel.child(
                        div()
                            .px_3()
                            .id("wb-export-all")
                            .cursor_pointer()
                            .text_color(theme.accent)
                            .child("导出全部筛选结果（可选择 xlsx / csv / json / md / html / sql）")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.export_table_file(true, cx)),
                            ),
                    );
                    panel = panel.child(self.import_preview(cx));
                }
                if bench.loading {
                    panel = panel.child(
                        div()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("加载中…"),
                    );
                    return panel.child(self.workbench_footer(cx)).into_any_element();
                }

                let Some(result) = bench.result.as_ref() else {
                    panel = panel.child(
                        div()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("选择条件后加载数据"),
                    );
                    return panel.child(self.workbench_footer(cx)).into_any_element();
                };

                if bench.tools_open || !bench.edits.is_empty() || bench.cell.is_some() {
                    panel = panel.child(self.data_edit_toolbar(cx));
                }
                panel = panel.child(self.workbench_grid(result, cx));
            }
            crate::workbench::WorkbenchTab::Structure => {
                panel = panel.child(self.workbench_structure(cx));
            }
            crate::workbench::WorkbenchTab::Ddl => {
                let text: SharedString = bench
                    .ddl_source
                    .clone()
                    .unwrap_or_else(|| {
                        if bench.ddl_loading {
                            "正在读取数据库 DDL…".into()
                        } else {
                            "未能读取 DDL，请重新打开此页".into()
                        }
                    })
                    .into();
                panel = panel.child(
                    div()
                        .id("wb-ddl-scroll")
                        .flex_1()
                        .min_h(px(0.))
                        .overflow_y_scroll()
                        .p_3()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child(text),
                );
            }
        }
        panel = panel.child(self.workbench_footer(cx));
        panel.into_any_element()
    }
}
