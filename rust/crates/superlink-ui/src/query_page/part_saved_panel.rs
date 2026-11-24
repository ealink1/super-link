use super::*;

impl QueryPage {
    /// 右侧已存查询面板：输入名称保存当前 SQL，或打开、删除已有条目。
    pub(super) fn saved_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(SAVED_WIDTH))
            .h_full()
            .bg(theme.sidebar)
            .border_l_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .h(px(36.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(icons::icon("superlink/sql-doc.svg", 13.0, theme.muted))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(if self.saved_context.is_some() {
                                        "已存查询 · 当前数据库"
                                    } else {
                                        "已存查询"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("close-saved")
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| this.close_saved(cx)))
                            .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_1()
                            .h(px(crate::theme::CONTROL_MIN))
                            .px_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .child(self.saved_title.clone()),
                    )
                    .child(
                        div()
                            .id("save-current-query")
                            .flex()
                            .items_center()
                            .justify_center()
                            .h(px(crate::theme::CONTROL_MIN))
                            .px_3()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme.accent)
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.surface)
                            .on_click(
                                cx.listener(|this, _event, _window, cx| {
                                    this.save_current_query(cx)
                                }),
                            )
                            .child("保存当前"),
                    ),
            );

        if let Some(note) = self.saved_note.clone() {
            panel = panel.child(
                div()
                    .px_3()
                    .pb_1()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(note),
            );
        }

        let items = self
            .saved_items
            .iter()
            .filter(|item| {
                self.saved_context.as_ref().is_none_or(|(profile, scope)| {
                    item.profile_id == *profile && (scope.is_empty() || item.scope == *scope)
                })
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("还没有已存查询。填写名称后点击「保存当前」。"),
            );
        }

        let mut list = div()
            .id("saved-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll();
        for item in items {
            let renaming = self
                .saved_renaming
                .as_ref()
                .map(|(rename_id, _)| rename_id == &item.id)
                .unwrap_or(false);
            if renaming {
                let editor = self
                    .saved_renaming
                    .as_ref()
                    .map(|(_, editor)| editor.clone())
                    .expect("renaming state carries an editor");
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .mx_2()
                        .my(px(1.0))
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .bg(theme.accent_soft)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .flex_1()
                                .min_w(px(0.0))
                                .h(px(28.0))
                                .px_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(editor),
                        )
                        .child(
                            div()
                                .id("confirm-saved-rename")
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(26.0))
                                .rounded_md()
                                .cursor_pointer()
                                .bg(theme.accent)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.commit_saved_rename(cx)
                                }))
                                .child(icons::icon("shell/check.svg", 13.0, theme.surface)),
                        )
                        .child(
                            div()
                                .id("cancel-saved-rename")
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(26.0))
                                .rounded_md()
                                .cursor_pointer()
                                .hover(move |style| style.bg(theme.accent_soft))
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.cancel_saved_rename(cx)
                                }))
                                .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                        ),
                );
                continue;
            }

            let id = item.id.clone();
            let open_id = item.id.clone();
            let rename_id = item.id.clone();
            let rename_title = item.title.clone();
            let title = SharedString::from(if item.title.is_empty() {
                "未命名查询".to_string()
            } else {
                item.title.clone()
            });
            let meta = match (item.scope.trim().is_empty(), item.updated_at) {
                (true, Some(time)) => time
                    .with_timezone(&chrono::Local)
                    .format("%m/%d %H:%M")
                    .to_string(),
                (false, Some(time)) => format!(
                    "{} · {}",
                    item.scope,
                    time.with_timezone(&chrono::Local).format("%m/%d %H:%M")
                ),
                (true, None) => "—".to_string(),
                (false, None) => item.scope.clone(),
            };
            list = list.child(
                div()
                    .id(SharedString::from(format!("saved-{}", item.id)))
                    .flex()
                    .items_center()
                    .gap_1()
                    .mx_2()
                    .my(px(1.0))
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .pr_1()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.muted)
                                    .child(SharedString::from(meta)),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("rename-saved-{}", item.id)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(26.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.start_saved_rename(rename_id.clone(), rename_title.clone(), cx)
                            }))
                            .child(icons::icon("superlink/pencil.svg", 13.0, theme.muted)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("open-saved-{}", item.id)))
                            .px_2()
                            .h(px(26.0))
                            .flex()
                            .items_center()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme.button)
                            .text_size(px(12.0))
                            .text_color(theme.text)
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.load_saved(open_id.clone(), cx)
                            }))
                            .child("打开"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("delete-saved-{}", item.id)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(26.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.delete_saved(id.clone(), cx)
                            }))
                            .child(icons::icon("superlink/trash.svg", 13.0, theme.muted)),
                    ),
            );
        }
        panel.child(list)
    }

    pub(super) fn history_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(HISTORY_WIDTH))
            .h_full()
            .bg(theme.sidebar)
            .border_l_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .h(px(36.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(icons::icon("superlink/history.svg", 13.0, theme.muted))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("执行历史"),
                            ),
                    )
                    .child(
                        div()
                            .id("clear-history")
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.clear_history(cx)),
                            )
                            .child(icons::icon("superlink/trash.svg", 13.0, theme.muted)),
                    ),
            );

        if self.entries.is_empty() {
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("还没有执行记录"),
            );
        }

        for entry in &self.entries {
            let theme = self.theme;
            let time = entry
                .created_at
                .map(|value| {
                    value
                        .with_timezone(&chrono::Local)
                        .format("%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();
            let line = entry.text.lines().next().unwrap_or("");
            let text = if line.chars().count() > 48 {
                format!("{}…", line.chars().take(48).collect::<String>())
            } else {
                line.to_string()
            };
            let entry_id = entry.id;
            let redacted = entry.text.clone();
            panel = panel.child(
                div()
                    .id(SharedString::from(format!("history-{}", entry.id)))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mx_2()
                    .my_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.load_history(entry_id, redacted.clone(), cx)
                    }))
                    .bg(theme.surface)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(if entry.success {
                                        theme.accent
                                    } else {
                                        theme.danger
                                    })
                                    .child(if entry.success { "成功" } else { "失败" }),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(time),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(text)),
                    ),
            );
        }
        panel
    }

    pub(super) fn editor_text(entity: &Entity<SqlEditor>, cx: &Context<Self>) -> String {
        entity.read(cx).text().to_string()
    }

    pub fn set_on_saved(
        &mut self,
        handler: impl Fn(&str, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.connection_form
            .update(cx, |form, cx| form.set_on_saved(handler, cx));
    }

    pub(super) fn export_result_csv(&mut self, cx: &mut Context<Self>) {
        self.export_open = !self.export_open;
        cx.notify();
    }

    pub fn load_table_query(
        &mut self,
        profile: Profile,
        database: String,
        sql: String,
        cx: &mut Context<Self>,
    ) {
        self.select_profile(profile, cx);
        self.scope = database;
        self.load_sql(sql, cx);
    }

    /// 从结构树点选表后回填查询。
    pub fn load_sql(&mut self, sql: String, cx: &mut Context<Self>) {
        self.stop_query(cx);
        self.editor
            .update(cx, |editor, cx| editor.set_text(sql, cx));
        self.redacted_notice = None;
        self.confirm_run = false;
        self.status = "已回填表查询".into();
        cx.notify();
    }

    pub fn open_connection_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.connection_form
            .update(cx, |form, cx| form.open_new(window, cx));
        cx.notify();
    }

    /// 编辑当前选中的连接。
    pub fn open_edit_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            self.status = "请先选择连接".into();
            cx.notify();
            return;
        };
        self.connection_form
            .update(cx, |form, cx| form.open_edit(profile, window, cx));
        cx.notify();
    }

    pub fn close_connection_form(&mut self, cx: &mut Context<Self>) {
        self.connection_form.update(cx, |form, cx| form.close(cx));
        cx.notify();
    }
}
