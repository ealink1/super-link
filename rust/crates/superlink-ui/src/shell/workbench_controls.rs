use super::*;
use crate::workbench::WorkbenchTab;

impl SuperLinkApp {
    pub(super) fn can_reload_workbench(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(b) = self.workbench.as_mut() else {
            return false;
        };
        if b.stale || b.loading || !b.edits.is_empty() {
            b.error = Some("请先完成当前操作或撤销变更，再调整筛选、排序和分页".into());
            cx.notify();
            return false;
        }
        true
    }
    pub(super) fn workbench_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let bench = self.workbench.as_ref().unwrap();
        let theme = self.theme;
        let mut bar = div()
            .flex()
            .items_center()
            .gap_1()
            .h(px(38.))
            .flex_shrink_0()
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            .text_size(px(12.))
            .text_color(theme.text)
            .child(icons::icon("superlink/table.svg", 14., theme.muted))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(format!("{} · {}", bench.table, bench.database)),
            )
            .child(self.edit_button(
                "wb-refresh",
                "刷新",
                |this, cx| this.refresh_workbench(cx),
                cx,
            ));
        if bench.tab == WorkbenchTab::Data {
            bar = bar
                .child(self.edit_button(
                    "wb-tools",
                    "筛选 / 导入",
                    |this, cx| {
                        if let Some(b) = this.workbench.as_mut() {
                            b.tools_open = !b.tools_open;
                        }
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.edit_button("wb-new-row", "+ 行", |this, cx| this.add_data_row(cx), cx))
                .child(self.edit_button(
                    "wb-remove-row",
                    "删行",
                    |this, cx| this.delete_data_rows(cx),
                    cx,
                ))
                .child(self.edit_button(
                    "wb-export",
                    "导出",
                    |this, cx| this.export_table_file(true, cx),
                    cx,
                ));
        }
        bar.child(self.edit_button(
            "wb-edit-structure",
            "设计",
            |this, cx| {
                if let Some(b) = this.workbench.as_ref()
                    && b.section == crate::workbench::TableSection::Indexes
                    && b.tab == WorkbenchTab::Structure
                {
                    this.open_structure_editor(b.conn, b.database.clone(), b.table.clone(), cx);
                } else {
                    this.open_field_designer(cx);
                }
            },
            cx,
        ))
        .into_any_element()
    }

    pub(super) fn workbench_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let b = self.workbench.as_ref().unwrap();
        let theme = self.theme;
        let pages = b
            .total
            .map(|total| ((total.max(0) as u64).div_ceil(u64::from(b.page_size))).max(1));
        let mut bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .min_h(px(34.))
            .flex_shrink_0()
            .px_2()
            .border_t_1()
            .border_color(theme.border)
            .text_size(px(11.))
            .text_color(theme.muted);
        for tab in WorkbenchTab::ALL {
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("wb-mode-{tab:?}")))
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(if b.tab == tab {
                        gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.15)
                    } else {
                        theme.surface
                    })
                    .text_color(if b.tab == tab {
                        gpui::Hsla::from(gpui::rgb(0x17823b))
                    } else {
                        theme.muted
                    })
                    .child(tab.label())
                    .on_click(cx.listener(move |this, _, _, cx| this.set_workbench_tab(tab, cx))),
            );
        }
        if b.result.as_ref().is_some_and(|r| r.columns.len() > 40) {
            bar = bar.child(self.edit_button(
                "wb-next-columns",
                "切换列组",
                |this, cx| {
                    if let Some(b) = this.workbench.as_mut() {
                        let pages = b
                            .result
                            .as_ref()
                            .map(|r| r.columns.len().div_ceil(40))
                            .unwrap_or(1)
                            .max(1);
                        b.column_page = (b.column_page + 1) % pages;
                    }
                    cx.notify();
                },
                cx,
            ));
        }
        bar = bar
            .child(self.edit_button(
                "wb-json-view",
                if b.text_view { "表格" } else { "JSON" },
                |this, cx| {
                    if let Some(b) = this.workbench.as_mut() {
                        b.text_view = !b.text_view;
                    }
                    cx.notify();
                },
                cx,
            ))
            .child(div().flex_1());
        if b.tab == WorkbenchTab::Data {
            bar = bar
                .child(format!(
                    "{} 行",
                    b.total.map(|n| n.to_string()).unwrap_or("—".into())
                ))
                .child(self.edit_button(
                    "wb-first",
                    "|‹",
                    |this, cx| this.jump_workbench_page(false, cx),
                    cx,
                ))
                .child(self.edit_button("wb-prev", "‹", |this, cx| this.workbench_page(-1, cx), cx))
                .child(format!(
                    "{} / {}",
                    b.page,
                    pages.map(|n| n.to_string()).unwrap_or("?".into())
                ))
                .child(self.edit_button("wb-next", "›", |this, cx| this.workbench_page(1, cx), cx))
                .child(self.edit_button(
                    "wb-last",
                    "›|",
                    |this, cx| this.jump_workbench_page(true, cx),
                    cx,
                ))
                .child(self.edit_button(
                    "wb-page-size",
                    match b.page_size {
                        50 => "50 行⌄",
                        100 => "100 行⌄",
                        _ => "200 行⌄",
                    },
                    |this, cx| {
                        if let Some(b) = this.workbench.as_mut() {
                            if b.loading || !b.edits.is_empty() {
                                b.error = Some("请先完成当前操作或撤销变更".into());
                                cx.notify();
                                return;
                            }
                            b.page_size = match b.page_size {
                                50 => 100,
                                100 => 200,
                                _ => 50,
                            };
                            b.page = 1;
                        }
                        this.fetch_workbench_data(cx);
                    },
                    cx,
                ));
        }
        bar.into_any_element()
    }

    fn jump_workbench_page(&mut self, last: bool, cx: &mut Context<Self>) {
        if let Some(b) = self.workbench.as_mut() {
            if b.loading || !b.edits.is_empty() {
                return;
            }
            b.page = if last {
                b.total
                    .map(|n| (n.max(0) as u64).div_ceil(u64::from(b.page_size)).max(1) as u32)
                    .unwrap_or(b.page)
            } else {
                1
            };
        }
        self.fetch_workbench_data(cx);
    }
}
