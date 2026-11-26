use super::*;
use crate::workbench::WorkbenchTab;

pub(super) const ROW_HEIGHT: f32 = 27.;
pub(super) const COLUMN_PAGE_SIZE: usize = 40;
pub(super) const INDEX_COLUMN_WIDTH: f32 = 70.;

/// 与 `workbench_grid` 相同的列宽规则。
fn column_width(bench: &crate::workbench::Workbench, index: usize) -> f32 {
    bench
        .column_widths
        .get(&index)
        .copied()
        .unwrap_or(super::workbench_grid::DEFAULT_COLUMN_WIDTH)
}

impl SuperLinkApp {
    /// Cmd/Ctrl+F 在当前数据页打开查找框；已打开时重新聚焦并选中已有文本。
    pub(super) fn open_workbench_find(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bench) = self.workbench.as_ref() else {
            return false;
        };
        if bench.tab != WorkbenchTab::Data || bench.text_view || bench.result.is_none() {
            return false;
        }
        if let Some(bench) = self.workbench.as_mut() {
            bench.find.open = true;
        }
        use gpui::Focusable as _;
        let handle = self.wb_find.read(cx).focus_handle(cx);
        window.focus(&handle);
        cx.notify();
        true
    }

    pub(super) fn close_workbench_find(&mut self, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.find.close();
        }
        self.wb_find
            .update(cx, |editor, cx| editor.set_text("", cx));
        cx.notify();
    }

    /// 在命中项之间移动；`delta` 为 +1 下一个、-1 上一个，循环。
    pub(super) fn step_workbench_find(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_mut() else {
            return;
        };
        let Some(result) = bench.result.as_ref() else {
            return;
        };
        let hits = bench.find.matches(result);
        if hits.is_empty() {
            bench.find.current = 0;
        } else {
            let len = hits.len();
            let next = (bench.find.current as isize + delta).rem_euclid(len as isize) as usize;
            bench.find.current = next;
        }
        // 命中所在的列页不在当前显示范围时切换列组，再让整格滚进视口。
        if let Some(&(row, column)) = hits.get(bench.find.current) {
            let page = column / COLUMN_PAGE_SIZE;
            if bench.column_page != page {
                bench.column_page = page;
            }
            let page_start =
                (bench.column_page * COLUMN_PAGE_SIZE).min(result.columns.len().saturating_sub(1));
            let mut left = INDEX_COLUMN_WIDTH;
            for index in page_start..column {
                left += column_width(bench, index);
            }
            let cell_width = column_width(bench, column);
            let vertical = bench.vertical.clone();
            let top = px(row as f32 * ROW_HEIGHT);
            let viewport = vertical.bounds().size.height;
            let visible_top = -vertical.offset().y;
            if top < visible_top {
                vertical.set_offset(point(vertical.offset().x, -top));
            } else if top + px(ROW_HEIGHT) > visible_top + viewport {
                vertical.set_offset(point(
                    vertical.offset().x,
                    -(top + px(ROW_HEIGHT) - viewport),
                ));
            }
            let horizontal = bench.horizontal.clone();
            let visible_width = horizontal.bounds().size.width;
            let visible_left = -horizontal.offset().x;
            let left = px(left);
            let right = left + px(cell_width);
            if left < visible_left {
                horizontal.set_offset(point(-left, horizontal.offset().y));
            } else if right > visible_left + visible_width {
                horizontal.set_offset(point(-(right - visible_width), horizontal.offset().y));
            }
            bench.inspected = Some((row, column));
        }
        cx.notify();
    }

    /// 浮在数据网格右上角的查找条：输入框、命中计数、上一个/下一个、关闭。
    pub(super) fn workbench_find_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        // `edit_button` 需要可变上下文，这里先把按钮占位算出来，避免与
        // 只读借用 `bench` 冲突。
        let prev = self.find_button(
            "wb-find-prev",
            "shell/chevron-right.svg",
            true,
            |this, cx| this.step_workbench_find(-1, cx),
            cx,
        );
        let next = self.find_button(
            "wb-find-next",
            "shell/chevron-right.svg",
            false,
            |this, cx| this.step_workbench_find(1, cx),
            cx,
        );
        let close = self.find_button(
            "wb-find-close",
            "shell/x.svg",
            false,
            |this, cx| this.close_workbench_find(cx),
            cx,
        );
        let theme = self.theme;
        let Some(bench) = self.workbench.as_ref() else {
            return div().into_any_element();
        };
        let find = &bench.find;
        let count = bench
            .result
            .as_ref()
            .map(|result| find.matches(result).len())
            .unwrap_or(0);
        let label = if find.query.trim().is_empty() {
            String::new()
        } else if count == 0 {
            "无匹配".to_string()
        } else {
            format!("{} / {}", find.current.min(count - 1) + 1, count)
        };
        let muted = theme.muted;
        div()
            .absolute()
            .top(px(6.))
            .right(px(10.))
            .flex()
            .items_center()
            .gap_2()
            .h(px(crate::theme::CONTROL_MIN))
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .shadow(vec![gpui::BoxShadow {
                color: gpui::rgba(0x00000026).into(),
                offset: point(px(0.), px(4.)),
                blur_radius: px(12.),
                spread_radius: px(0.),
            }])
            .occlude()
            .text_size(px(11.))
            .child(icons::icon("shell/search.svg", 13.0, muted))
            .child(div().w(px(180.)).child(self.wb_find.clone()))
            .child(div().w(px(48.)).text_color(muted).child(label))
            .child(prev)
            .child(next)
            .child(close)
            .into_any_element()
    }

    fn find_button(
        &self,
        id: &'static str,
        icon: &'static str,
        rotate: bool,
        handler: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut glyph = gpui::svg()
            .path(icon)
            .w(px(12.))
            .h(px(12.))
            .text_color(theme.muted);
        if rotate {
            glyph = glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(
                std::f32::consts::PI,
            )));
        }
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .w(px(22.))
            .h(px(22.))
            .rounded_sm()
            .cursor_pointer()
            .hover(move |s| s.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _, _, cx| handler(this, cx)))
            .child(glyph)
            .into_any_element()
    }
}
