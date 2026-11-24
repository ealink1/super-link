use super::*;
const COLUMNS: usize = 40;
impl QueryPage {
    fn column_width(&self, index: usize) -> f32 {
        self.column_widths
            .get(&index)
            .copied()
            .unwrap_or(COLUMN_WIDTH)
    }
    pub(super) fn result_grid(&self, result: &ResultSet, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let start = self.column_page * COLUMNS;
        let end = (start + COLUMNS).min(result.columns.len());
        let width = ROW_NUMBER_WIDTH + (start..end).map(|i| self.column_width(i)).sum::<f32>();
        let header = div()
            .flex()
            .h(px(crate::theme::CONTROL_MIN))
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(px(ROW_NUMBER_WIDTH))
                    .flex_shrink_0()
                    .px_2()
                    .child("#"),
            )
            .children((start..end).map(|index| {
                let width = self.column_width(index);
                div()
                    .flex()
                    .flex_shrink_0()
                    .w(px(width))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .px_2()
                            .font_weight(FontWeight::BOLD)
                            .child(result.columns[index].name.clone()),
                    )
                    .child(
                        div()
                            .id(("column-divider", index))
                            .w(px(5.))
                            .h_full()
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.column_drag = Some((index, event.position.x, width));
                                    cx.notify();
                                }),
                            ),
                    )
            }));
        let rows = result
            .rows
            .iter()
            .enumerate()
            .skip(self.display_page * DISPLAY_ROWS)
            .take(DISPLAY_ROWS)
            .map(|(row, values)| {
                div()
                    .flex()
                    .items_center()
                    .h(px(26.))
                    .flex_shrink_0()
                    .bg(if row % 2 == 0 {
                        theme.surface
                    } else {
                        theme.canvas
                    })
                    .child(
                        div()
                            .w(px(ROW_NUMBER_WIDTH))
                            .flex_shrink_0()
                            .px_2()
                            .text_color(theme.muted)
                            .child((row + 1).to_string()),
                    )
                    .children((start..end).map(|column| {
                        let value = values.get(column).unwrap_or(&Value::Null);
                        div()
                            .id(SharedString::from(format!("result-cell-{row}-{column}")))
                            .flex_shrink_0()
                            .w(px(self.column_width(column)))
                            .h_full()
                            .px_2()
                            .overflow_hidden()
                            .cursor_pointer()
                            .text_color(if value.is_null() {
                                theme.muted
                            } else {
                                theme.text
                            })
                            .bg(if self.inspected == Some((row, column)) {
                                theme.accent_soft
                            } else {
                                gpui::transparent_black()
                            })
                            .child(crate::text::shorten(&cell_text(value), 100))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.inspected = Some((row, column));
                                cx.notify();
                            }))
                    }))
            })
            .collect::<Vec<_>>();
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .text_size(px(12.))
            .text_color(theme.text)
            .child(self.result_pager(cx));
        if result.columns.len() > COLUMNS {
            let pages = result.columns.len().div_ceil(COLUMNS);
            panel = panel.child(
                div()
                    .flex()
                    .gap_3()
                    .px_3()
                    .h(px(24.))
                    .child(
                        div()
                            .id("prev-columns")
                            .cursor_pointer()
                            .child("上一组列")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.column_page = this.column_page.saturating_sub(1);
                                cx.notify();
                            })),
                    )
                    .child(format!(
                        "列 {}–{} / {}",
                        start + 1,
                        end,
                        result.columns.len()
                    ))
                    .child(
                        div()
                            .id("next-columns")
                            .cursor_pointer()
                            .child("下一组列")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.column_page = (this.column_page + 1).min(pages - 1);
                                cx.notify();
                            })),
                    ),
            );
        }
        panel = panel.child(
            div()
                .id("result-horizontal")
                .flex_1()
                .min_h(px(0.))
                .overflow_x_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w(px(width))
                        .h_full()
                        .child(header)
                        .child(
                            div()
                                .id("query-result-scroll")
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .children(rows),
                        ),
                ),
        );
        if let Some((row, column)) = self.inspected
            && let Some(value) = result.rows.get(row).and_then(|r| r.get(column))
        {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .max_h(px(140.))
                    .p_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(format!("行 {} · {}", row + 1, result.columns[column].name))
                            .child(
                                div()
                                    .id("copy-inspected-cell")
                                    .cursor_pointer()
                                    .text_color(theme.accent)
                                    .child("复制完整值")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some((row, column)) = this.inspected
                                            && let Some(value) = this
                                                .result
                                                .as_ref()
                                                .and_then(|r| r.rows.get(row))
                                                .and_then(|r| r.get(column))
                                        {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                cell_text(value).to_string(),
                                            ));
                                        }
                                    })),
                            )
                            .child(
                                div()
                                    .id("close-inspected-cell")
                                    .cursor_pointer()
                                    .child("关闭")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.inspected = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("cell-inspector-scroll")
                            .overflow_y_scroll()
                            .child(crate::text::shorten(&cell_text(value), 16_384)),
                    ),
            );
        }
        panel.into_any_element()
    }
}
