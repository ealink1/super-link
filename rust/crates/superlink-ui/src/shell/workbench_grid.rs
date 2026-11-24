use super::*;
use superlink_domain::ResultSet;

const DEFAULT_COLUMN_WIDTH: f32 = 100.;

impl SuperLinkApp {
    pub(super) fn workbench_grid(&self, result: &ResultSet, cx: &mut Context<Self>) -> AnyElement {
        let b = self.workbench.as_ref().unwrap();
        let theme = self.theme;
        if b.text_view {
            return div()
                .id("wb-json-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .p_3()
                .text_size(px(12.))
                .text_color(theme.text)
                .child(crate::workbench::json_preview(result))
                .into_any_element();
        }
        let width = |i: usize| {
            b.column_widths
                .get(&i)
                .copied()
                .unwrap_or(DEFAULT_COLUMN_WIDTH)
        };
        let start = (b.column_page * 40).min(result.columns.len().saturating_sub(1));
        let end = (start + 40).min(result.columns.len());
        let total_width = 70. + (start..end).map(width).sum::<f32>();
        let header = div()
            .flex()
            .h(px(42.))
            .flex_shrink_0()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("wb-select-all")
                    .w(px(70.))
                    .flex_shrink_0()
                    .px_2()
                    .cursor_pointer()
                    .child("□    #")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(b) = this.workbench.as_mut() {
                            let len = b.result.as_ref().map(|r| r.rows.len()).unwrap_or(0);
                            if b.selected.len() == len {
                                b.selected.clear();
                            } else {
                                b.selected = (0..len).collect();
                            }
                        }
                        cx.notify();
                    })),
            )
            .children(
                result
                    .columns
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(end - start)
                    .map(|(i, column)| {
                        let name = column.name.clone();
                        let tooltip_name = name.clone();
                        let tooltip_column = b.columns.iter().find(|c| c.name == name).cloned();
                        let data_type = b
                            .columns
                            .iter()
                            .find(|c| c.name == name)
                            .map(|c| {
                                format!(
                                    "{}{}",
                                    c.data_type,
                                    if c.primary_key { " · PK" } else { "" }
                                )
                            })
                            .unwrap_or_default();
                        div()
                            .flex()
                            .w(px(width(i)))
                            .flex_shrink_0()
                            .h_full()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .id(("wb-column", i))
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .px_2()
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| {
                                            workbench_tooltip::ColumnTooltip::new(
                                                &tooltip_name,
                                                tooltip_column.as_ref(),
                                                theme,
                                            )
                                        })
                                        .into()
                                    })
                                    .child(column.name.clone())
                                    .child(
                                        div()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_size(px(10.))
                                            .text_color(theme.muted)
                                            .child(data_type),
                                    ),
                            )
                            .child(self.workbench_header_actions(i, &column.name, cx))
                            .child(
                                div()
                                    .id(("wb-column-resize", i))
                                    .w(px(5.))
                                    .h_full()
                                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, event: &gpui::MouseDownEvent, _, cx| {
                                                let w = this
                                                    .workbench
                                                    .as_ref()
                                                    .and_then(|b| b.column_widths.get(&i))
                                                    .copied()
                                                    .unwrap_or(DEFAULT_COLUMN_WIDTH);
                                                this.wb_column_drag =
                                                    Some((i, event.position.x, w));
                                                cx.notify();
                                            },
                                        ),
                                    ),
                            )
                    }),
            );
        let rows = result.rows.iter().enumerate().map(|(row, values)| {
            div()
                .flex()
                .h(px(27.))
                .flex_shrink_0()
                .bg(if row % 2 == 0 {
                    theme.surface
                } else {
                    theme.canvas
                })
                .border_b_1()
                .border_color(theme.border.opacity(0.5))
                .child(
                    div()
                        .id(("wb-select-row", row))
                        .w(px(70.))
                        .h_full()
                        .flex_shrink_0()
                        .px_2()
                        .cursor_pointer()
                        .text_color(theme.muted)
                        .child(format!(
                            "{}  {}",
                            if b.selected.contains(&row) {
                                "☑"
                            } else {
                                "□"
                            },
                            (b.page - 1) * b.page_size + row as u32 + 1
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(b) = this.workbench.as_mut()
                                && !b.selected.remove(&row)
                            {
                                b.selected.insert(row);
                            }
                            cx.notify();
                        })),
                )
                .children(
                    result
                        .columns
                        .iter()
                        .enumerate()
                        .skip(start)
                        .take(end - start)
                        .map(|(column, _)| {
                            let value = values.get(column).unwrap_or(&serde_json::Value::Null);
                            let selected = b.inspected == Some((row, column));
                            div()
                                .id(SharedString::from(format!("wb-cell-{row}-{column}")))
                                .w(px(width(column)))
                                .h_full()
                                .flex_shrink_0()
                                .px_2()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .cursor_pointer()
                                .border_1()
                                .border_color(if selected {
                                    gpui::Hsla::from(gpui::rgb(0x17823b))
                                } else {
                                    theme.border.opacity(0.3)
                                })
                                .bg(if selected {
                                    gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.08)
                                } else {
                                    gpui::transparent_black()
                                })
                                .text_color(if value.is_null() {
                                    theme.muted
                                } else {
                                    theme.text
                                })
                                .child(crate::text::shorten(
                                    &crate::query_page::cell_text(value),
                                    180,
                                ))
                                .on_click(cx.listener(
                                    move |this, event: &gpui::ClickEvent, _, cx| {
                                        if let Some(b) = this.workbench.as_mut() {
                                            b.inspected = Some((row, column));
                                        }
                                        if event.click_count() == 2 {
                                            this.open_data_cell(row, column, cx);
                                        }
                                        cx.notify();
                                    },
                                ))
                        }),
                )
        });
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .min_w(px(0.))
            .text_size(px(12.))
            .text_color(theme.text)
            .child(
                div()
                    .id("wb-horizontal")
                    .w_full()
                    .min_w(px(0.))
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_x_scroll()
                    .track_scroll(&b.horizontal)
                    .map(|mut element| {
                        element.style().restrict_scroll_to_axis = Some(true);
                        element
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(total_width))
                            .h_full()
                            .child(header)
                            .child(
                                div()
                                    .id("wb-vertical")
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_h(px(0.))
                                    .overflow_y_scroll()
                                    .track_scroll(&b.vertical)
                                    .map(|mut element| {
                                        element.style().restrict_scroll_to_axis = Some(true);
                                        element
                                    })
                                    .children(rows),
                            ),
                    ),
            );
        panel = panel.child(self.workbench_scrollbar(cx));
        if let Some((row, column)) = b.inspected
            && let Some(value) = result.rows.get(row).and_then(|r| r.get(column))
        {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .max_h(px(100.))
                    .border_t_1()
                    .border_color(theme.border)
                    .p_2()
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(format!("行 {} · {}", row + 1, result.columns[column].name))
                            .child(self.edit_button(
                                "wb-copy-cell",
                                "复制完整值",
                                |this, cx| {
                                    if let Some(b) = this.workbench.as_ref()
                                        && let Some((row, col)) = b.inspected
                                        && let Some(v) = b
                                            .result
                                            .as_ref()
                                            .and_then(|r| r.rows.get(row))
                                            .and_then(|r| r.get(col))
                                    {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            crate::query_page::cell_text(v).to_string(),
                                        ));
                                    }
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .id("wb-inspected-value")
                            .overflow_y_scroll()
                            .child(crate::query_page::cell_text(value)),
                    ),
            );
        }
        panel.into_any_element()
    }
}
