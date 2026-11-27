use super::*;

impl QueryPage {
    pub(super) fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let profile_label = self
            .profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| "未选择连接".to_string());
        let profile_kind = self
            .profile
            .as_ref()
            .map(|profile| profile.sql_dialect().to_string())
            .unwrap_or_default();
        let limit = self.settings.row_limit();
        let next_limit = match limit {
            100 => 500,
            500 => 1_000,
            1_000 => 5_000,
            5_000 => 10_000,
            _ => 100,
        };

        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(42.0))
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            // 连接下拉
            .child(
                div()
                    .id("profile-pill")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(crate::theme::CONTROL_MIN))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.profiles_open = !this.profiles_open;
                        cx.notify();
                    }))
                    .child(icons::icon(
                        icons::datasource_icon(profile_kind.as_str()),
                        14.0,
                        theme.muted,
                    ))
                    .child(
                        div()
                            .max_w(px(180.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(crate::text::shorten(&profile_label, 18))),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("▾"),
                    ),
            )
            // 编辑当前连接
            .children(self.profile.is_some().then(|| {
                div()
                    .id("edit-connection")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(30.0))
                    .h(px(crate::theme::CONTROL_MIN))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(
                        cx.listener(|this, _event, window, cx| {
                            this.open_edit_connection(window, cx)
                        }),
                    )
                    .child(icons::icon("superlink/pencil.svg", 14.0, theme.muted))
            }))
            // 数据库 / Namespace：输入框兼作搜索框，候选弹层宽度与输入框一致。
            .child(
                div()
                    .relative()
                    .w(px(SCOPE_BUTTON_WIDTH))
                    .flex_shrink_0()
                    .child(
                        div()
                            .on_children_prepainted({
                                let slot = self.scope_input_bounds.clone();
                                move |bounds, _, _| {
                                    if let Some(bounds) = bounds.first() {
                                        *slot.borrow_mut() = Some(*bounds);
                                    }
                                }
                            })
                            .w_full()
                            .h(px(crate::theme::CONTROL_MIN))
                            .px_2()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded_md()
                            .border_1()
                            .border_color(if self.scope_open {
                                theme.accent
                            } else {
                                theme.border
                            })
                            .bg(theme.surface)
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(
                                div()
                                    .id("query-scope-field")
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .cursor(gpui::CursorStyle::IBeam)
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, window, cx| {
                                            this.focus_scope_editor(window, cx);
                                        }),
                                    )
                                    .child(self.scope_search.clone()),
                            )
                            .child(
                                div()
                                    .id("query-scope-toggle")
                                    .flex_shrink_0()
                                    .cursor_pointer()
                                    .on_click(cx.listener(
                                        |this, _event: &gpui::ClickEvent, window, cx| {
                                            this.toggle_scope(window, cx);
                                        },
                                    ))
                                    .child(icons::icon(
                                        "shell/chevron-down.svg",
                                        12.0,
                                        theme.muted,
                                    )),
                            ),
                    )
                    .children(self.scope_picker(cx)),
            )
            // 行数上限（点击轮换）
            .child(
                div()
                    .id("limit-pill")
                    .px_2()
                    .h(px(crate::theme::CONTROL_MIN))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.settings.set_row_limit(next_limit);
                        this.status = format!("结果行数上限 → {}", next_limit).into();
                        cx.notify();
                    }))
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(SharedString::from(format!("{limit} ▾"))),
            )
            .children(self.running.then(|| {
                div()
                    .id("stop-query")
                    .cursor_pointer()
                    .px_2()
                    .text_color(theme.danger)
                    .on_click(cx.listener(|this, _, _, cx| this.stop_query(cx)))
                    .child("停止")
            }))
            // 运行：蓝色圆形按钮
            .child(
                div()
                    .id("run-query")
                    .size(px(crate::theme::CONTROL_MIN))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(if self.running {
                        theme.border
                    } else {
                        theme.accent
                    })
                    .on_click(cx.listener(|this, _event, _window, cx| this.run(cx)))
                    .child(icons::icon("superlink/send.svg", 14.0, theme.surface)),
            )
            // 保存：圆形图标按钮
            .child(
                div()
                    .id("save-query")
                    .size(px(crate::theme::CONTROL_MIN))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.button)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.save_active(cx);
                        this.status = "已保存到草稿".into();
                        cx.notify();
                    }))
                    .child(icons::icon("shell/save.svg", 14.0, theme.muted)),
            )
            // 导出 CSV：圆形图标按钮
            .child(
                div()
                    .id("export-csv")
                    .size(px(crate::theme::CONTROL_MIN))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.button)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.export_result_csv(cx)))
                    .child(icons::icon("superlink/copy.svg", 14.0, theme.muted)),
            )
            // 执行历史：圆形图标按钮
            .child(
                div()
                    .id("history")
                    .size(px(crate::theme::CONTROL_MIN))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border)
                    .bg(if self.history_open {
                        theme.accent_soft
                    } else {
                        theme.button
                    })
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.toggle_history(cx)))
                    .child(icons::icon(
                        "superlink/history.svg",
                        14.0,
                        if self.history_open {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(self.status.clone()),
            )
    }

    /// 编辑器与结果区之间的状态行（对齐 Fyne 时期的位置）。
    pub(super) fn status_row(&self) -> impl IntoElement {
        let theme = self.theme;
        let limit = self.settings.row_limit().max(100);
        let text = if self.status.is_empty() {
            SharedString::from(format!("就绪 · 本次最多 {limit} 行"))
        } else {
            self.status.clone()
        };
        div()
            .px_3()
            .py_1()
            .text_size(px(crate::theme::TEXT_SIZE))
            .text_color(theme.muted)
            .bg(theme.surface)
            .border_t_1()
            .border_color(theme.border)
            .child(text)
    }
    pub(super) fn editor_panel(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .gap_1()
            .h(px(self.editor_height))
            .border_b_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .child(self.editor.clone())
    }

    pub(super) fn result_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .h(px(32.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .children(ResultTab::ALL.iter().map(|tab| {
                        let active = *tab == self.result_tab;
                        div()
                            .id(SharedString::from(format!("result-tab-{:?}", tab)))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.success } else { theme.muted })
                            .font_weight(if active {
                                FontWeight::BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .bg(if active {
                                theme.success_soft
                            } else {
                                theme.surface
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.result_tab = *tab;
                                cx.notify();
                            }))
                            .child(tab.label())
                    })),
            );

        let content: AnyElement = match self.result_tab {
            ResultTab::Log => {
                let mut log = div().flex().flex_col().gap_2().p_3();
                match (&self.last_sql, &self.error) {
                    (Some(sql), error) => {
                        let status_text: String = if let Some(err) = error {
                            err.to_string()
                        } else {
                            format!(
                                "成功 · {:.1} ms",
                                self.last_duration
                                    .map(|ns| ns as f64 / 1_000_000.0)
                                    .unwrap_or_default()
                            )
                        };
                        log = log.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child("状态"),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(if error.is_some() {
                                            theme.danger
                                        } else {
                                            theme.text
                                        })
                                        .child(SharedString::from(status_text)),
                                ),
                        );
                        log = log.child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("语句"),
                        );
                        log = log.child(
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(theme.canvas)
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(SharedString::from(sql.clone())),
                        );
                    }
                    _ => {
                        log = log.child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("还没有执行记录"),
                        );
                    }
                }
                log.into_any_element()
            }
            ResultTab::Params => div()
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child("填写 JSON 对象绑定 :name，或 JSON 数组绑定位置参数")
                .child(self.parameters.clone())
                .into_any_element(),
            ResultTab::Fields => {
                let Some(result) = self.result.as_ref() else {
                    return panel
                        .child(
                            div()
                                .flex_1()
                                .p_3()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("执行查询后显示字段"),
                        )
                        .into_any_element();
                };
                let mut fields = div().flex().flex_col().gap_1().p_3();
                for (position, column) in result.columns.iter().enumerate() {
                    fields = fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(28.0))
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(format!("{}", position + 1)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(SharedString::from(column.name.clone())),
                            ),
                    );
                }
                fields.into_any_element()
            }
            ResultTab::Ddl => {
                let Some(sql) = self.last_sql.as_ref() else {
                    return panel
                        .child(
                            div()
                                .flex_1()
                                .p_3()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("执行查询后显示语句"),
                        )
                        .into_any_element();
                };
                div()
                    .p_3()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(SharedString::from(sql.clone()))
                    .into_any_element()
            }
            ResultTab::Result => {
                if let Some(error) = self.error.clone() {
                    return panel
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.danger)
                                .child(error),
                        )
                        .into_any_element();
                }
                let Some(result) = self.result.as_ref() else {
                    return panel
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("执行查询后显示结果"),
                        )
                        .into_any_element();
                };

                self.result_grid(result, cx)
            }
        };
        panel.child(content).into_any_element()
    }
}
