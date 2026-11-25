//! 连接表单的视图：数据源选择器与四个分区（基本 / 网络与安全 / 外观 / 高级）。

use super::*;
use crate::theme::TEXT_SIZE;
use gpui::{MouseButton, relative};

impl ConnectionForm {
    /// 选择器弹层：点击卡片直接选定类型。
    pub(super) fn picker_overlay(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let picker = self.picker.read(cx);
        let categories = picker::CATEGORIES.map(|category| (category, picker.count(category)));
        let active = picker.category();
        let matches = picker.matches(cx);
        let empty = matches.is_empty();

        let mut nav = div().flex().flex_col().gap_1().w(px(180.0)).flex_shrink_0();
        for (category, count) in categories {
            let on = category == active;
            nav = nav.child(
                div()
                    .id(SharedString::from(format!("picker-nav-{category}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py(px(6.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if on { theme.accent_soft } else { theme.canvas })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.picker
                            .update(cx, |picker, cx| picker.set_category(category, cx));
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(TEXT_SIZE))
                            .font_weight(if on {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if on { theme.text } else { theme.muted })
                            .child(category),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child(count.to_string()),
                    ),
            );
        }

        let mut grid = div()
            .id("picker-grid")
            .flex()
            .flex_wrap()
            .gap_2()
            .flex_1()
            .min_w(px(0.0))
            .overflow_y_scroll()
            .scrollbar_width(px(8.0));
        for descriptor in &matches {
            let key = descriptor.key;
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("picker-card-{key}")))
                    .w(px(206.0))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_kind(key, window, cx);
                    }))
                    .child(picker::source_card(descriptor, theme)),
            );
        }
        if empty {
            grid = grid.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .flex_1()
                    .py_8()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("没有匹配的数据源"),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("试试其他名称，或切换左侧分类"),
                    ),
            );
        }

        let search = self.picker.read(cx).search_editor();
        let selected_label = picker::category_label(active).to_string();
        let total = picker.count(active);

        div()
            .id("source-picker-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000044))
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                // 点击遮罩等同关闭：避免停留在空白的表单页。
                cx.listener(|this, _, _, cx| this.close(cx)),
            )
            .child(
                div()
                    .id("source-picker-card")
                    .flex()
                    .flex_col()
                    .w(px(900.0))
                    .max_w(relative(0.94))
                    .h(px(600.0))
                    .max_h(relative(0.92))
                    .rounded_lg()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .shadow(vec![gpui::BoxShadow {
                        color: gpui::rgba(0x00000033).into(),
                        offset: gpui::point(px(0.0), px(8.0)),
                        blur_radius: px(24.0),
                        spread_radius: px(0.0),
                    }])
                    .text_color(theme.text)
                    .text_size(px(TEXT_SIZE))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        // 页头：品牌、步骤条、关闭
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .h(px(52.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .size(px(26.0))
                                            .rounded_md()
                                            .bg(theme.accent)
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(theme.surface)
                                                    .child("SL"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(15.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("SuperLink"),
                                    ),
                            )
                            .child(div().flex_1())
                            .child(picker::step_strip(0, theme))
                            .child(
                                div()
                                    .id("picker-close")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(24.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close(cx);
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h(px(0.0))
                            .gap_3()
                            .p_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .w(px(196.0))
                                    .flex_shrink_0()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .px_2()
                                            .h(px(30.0))
                                            .rounded_md()
                                            .border_1()
                                            .border_color(theme.border)
                                            .bg(theme.canvas)
                                            .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                                            .child(search),
                                    )
                                    .child(nav),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(15.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(selected_label),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(theme.muted)
                                                    .child(format!("{total} 个类型")),
                                            )
                                            .child(div().flex_1())
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(theme.muted)
                                                    .child(format!(
                                                        "单击进入配置表单 · 窗口 {:.0}×{:.0}",
                                                        f32::from(window.viewport_size().width),
                                                        f32::from(window.viewport_size().height)
                                                    )),
                                            ),
                                    )
                                    .child(grid),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_4()
                            .h(px(42.0))
                            .border_t_1()
                            .border_color(theme.border)
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child("JVM 连接器不在本版本范围内；选中类型后可在「网络与安全」配置 SSH、代理与 TLS。"),
                    ),
            )
            .into_any_element()
    }
}
