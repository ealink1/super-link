use super::*;
use gpui::{MouseButton, SharedString};
const WIDTHS: [f32; 10] = [34., 24., 170., 180., 66., 56., 56., 82., 160., 240.];
const TYPES: &[&str] = &[
    "tinyint",
    "tinyint(1)",
    "smallint",
    "mediumint",
    "int",
    "bigint",
    "float",
    "double",
    "decimal(10,2)",
    "char(1)",
    "varchar(255)",
    "text",
    "mediumtext",
    "longtext",
    "date",
    "time",
    "datetime",
    "timestamp",
    "json",
    "blob",
    "boolean",
];
impl SchemaDesigner {
    pub(super) fn grid(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let editable = self.editable();
        let mysql = superlink_sql::structure_dialect(self.profile.sql_dialect()) == "mysql";
        let headers = [
            "□",
            "",
            "名",
            "类型",
            "无符号",
            "主键",
            "自增",
            "不是 NULL",
            "默认",
            "注释",
        ];
        let mut panel = div().flex().flex_col().flex_1().min_h(px(0.)).min_w(px(0.));
        if self.rows.len() > 100 {
            let pages = self.rows.len().div_ceil(100);
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .px_3()
                    .flex_shrink_0()
                    .child(format!(
                        "字段 {}–{} / {}",
                        self.field_page * 100 + 1,
                        ((self.field_page + 1) * 100).min(self.rows.len()),
                        self.rows.len()
                    ))
                    .child(self.button(
                        "schema-prev-fields",
                        "上一组",
                        self.field_page > 0,
                        |this, cx| {
                            this.field_page = this.field_page.saturating_sub(1);
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(self.button(
                        "schema-next-fields",
                        "下一组",
                        self.field_page + 1 < pages,
                        move |this, cx| {
                            this.field_page = (this.field_page + 1).min(pages - 1);
                            cx.notify();
                        },
                        cx,
                    )),
            );
        }
        let header = div()
            .flex()
            .h(px(32.))
            .flex_shrink_0()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .children(headers.into_iter().enumerate().map(|(i, label)| {
                let mut cell = div()
                    .id(("schema-header", i))
                    .w(px(WIDTHS[i]))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_2()
                    .text_color(theme.muted)
                    .child(label);
                if i == 0 {
                    cell = cell
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.editable() {
                                return;
                            }
                            if this.selected.len() == this.rows.len() {
                                this.selected.clear();
                            } else {
                                this.selected = this.rows.iter().map(|r| r.id.clone()).collect();
                            }
                            this.plan = None;
                            cx.notify();
                        }));
                }
                cell
            }));
        let rows = self
            .rows
            .iter()
            .skip(self.field_page * 100)
            .take(100)
            .enumerate()
            .map(|(i, row)| {
                let select = row.id.clone();
                let drag = row.id.clone();
                let drop = row.id.clone();
                let picker = row.id.clone();
                div()
                    .id(SharedString::from(format!("schema-row-{}", row.id)))
                    .flex()
                    .h(px(36.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(if self.selected.contains(&row.id) {
                        gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.1)
                    } else if i % 2 == 0 {
                        theme.surface
                    } else {
                        theme.canvas
                    })
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| this.drop_field(&drop, cx)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("schema-select-{}", row.id)))
                            .w(px(WIDTHS[0]))
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(if self.selected.contains(&row.id) {
                                "☑"
                            } else {
                                "□"
                            })
                            .text_color(if self.selected.contains(&row.id) {
                                gpui::Hsla::from(gpui::rgb(0x17823b))
                            } else {
                                theme.muted
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.editable() {
                                    return;
                                }
                                if !this.selected.remove(&select) {
                                    this.selected.insert(select.clone());
                                }
                                this.plan = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("schema-drag-{}", row.id)))
                            .w(px(WIDTHS[1]))
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .text_color(theme.muted)
                            .cursor(gpui::CursorStyle::OpenHand)
                            .child("≡")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    if this.editable() {
                                        this.dragging = Some(drag.clone());
                                        cx.notify();
                                    }
                                }),
                            ),
                    )
                    .child(self.editor_cell(&row.name, WIDTHS[2]))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w(px(WIDTHS[3]))
                            .h_full()
                            .flex_shrink_0()
                            .px_2()
                            .gap_1()
                            .child(div().flex_1().min_w(px(0.)).child(row.data_type.clone()))
                            .child(
                                div()
                                    .id(SharedString::from(format!("schema-type-{}", row.id)))
                                    .w(px(18.))
                                    .cursor_pointer()
                                    .child("⌄")
                                    .on_click(cx.listener(
                                        move |this, event: &gpui::ClickEvent, _, cx| {
                                            if !this.editable() {
                                                return;
                                            }
                                            this.type_picker = Some(picker.clone());
                                            this.type_anchor = gpui::point(
                                                event.position().x - px(160.),
                                                event.position().y + px(12.),
                                            );
                                            cx.notify();
                                        },
                                    )),
                            ),
                    )
                    .children(
                        [row.unsigned, row.primary, row.auto, row.not_null]
                            .into_iter()
                            .enumerate()
                            .map(|(flag, value)| {
                                let id = row.id.clone();
                                let enabled = editable
                                    && row.base.writable()
                                    && (flag == 3 && !row.primary || mysql);
                                div()
                                    .id(SharedString::from(format!(
                                        "schema-flag-{flag}-{}",
                                        row.id
                                    )))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h_full()
                                    .w(px(WIDTHS[flag + 4]))
                                    .flex_shrink_0()
                                    .cursor_pointer()
                                    .text_color(if value {
                                        gpui::Hsla::from(gpui::rgb(0x17823b))
                                    } else {
                                        theme.muted
                                    })
                                    .child(if value { "☑" } else { "□" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if enabled {
                                            this.toggle_flag(&id, flag, cx);
                                        }
                                    }))
                            }),
                    )
                    .child(self.editor_cell(&row.default, WIDTHS[8]))
                    .child(self.editor_cell(&row.comment, WIDTHS[9]))
            });
        panel
            .child(
                div()
                    .id("schema-grid-horizontal")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_x_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(WIDTHS.iter().sum()))
                            .h_full()
                            .child(header)
                            .child(
                                div()
                                    .id("schema-grid-vertical")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h(px(0.))
                                    .overflow_y_scroll()
                                    .children(rows),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn editor_cell(&self, editor: &Entity<SqlEditor>, width: f32) -> AnyElement {
        div()
            .flex()
            .items_center()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .px_2()
            .child(div().flex_1().min_w(px(0.)).child(editor.clone()))
            .into_any_element()
    }
    pub(super) fn picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(id) = &self.type_picker else {
            return div().into_any_element();
        };
        let theme = self.theme;
        let choices: Vec<&str> =
            if superlink_sql::structure_dialect(self.profile.sql_dialect()) == "sqlite" {
                vec!["INTEGER", "TEXT", "REAL", "BLOB", "NUMERIC"]
            } else if superlink_sql::structure_dialect(self.profile.sql_dialect()) == "postgres" {
                vec![
                    "smallint",
                    "integer",
                    "bigint",
                    "numeric(10,2)",
                    "real",
                    "double precision",
                    "varchar(255)",
                    "text",
                    "date",
                    "timestamp",
                    "boolean",
                    "json",
                    "jsonb",
                    "uuid",
                    "bytea",
                ]
            } else {
                TYPES.to_vec()
            };
        gpui::deferred(
            gpui::anchored().position(self.type_anchor).child(
                div()
                    .id("schema-type-menu")
                    .flex()
                    .flex_col()
                    .w(px(180.))
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .bg(theme.surface)
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .shadow_md()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.type_picker = None;
                        cx.notify();
                    }))
                    .children(choices.into_iter().enumerate().map(|(i, value)| {
                        let id = id.clone();
                        div()
                            .id(("schema-type-option", i))
                            .h(px(28.))
                            .flex_shrink_0()
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .hover(move |s| s.bg(theme.accent_soft))
                            .child(value)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.choose_type(&id, value, cx)),
                            )
                    })),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }
}
