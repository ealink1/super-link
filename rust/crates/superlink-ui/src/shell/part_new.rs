use super::*;

impl SuperLinkApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        history: Arc<HistoryService>,
        saved: Arc<SavedQueryService>,
        structure: Arc<StructureService>,
        groups: Arc<superlink_app::GroupService>,
        notebook: Arc<NotebookService>,
        hosts: Arc<HostService>,
        settings: Arc<SettingsService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = ThemeMode::from_key(&settings.theme_mode());
        let resolved_dark = window.appearance() == gpui::WindowAppearance::Dark;
        let theme = mode.resolve(resolved_dark);
        let (profiles, error) = match service.list() {
            Ok(profiles) => (profiles, None),
            Err(err) => (Vec::new(), Some(SharedString::from(err.to_string()))),
        };
        let query_page = cx.new(|cx| {
            QueryPage::new(
                service.clone(),
                queries.clone(),
                drafts,
                history,
                saved.clone(),
                settings.clone(),
                theme,
                cx,
            )
        });
        cx.subscribe(
            &query_page,
            |this, _, event: &crate::query_page::QueryProfileChanged, cx| {
                if let Some(index) = this
                    .profiles
                    .iter()
                    .position(|profile| profile.id == event.0)
                {
                    this.selected = index;
                    this.sync_catalog(cx);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.observe(&query_page, |_, _, cx| cx.notify()).detach();
        let weak_shell = cx.entity().downgrade();
        query_page.update(cx, |page, cx| {
            page.set_on_saved(
                move |id, app| {
                    // 回调发生在 QueryPage 更新期间，先排队再更新外壳，避免重入。
                    let weak_shell = weak_shell.clone();
                    let id = id.to_string();
                    app.defer(move |app| {
                        let _ =
                            weak_shell.update(app, |shell, cx| shell.select_profile_by_id(&id, cx));
                    });
                },
                cx,
            );
        });
        let sql_search = cx.new(SqlEditor::new);
        sql_search.update(cx, |editor, cx| {
            editor.set_placeholder("搜索连接…", cx);
        });
        let object_search = cx.new(SqlEditor::new);
        object_search.update(cx, |editor, cx| {
            editor.set_placeholder("搜索当前对象…", cx);
        });
        // 过滤值直接读输入框文本，避免在回调里重入更新本实体。
        for search in [&sql_search, &object_search] {
            cx.observe(search, |_this, _editor, cx| cx.notify())
                .detach();
        }
        let note_page = cx.new(|cx| NotePage::new(notebook, theme, cx));
        let terminal_page = cx.new(|cx| ShellWorkspace::new(hosts, settings.clone(), theme, cx));
        let chat_panel = cx.new(|cx| {
            crate::chat_panel::ChatPanel::new(Arc::new(service.chat_service()), theme, cx)
        });
        let root = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, app| {
            let quitting = root.update(app, |this, _| this.quitting).unwrap_or(true);
            if quitting {
                return true;
            }
            if cfg!(target_os = "macos") {
                app.hide();
            } else {
                let _ = root.update(app, |this, cx| this.request_quit(cx));
            }
            false
        });
        let mut app = Self {
            drivers: driver_ui::DriverUi::new(cx),
            updates: Default::default(),
            designer: None,
            connection_actions: None,
            connection_delete_confirm: false,
            diagnostics: Vec::new(),
            sidebar_width: settings
                .get("sql.sidebarWidth")
                .and_then(|v| v.as_f64())
                .unwrap_or(264.)
                .clamp(200., 480.) as f32,
            sidebar_drag: None,
            chat_panel,
            chat_open: false,
            quitting: false,
            service,
            query_page,
            note_page,
            terminal_page,
            profiles,
            selected: 0,
            collapsed_groups: HashSet::new(),
            settings,
            mode,
            settings_open: false,
            sql_search: sql_search.clone(),
            object_search: object_search.clone(),
            queries: queries.clone(),
            expanded_conns: HashSet::new(),
            conn_dbs: HashMap::new(),
            conn_db_note: HashMap::new(),
            expanded_dbs: HashSet::new(),
            db_tables: HashMap::new(),
            expanded_tables: HashSet::new(),
            table_counts: HashMap::new(),
            table_structures: HashMap::new(),
            column_errors: HashMap::new(),
            structure: structure.clone(),
            groups: groups.clone(),
            groups_open: false,
            group_busy: false,
            group_new: {
                let editor = cx.new(SqlEditor::new);
                editor.update(cx, |editor, cx| editor.set_placeholder("新分组名", cx));
                editor
            },
            group_rename: None,
            declared_groups: Vec::new(),
            group_note: None,
            prefetching: HashSet::new(),
            search_open: false,
            filter_kind: String::new(),
            structure_edit: None,
            object_preview: None,
            last_object_scope: None,
            sql_tab_menu: None,
            tab_overflow_position: None,
            workbench: None,
            parked_workbenches: Vec::new(),
            table_tab_order: Vec::new(),
            collapsed_object_groups: HashSet::new(),
            wb_column_drag: None,
            workbench_scroll_drag: None,
            wb_column: cx.new(SqlEditor::new),
            wb_value: cx.new(SqlEditor::new),
            wb_editor: cx.new(SqlEditor::new),
            wb_import: {
                let editor = cx.new(SqlEditor::new);
                editor.update(cx, |editor, cx| {
                    editor.set_placeholder("导入文件路径（CSV）", cx)
                });
                editor
            },
            workspace: Workspace::Sql,
            theme,
            resolved_dark,
            notice: None,
            error,
        };
        app.sync_selection(cx);
        app
    }

    /// Cycles 自动 -> 浅色 -> 深色 and persists the choice.
    pub(super) fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(self.mode.next(), window, cx);
    }

    pub(super) fn set_mode(
        &mut self,
        mode: ThemeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.resolved_dark = window.appearance() == gpui::WindowAppearance::Dark;
        let theme = mode.resolve(self.resolved_dark);
        self.mode = mode;
        self.theme = theme;
        self.query_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.terminal_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.note_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.chat_panel
            .update(cx, |page, cx| page.set_theme(theme, cx));
        for bench in self.workbench.iter().chain(self.parked_workbenches.iter()) {
            if let Some(editor) = &bench.designer {
                editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
            }
        }
        self.settings.set_theme_mode(mode.as_key());
        cx.notify();
    }

    pub(super) fn settings_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let modes = [
            (ThemeMode::System, "自动"),
            (ThemeMode::Light, "浅色"),
            (ThemeMode::Dark, "深色"),
        ];
        let limits = [1_000u32, 5_000, 20_000];

        let mut mode_row = div().flex().items_center().gap_2();
        for (mode, label) in modes {
            let active = self.mode == mode;
            mode_row =
                mode_row.child(
                    div()
                        .id(SharedString::from(format!("theme-mode-{}", mode.as_key())))
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if active { theme.accent } else { theme.canvas })
                        .border_1()
                        .border_color(if active { theme.accent } else { theme.border })
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.set_mode(mode, window, cx)
                        }))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if active { theme.surface } else { theme.text })
                                .child(label),
                        ),
                );
        }

        let current_limit = self.settings.row_limit();
        let mut limit_row = div().flex().items_center().gap_2();
        for limit in limits {
            let active = current_limit == limit;
            limit_row = limit_row.child(
                div()
                    .id(SharedString::from(format!("row-limit-{limit}")))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.settings.set_row_limit(limit);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(format!("{limit} 行")),
                    ),
            );
        }

        div()
            .id("settings-scroll")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .overflow_y_scroll()
            .p_4()
            .gap_4()
            .items_center()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_6()
                    .w(px(520.0))
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icons::icon("superlink/settings.svg", 16.0, theme.muted))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text)
                                    .child("设置"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("close-settings")
                                    .px_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.settings_open = false;
                                        cx.notify();
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("外观"),
                            )
                            .child(mode_row),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("结果行数上限"),
                            )
                            .child(limit_row),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("诊断与版本"),
                    )
                    .w(px(520.))
                    .max_w(gpui::relative(0.94))
                    .child(self.update_panel(cx))
                    .child(self.drivers_panel(cx))
                    .children(self.diagnostics.clone().into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(SharedString::from(label)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(SharedString::from(value)),
                            )
                    })),
            )
    }

    /// A connection was saved in the form: refresh, select it, open its query.
    pub(super) fn select_profile_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        self.workspace = Workspace::Sql;
        self.refresh_profiles(Some(id.to_string()), cx);
    }

    pub(super) fn sync_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(profile) = self.profiles.get(self.selected).cloned() {
            self.query_page
                .update(cx, |page, cx| page.select_profile(profile, cx));
        }
        self.sync_catalog(cx);
    }

    /// Rebuilds the completion snapshot from the metadata the object tree has
    /// already cached for the selected connection.
    pub(super) fn sync_catalog(&mut self, cx: &mut Context<Self>) {
        let index = self.selected;
        let mut tables: Vec<superlink_sql::CompletionTable> = Vec::new();
        for ((conn, database), entries) in self.db_tables.iter() {
            if *conn != index {
                continue;
            }
            let Some(entries) = entries else {
                continue;
            };
            for entry in entries {
                let columns = self
                    .table_structures
                    .get(&(index, database.clone(), entry.name.clone()))
                    .map(|structure| {
                        structure
                            .columns
                            .iter()
                            .map(|column| column.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                tables.push(superlink_sql::CompletionTable {
                    schema: database.clone(),
                    name: entry.name.clone(),
                    columns,
                });
            }
        }
        self.query_page
            .update(cx, |page, cx| page.set_catalog(tables, cx));
    }

    /// Fetches every uncached table structure for one database in the
    /// background so column completion works without expanding each table.
    pub(super) fn prefetch_structures(
        &mut self,
        index: usize,
        database: String,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        if !self.prefetching.insert((index, database.clone())) {
            return;
        }
        let entries = self
            .db_tables
            .get(&(index, database.clone()))
            .and_then(|entries| entries.clone())
            .unwrap_or_default();
        let cached: HashSet<String> = self
            .table_structures
            .keys()
            .filter(|(conn, db, _)| *conn == index && db == &database)
            .map(|(_, _, table)| table.clone())
            .collect();
        let pending = missing_structures(&entries, &cached, PREFETCH_LIMIT);
        if pending.is_empty() {
            self.prefetching.remove(&(index, database));
            return;
        }
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let profile_id = profile.id.clone();
        let profile_revision = profile.revision;
        let credentials_task = {
            let profile = profile.clone();
            cx.background_executor()
                .spawn(async move { read_credentials(&profiles, &profile) })
        };
        cx.spawn(async move |this, cx| {
            let credentials = match credentials_task.await {
                Ok(credentials) => credentials,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.prefetching.remove(&(index, database.clone()));
                        this.conn_db_note.insert(index, error.to_string().into());
                        cx.notify();
                    });
                    return;
                }
            };
            let mut done = 0usize;
            for table in pending {
                let queries = queries.clone();
                let profile = profile.clone();
                let credentials = credentials.clone();
                let db = database.clone();
                let table_for_task = table.clone();
                let task = cx.background_executor().spawn(async move {
                    queries.structure(&profile, &credentials, &db, &table_for_task)
                });
                let outcome = task.await;
                done += 1;
                let refresh = done.is_multiple_of(PREFETCH_REFRESH_EVERY);
                let updated = this.update(cx, |this, cx| {
                    let still_selected = this.selected == index
                        && this
                            .profiles
                            .get(index)
                            .map(|profile| {
                                profile.id == profile_id && profile.revision == profile_revision
                            })
                            .unwrap_or(false);
                    if !still_selected {
                        return false;
                    }
                    if let Ok(structure) = outcome {
                        this.table_structures
                            .insert((index, database.clone(), table.clone()), structure);
                    }
                    if refresh {
                        this.sync_catalog(cx);
                    }
                    true
                });
                if !matches!(updated, Ok(true)) {
                    break;
                }
            }
            this.update(cx, |this, cx| {
                this.prefetching.remove(&(index, database.clone()));
                this.sync_catalog(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 连接节点的库列表；SQLite 返回空即没有库层级，直接挂表。
    pub(super) fn toggle_conn(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.expanded_conns.remove(&index) {
            cx.notify();
            return;
        }
        self.expanded_conns.insert(index);
        if !self.conn_dbs.contains_key(&index) {
            self.fetch_databases(index, cx);
        }
        cx.notify();
    }

    pub(super) fn fetch_databases(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let identity = (profile.id.clone(), profile.revision);
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = read_credentials(&profiles, &profile)?;
            queries.databases(&profile, &credentials)
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if this.profiles.get(index).map(|p| (&p.id, p.revision))
                    != Some((&identity.0, identity.1))
                {
                    return;
                }
                match outcome {
                    Ok(databases) => {
                        let flat = databases.is_empty();
                        this.conn_dbs.insert(index, Some(databases));
                        this.conn_db_note.remove(&index);
                        if flat {
                            // SQLite 一类没有库层级的数据源：连接下直接取表。
                            this.fetch_tables(index, String::new(), cx);
                        }
                    }
                    Err(err) => {
                        this.conn_dbs.insert(index, Some(Vec::new()));
                        this.conn_db_note
                            .insert(index, SharedString::from(err.to_string()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn toggle_db(&mut self, index: usize, database: String, cx: &mut Context<Self>) {
        let key = (index, database.clone());
        if self.expanded_dbs.remove(&key) {
            cx.notify();
            return;
        }
        self.expanded_dbs.insert(key);
        let cache_key = (index, database.clone());
        if !self.db_tables.contains_key(&cache_key) {
            self.fetch_tables(index, database, cx);
        }
        cx.notify();
    }

    pub(super) fn fetch_tables(&mut self, index: usize, database: String, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let identity = (profile.id.clone(), profile.revision);
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let db_for_task = database.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = read_credentials(&profiles, &profile)?;
            queries.tables(&profile, &credentials, &db_for_task)
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if this.profiles.get(index).map(|p| (&p.id, p.revision))
                    != Some((&identity.0, identity.1))
                {
                    return;
                }
                let key = (index, database.clone());
                match outcome {
                    Ok(entries) => {
                        this.db_tables.insert(key, Some(entries));
                        this.fetch_table_counts(index, database.clone(), cx);
                        this.sync_catalog(cx);
                        this.prefetch_structures(index, database.clone(), cx);
                    }
                    Err(err) => {
                        this.db_tables.insert(key, Some(Vec::new()));
                        this.column_errors.insert(
                            (index, database, String::new()),
                            SharedString::from(err.to_string()),
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
