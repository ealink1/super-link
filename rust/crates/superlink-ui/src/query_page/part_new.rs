use super::*;

impl QueryPage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profiles: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        history: Arc<HistoryService>,
        saved: Arc<SavedQueryService>,
        settings: Arc<SettingsService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection_form = cx.new(|cx| {
            ConnectionForm::new(
                profiles.clone(),
                queries.clone(),
                settings.clone(),
                theme,
                cx,
            )
        });
        let saved_title = cx.new(SqlEditor::new);
        saved_title.update(cx, |editor, cx| editor.set_placeholder("查询名称", cx));
        let page = Self {
            protocol: protocols::ProtocolControls::new(cx),
            export_operation: None,
            export_open: false,
            other_results: Vec::new(),
            selected_result: 0,
            operation: None,
            request_generation: 0,
            draft_generation: 0,
            library_generation: 0,
            library_busy: false,
            profile_choices: Vec::new(),
            parameters: cx.new(|cx| {
                let mut e = SqlEditor::new(cx);
                e.set_placeholder("参数 JSON，例如 {\"id\": 1} 或 [1]", cx);
                e
            }),
            scope_input: cx.new(SqlEditor::new),
            schema_input: cx.new(SqlEditor::new),
            scope_open: false,
            profiles_open: false,
            scope_options: Vec::new(),
            scope: String::new(),
            schema: String::new(),
            editor_height: 320.0,
            resizing: None,
            display_page: 0,
            column_page: 0,
            column_widths: HashMap::new(),
            column_drag: None,
            inspected: None,
            profiles,
            queries,
            drafts,
            history,
            settings,
            theme,
            editor: cx.new(|cx| {
                let mut editor = SqlTextEditor::new(cx);
                editor.set_theme(theme, cx);
                editor
            }),
            profile: None,
            tab_cache: HashMap::new(),
            tabs: Vec::new(),
            active: 0,
            result: None,
            result_tab: ResultTab::default(),
            last_sql: None,
            last_request: None,
            last_duration: None,
            error: None,
            status: "选择连接后执行查询".into(),
            running: false,
            history_open: false,
            entries: Vec::new(),
            recent_sql: HashMap::new(),
            redacted_notice: None,
            confirm_run: false,
            connection_form,
            saved,
            saved_open: false,
            saved_context: None,
            saved_items: Vec::new(),
            saved_title,
            saved_renaming: None,
            saved_note: None,
        };
        let weak_page = cx.entity().downgrade();
        page.editor.update(cx, move |editor, cx| {
            editor.set_on_change(
                move |text, app| {
                    let weak = weak_page.clone();
                    let text = text.to_string();
                    app.defer(move |app| {
                        let _ = weak.update(app, |this, cx| {
                            if let Some(tab) = this.tabs.get_mut(this.active) {
                                tab.text = text;
                            }
                            this.draft_generation = this.draft_generation.wrapping_add(1);
                            let generation = this.draft_generation;
                            cx.spawn(async move |this, cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(400))
                                    .await;
                                let _ = this.update(cx, |this, cx| {
                                    if this.draft_generation == generation {
                                        this.save_active(cx);
                                    }
                                });
                            })
                            .detach();
                        });
                    });
                },
                cx,
            );
        });
        let loader = page.profiles.clone();
        let task = cx.background_executor().spawn(async move { loader.list() });
        cx.spawn(async move |this, cx| {
            if let Ok(profiles) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.profile_choices = profiles;
                    cx.notify();
                });
            }
        })
        .detach();
        // Cmd+R in the editor runs the statement, matching the toolbar label.
        let weak = cx.entity().downgrade();
        page.editor.update(cx, move |editor, cx| {
            editor.set_on_run(
                move |_window, app| {
                    let weak = weak.clone();
                    app.defer(move |app| {
                        let _ = weak.update(app, |page, cx| page.run(cx));
                    });
                },
                cx,
            );
        });
        page
    }

    /// Hands the editor the cached table/column snapshot from the object tree.
    pub fn set_catalog(&mut self, tables: Vec<CompletionTable>, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_tables(tables, cx));
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.editor
            .update(cx, |editor, cx| editor.set_theme(theme, cx));
        self.connection_form
            .update(cx, |form, cx| form.set_theme(theme, cx));
        cx.notify();
    }

    pub fn forget_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        self.tab_cache.remove(id);
        self.profile_choices.retain(|p| p.id != id);
        if self.profile.as_ref().is_some_and(|p| p.id == id) {
            self.stop_query(cx);
            self.profile = None;
            self.tabs.clear();
            self.result = None;
            self.other_results.clear();
            self.editor
                .update(cx, |e, cx| e.set_text(String::new(), cx));
        }
    }
    pub fn select_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        if self
            .profile
            .as_ref()
            .is_some_and(|p| p.id == profile.id && p.revision == profile.revision)
        {
            return;
        }
        self.save_active(cx);
        self.store_output(cx);
        if let Some(previous) = self.profile.as_ref() {
            self.tab_cache.insert(
                previous.id.clone(),
                (std::mem::take(&mut self.tabs), self.active),
            );
        }
        if self.tab_cache.len() > 8
            && let Some(id) = self.tab_cache.keys().find(|id| **id != profile.id).cloned()
        {
            self.tab_cache.remove(&id);
        }
        self.stop_query(cx);
        if let Some(existing) = self.profile_choices.iter_mut().find(|p| p.id == profile.id) {
            *existing = profile.clone();
        } else {
            self.profile_choices.push(profile.clone());
        }
        self.profiles_open = false;
        self.scope = profile.config.database.clone();
        self.schema.clear();
        let supported = self.queries.supports(profile.sql_dialect());
        self.redacted_notice = None;
        self.confirm_run = false;
        (self.tabs, self.active) = self
            .tab_cache
            .remove(&profile.id)
            .unwrap_or_else(|| (self.load_tabs(&profile), 0));
        let text = self
            .tabs
            .get(self.active)
            .map(|tab| tab.text.clone())
            .unwrap_or_default();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        let dialect = profile.sql_dialect().to_string();
        let placeholder = protocols::editor_placeholder(&profile.config.kind);
        self.editor.update(cx, |editor, cx| {
            editor.set_dialect(&dialect, cx);
            editor.set_placeholder(placeholder, cx);
        });
        cx.emit(super::QueryProfileChanged(profile.id.clone()));
        self.protocol.name.update(cx, |e, cx| e.set_text("", cx));
        self.protocol.value.update(cx, |e, cx| e.set_text("", cx));
        self.protocol.group.update(cx, |e, cx| e.set_text("", cx));
        self.profile = Some(profile);
        self.restore_output(cx);
        self.trim_results();
        self.status = if supported {
            "就绪 · 结果按行数上限截断".into()
        } else {
            "该数据源尚未提供 Rust 驱动 agent".into()
        };
        cx.notify();
    }

    pub(super) fn load_tabs(&self, profile: &Profile) -> Vec<QueryTab> {
        let mut tabs: Vec<QueryTab> = self
            .drafts
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|draft| draft.profile_id == profile.id && !draft.closed)
            .map(|draft| QueryTab {
                id: draft.id,
                title: draft.title,
                text: draft.text,
                output: None,
                other_results: Vec::new(),
                selected_result: 0,
                error: None,
                last_sql: None,
                last_request: None,
                last_duration: None,
                parameters: String::new(),
                scope: draft.scope,
                schema: draft.schema,
            })
            .collect();
        if tabs.is_empty() {
            tabs.push(Self::new_tab(profile, 1));
        }
        tabs
    }

    pub(super) fn new_tab(profile: &Profile, index: usize) -> QueryTab {
        QueryTab {
            id: format!("query-tab-{}", uuid::Uuid::new_v4()),
            title: format!("查询 {index}"),
            text: String::new(),
            output: None,
            other_results: Vec::new(),
            selected_result: 0,
            error: None,
            last_sql: None,
            last_request: None,
            last_duration: None,
            parameters: String::new(),
            scope: profile.config.database.clone(),
            schema: String::new(),
        }
    }

    pub(super) fn stash(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_string();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.text = text;
        }
    }

    pub(super) fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.active || index >= self.tabs.len() {
            return;
        }
        self.save_active(cx);
        self.stop_query(cx);
        self.store_output(cx);
        self.active = index;
        self.restore_output(cx);
        self.redacted_notice = None;
        self.confirm_run = false;
        let text = self.tabs[index].text.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        cx.notify();
    }

    pub fn add_tab(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        self.save_active(cx);
        self.redacted_notice = None;
        self.confirm_run = false;
        self.stop_query(cx);
        self.store_output(cx);
        let tab = Self::new_tab(&profile, self.tabs.len() + 1);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.restore_output(cx);
        self.editor
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        cx.notify();
    }

    pub(super) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.stop_query(cx);
        self.save_active(cx);
        if index >= self.tabs.len() {
            return;
        }
        self.store_output(cx);
        let tab = self.tabs.remove(index);
        let _ = self.drafts.delete(&tab.id);
        if self.tabs.is_empty() {
            self.active = 0;
            self.restore_output(cx);
            self.editor
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            cx.notify();
            return;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        self.restore_output(cx);
        let text = self.tabs[self.active].text.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        cx.notify();
    }

    pub(super) fn save_active(&mut self, cx: &mut Context<Self>) {
        self.stash(cx);
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let draft = Draft {
            id: tab.id.clone(),
            profile_id: profile.id,
            title: tab.title.clone(),
            scope: self.scope.clone(),
            schema: self.schema.clone(),
            text: tab.text.clone(),
            ..Default::default()
        };
        if let Err(error) = self.drafts.save(&draft) {
            self.error = Some(format!("草稿保存失败：{error}").into());
            cx.notify();
        }
    }

    pub(super) fn load_history(&mut self, id: i64, redacted: String, cx: &mut Context<Self>) {
        let from_session = self.recent_sql.get(&id).cloned();
        let placeholders = placeholder_count(&redacted);
        let (text, note) = resolve_history_text(from_session.clone(), redacted);
        self.redacted_notice = from_session.is_none().then_some(placeholders);
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        self.status = note.into();
        cx.notify();
    }

    pub(super) fn redacted_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let count = self.redacted_notice.unwrap_or(0);
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(theme.warning_bg)
            .border_b_1()
            .border_l_2()
            .border_color(theme.warning_border)
            .child(icons::icon(
                "shell/circle-help.svg",
                15.0,
                theme.warning_text,
            ))
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.warning_text)
                    .child(format!(
                        "该语句来自历史记录，{count} 处字面量已被替换为 ?，执行前请手动补值"
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("dismiss-redacted")
                    .px_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.warning_border))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.redacted_notice = None;
                        cx.notify();
                    }))
                    .child(icons::icon("shell/x.svg", 12.0, theme.warning_text)),
            )
    }

    pub(super) fn confirm_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let placeholders = placeholder_count(self.editor.read(cx).text());
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(theme.warning_bg)
            .border_b_1()
            .border_l_2()
            .border_color(theme.warning_border)
            .child(icons::icon(
                "shell/circle-help.svg",
                15.0,
                theme.warning_text,
            ))
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.warning_text)
                    .child(format!(
                        "执行前请检查语句：将修改数据或结构，或仍含 {placeholders} 处待填写的占位符。确认执行？"
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("cancel-run")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.warning_border)
                    .on_click(cx.listener(|this, _event, _window, cx| this.cancel_execution(cx)))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.warning_text)
                            .child("取消"),
                    ),
            )
            .child(
                div()
                    .id("confirm-run")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.warning_text)
                    .on_click(cx.listener(|this, _event, _window, cx| this.confirm_execution(cx)))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.surface)
                            .child("仍然执行"),
                    ),
            )
    }

    pub fn toggle_history(&mut self, cx: &mut Context<Self>) {
        self.history_open = !self.history_open;
        if self.history_open {
            self.reload_history(cx);
        }
        cx.notify();
    }

    pub fn saved_open(&self) -> bool {
        self.saved_open
    }

    pub fn open_saved_for(
        &mut self,
        profile: Profile,
        scope: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = profile.id.clone();
        self.select_profile(profile, cx);
        self.open_saved(window, cx);
        if !scope.is_empty() {
            self.scope = scope.clone();
        }
        self.saved_context = Some((id, scope));
        cx.notify();
    }

    pub fn open_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.saved_context = None;
        self.saved_open = true;
        self.history_open = false;
        use gpui::Focusable as _;
        let handle = self.saved_title.read(cx).focus_handle(cx);
        window.focus(&handle);
        self.refresh_saved(cx);
    }

    pub(super) fn close_saved(&mut self, cx: &mut Context<Self>) {
        self.saved_open = false;
        cx.notify();
    }

    pub(super) fn start_saved_rename(&mut self, id: String, title: String, cx: &mut Context<Self>) {
        let editor = cx.new(SqlEditor::new);
        editor.update(cx, |editor, cx| {
            editor.set_placeholder("查询名称", cx);
            editor.set_text(title, cx);
        });
        self.saved_renaming = Some((id, editor));
        cx.notify();
    }

    pub(super) fn cancel_saved_rename(&mut self, cx: &mut Context<Self>) {
        self.saved_renaming = None;
        cx.notify();
    }
}
