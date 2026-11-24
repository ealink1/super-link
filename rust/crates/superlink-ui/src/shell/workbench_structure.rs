use super::*;
use crate::workbench::TableSection;

impl SuperLinkApp {
    pub(super) fn workbench_structure(&self, cx: &mut Context<Self>) -> AnyElement {
        let b = self.workbench.as_ref().unwrap();
        if b.section == TableSection::Columns
            && let Some(editor) = &b.designer
        {
            return editor.clone().into_any_element();
        }
        let theme = self.theme;
        let section = b.section;
        let mut panel =
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.))
                .min_w(px(0.))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .h(px(34.))
                        .items_center()
                        .px_2()
                        .flex_shrink_0()
                        .children(TableSection::ALL.map(|tab| {
                            div()
                                .id(SharedString::from(format!("structure-section-{tab:?}")))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .text_color(if tab == section {
                                    gpui::Hsla::from(gpui::rgb(0x17823b))
                                } else {
                                    theme.muted
                                })
                                .bg(if tab == section {
                                    gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.12)
                                } else {
                                    theme.surface
                                })
                                .child(tab.label())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.select_table_section(tab, cx)
                                }))
                        })),
                );
        let key = (b.conn, b.database.clone(), b.table.clone());
        if let Some(error) = self.column_errors.get(&key) {
            return panel
                .child(div().p_3().text_color(theme.danger).child(error.clone()))
                .into_any_element();
        }
        let Some(structure) = self.table_structures.get(&key) else {
            return panel
                .child(div().p_3().text_color(theme.muted).child("正在读取表结构…"))
                .into_any_element();
        };
        let (headers, rows): (Vec<String>, Vec<Vec<String>>) = match section {
            TableSection::Columns => (
                vec!["字段", "类型", "可为空", "键", "默认值"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                structure
                    .columns
                    .iter()
                    .map(|c| {
                        vec![
                            c.name.clone(),
                            c.data_type.clone(),
                            if c.nullable { "是" } else { "否" }.into(),
                            if c.primary_key { "主键" } else { "" }.into(),
                            c.default_value.clone().unwrap_or_default(),
                        ]
                    })
                    .collect(),
            ),
            TableSection::Indexes => (
                vec!["索引名".into(), "字段".into(), "唯一性".into()],
                structure
                    .indexes
                    .iter()
                    .map(|i| {
                        vec![
                            i.name.clone(),
                            i.columns.join(", "),
                            if i.unique { "唯一" } else { "非唯一" }.into(),
                        ]
                    })
                    .collect(),
            ),
            TableSection::ForeignKeys => (
                vec![
                    "外键名".into(),
                    "字段".into(),
                    "引用表".into(),
                    "引用字段".into(),
                ],
                structure
                    .foreign_keys
                    .iter()
                    .map(|k| {
                        vec![
                            k.name.clone(),
                            k.columns.join(", "),
                            k.referenced_table.clone(),
                            k.referenced_columns.join(", "),
                        ]
                    })
                    .collect(),
            ),
            TableSection::Triggers => {
                if let Some(error) = &b.trigger_error {
                    return panel
                        .child(div().p_3().text_color(theme.danger).child(error.clone()))
                        .into_any_element();
                }
                let Some(result) = b.triggers.as_ref() else {
                    return panel
                        .child(div().p_3().text_color(theme.muted).child("正在读取触发器…"))
                        .into_any_element();
                };
                (
                    result.columns.iter().map(|c| c.name.clone()).collect(),
                    result
                        .rows
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|v| crate::query_page::cell_text(v).to_string())
                                .collect()
                        })
                        .collect(),
                )
            }
        };
        panel = panel.child(div().px_3().py_2().text_color(theme.muted).child(format!(
            "{} · {} 项",
            section.label(),
            rows.len()
        )));
        let width = if section == TableSection::Triggers {
            320.
        } else {
            200.
        };
        let mut grid = div()
            .flex()
            .flex_col()
            .w(px(width * headers.len() as f32))
            .h_full()
            .child(
                div()
                    .flex()
                    .h(px(30.))
                    .flex_shrink_0()
                    .bg(theme.canvas)
                    .children(
                        headers
                            .into_iter()
                            .map(|name| div().w(px(width)).flex_shrink_0().px_2().child(name)),
                    ),
            );
        if rows.is_empty() {
            grid = grid.child(
                div()
                    .p_3()
                    .text_color(theme.muted)
                    .child(format!("没有{}", section.label())),
            );
        }
        grid = grid.child(
            div()
                .id("structure-rows")
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .children(rows.into_iter().enumerate().map(|(i, row)| {
                    div()
                        .flex()
                        .h(px(30.))
                        .flex_shrink_0()
                        .bg(if i % 2 == 0 {
                            theme.surface
                        } else {
                            theme.canvas
                        })
                        .children(row.into_iter().enumerate().map(|(column, text)| {
                            let preview = crate::text::shorten(&text, 200);
                            div()
                                .id(("structure-cell", i * 10000 + column))
                                .cursor_pointer()
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        text.clone(),
                                    ))
                                }))
                                .w(px(width))
                                .flex_shrink_0()
                                .px_2()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(preview)
                        }))
                })),
        );
        panel
            .child(
                div()
                    .id("structure-horizontal")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_x_scroll()
                    .child(grid),
            )
            .into_any_element()
    }
}
