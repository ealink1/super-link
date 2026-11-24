use super::*;
#[derive(Clone, Copy, PartialEq)]
pub(super) enum CreateKind {
    Database,
    Table,
}
pub(super) struct Designer {
    kind: CreateKind,
    profile: Profile,
    scope: Entity<SqlEditor>,
    name: Entity<SqlEditor>,
    field: Entity<SqlEditor>,
    data_type: Entity<SqlEditor>,
    columns: Vec<superlink_sql::ColumnDefinition>,
    nullable: bool,
    primary: bool,
    error: Option<SharedString>,
}
impl SuperLinkApp {
    pub(super) fn open_designer(&mut self, kind: CreateKind, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(self.selected).cloned() else {
            return;
        };
        let editor = |text: &str, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut e = SqlEditor::new(cx);
                e.set_text(text.to_string(), cx);
                e
            })
        };
        self.designer = Some(Designer {
            kind,
            scope: editor(&profile.config.database, cx),
            profile,
            name: editor("", cx),
            field: editor("", cx),
            data_type: editor("TEXT", cx),
            columns: Vec::new(),
            nullable: true,
            primary: false,
            error: None,
        });
        self.connection_actions = None;
        cx.notify();
    }
    fn add_designer_column(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.designer.as_mut() else {
            return;
        };
        let name = form.field.read(cx).text().trim().to_string();
        let data_type = form.data_type.read(cx).text().trim().to_string();
        if name.is_empty()
            || data_type.is_empty()
            || form
                .columns
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            form.error = Some("请填写不重复的字段名及类型".into());
            cx.notify();
            return;
        }
        if form.columns.len() >= 1024 {
            return;
        }
        form.columns.push(superlink_sql::ColumnDefinition {
            name,
            data_type,
            nullable: if form.nullable { "YES" } else { "NO" }.into(),
            key: if form.primary { "PRI" } else { "" }.into(),
            ..Default::default()
        });
        form.field.update(cx, |e, cx| e.set_text(String::new(), cx));
        form.error = None;
        cx.notify();
    }
    fn preview_designer(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.designer.as_mut() else {
            return;
        };
        let name = form.name.read(cx).text().trim();
        let scope = form.scope.read(cx).text().trim().to_string();
        let result = match form.kind {
            CreateKind::Database => {
                superlink_sql::create_database_sql(form.profile.sql_dialect(), name)
            }
            CreateKind::Table => superlink_sql::build_create_table(
                form.profile.sql_dialect(),
                &superlink_sql::Object {
                    name: name.into(),
                    scope: scope.clone(),
                    kind: "table".into(),
                    ..Default::default()
                },
                &form.columns,
            ),
        };
        match result {
            Ok(sql) => {
                let profile = form.profile.clone();
                self.designer = None;
                self.workspace = Workspace::Sql;
                self.query_page.update(cx, |page, cx| {
                    page.load_table_query(profile, scope, sql, cx)
                });
            }
            Err(error) => form.error = Some(error.to_string().into()),
        }
        cx.notify();
    }
    pub(super) fn designer_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.designer.as_ref() else {
            return div().into_any_element();
        };
        let theme = self.theme;
        let mut panel = div()
            .id("object-designer")
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .flex_1()
            .overflow_y_scroll()
            .bg(theme.surface)
            .text_color(theme.text)
            .child(if form.kind == CreateKind::Database {
                "新建数据库"
            } else {
                "新建表"
            })
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child("名称")
                    .child(div().w(px(300.)).child(form.name.clone())),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child("数据库")
                    .child(div().w(px(300.)).child(form.scope.clone())),
            );
        if form.kind == CreateKind::Table {
            panel = panel.child(
                div()
                    .flex()
                    .gap_3()
                    .child("字段名")
                    .child(div().w(px(180.)).child(form.field.clone()))
                    .child("类型")
                    .child(div().w(px(160.)).child(form.data_type.clone()))
                    .child(
                        div()
                            .id("create-column-null")
                            .cursor_pointer()
                            .child(if form.nullable {
                                "☑ 可空"
                            } else {
                                "☐ 可空"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(f) = this.designer.as_mut() {
                                    f.nullable = !f.nullable;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("create-column-pk")
                            .cursor_pointer()
                            .child(if form.primary {
                                "☑ 主键"
                            } else {
                                "☐ 主键"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(f) = this.designer.as_mut() {
                                    f.primary = !f.primary;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("create-column-add")
                            .cursor_pointer()
                            .text_color(theme.accent)
                            .child("添加字段")
                            .on_click(cx.listener(|this, _, _, cx| this.add_designer_column(cx))),
                    ),
            );
            for (index, column) in form.columns.iter().enumerate() {
                panel = panel.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(format!(
                            "{} · {} · {} {}",
                            column.name,
                            column.data_type,
                            if column.nullable == "YES" {
                                "NULL"
                            } else {
                                "NOT NULL"
                            },
                            column.key
                        ))
                        .child(
                            div()
                                .id(("remove-create-column", index))
                                .cursor_pointer()
                                .child("移除")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(f) = this.designer.as_mut() {
                                        f.columns.remove(index);
                                    }
                                    cx.notify();
                                })),
                        ),
                );
            }
        }
        panel
            .children(form.error.clone())
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .id("preview-create-object")
                            .cursor_pointer()
                            .text_color(theme.accent)
                            .child("生成 SQL 并预览")
                            .on_click(cx.listener(|this, _, _, cx| this.preview_designer(cx))),
                    )
                    .child(
                        div()
                            .id("close-object-designer")
                            .cursor_pointer()
                            .child("取消")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.designer = None;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn connection_actions_panel(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut panel = div()
            .flex()
            .flex_wrap()
            .gap_3()
            .px_5()
            .py_2()
            .text_size(px(12.))
            .text_color(self.theme.accent);
        for (id, label) in [
            ("edit", "编辑连接"),
            ("copy", "复制连接"),
            ("database", "新建库"),
            ("table", "新建表"),
            ("delete", "删除连接"),
        ] {
            panel = panel.child(
                div()
                    .id(SharedString::from(format!("connection-{index}-{id}")))
                    .cursor_pointer()
                    .child(if id == "delete" && self.connection_delete_confirm {
                        "确认删除连接"
                    } else {
                        label
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected = index;
                        match id {
                            "edit" => {
                                this.connection_actions = None;
                                this.sync_selection(cx);
                                this.query_page
                                    .update(cx, |page, cx| page.open_edit_connection(window, cx));
                            }
                            "database" => this.open_designer(CreateKind::Database, cx),
                            "table" => this.open_designer(CreateKind::Table, cx),
                            "copy" => this.copy_connection(index, cx),
                            "delete" => this.delete_connection(index, cx),
                            _ => {}
                        }
                        cx.notify();
                    })),
            );
        }
        panel.into_any_element()
    }
    fn copy_connection(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(mut profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let service = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = service.credentials(&profile)?.unwrap_or_default();
            profile.id = uuid::Uuid::new_v4().to_string();
            profile.revision = 0;
            profile.secret_ref.clear();
            profile.name = format!("{} 副本", profile.name);
            service.save_with_remember(profile, &credentials, true)?;
            service.list()
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Ok(profiles) => {
                        this.replace_profiles(profiles, cx);
                        this.connection_actions = None;
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn delete_connection(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.workbench.as_ref().is_some_and(|b| !b.edits.is_empty())
            || self
                .structure_edit
                .as_ref()
                .is_some_and(|e| !e.pending.is_empty())
        {
            self.error = Some("请先提交或撤销当前变更，再删除连接".into());
            cx.notify();
            return;
        }
        if !self.connection_delete_confirm {
            self.connection_delete_confirm = true;
            cx.notify();
            return;
        }
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let service = self.service.clone();
        let id = profile.id.clone();
        let task = cx.background_executor().spawn(async move {
            service.delete(&profile.id, profile.revision)?;
            service.list()
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Ok(profiles) => {
                        this.query_page
                            .update(cx, |page, cx| page.forget_profile(&id, cx));
                        this.replace_profiles(profiles, cx);
                        if let Some(cancel) =
                            this.workbench.as_mut().and_then(|b| b.operation.take())
                        {
                            cx.background_executor()
                                .spawn(async move {
                                    cancel.cancel();
                                })
                                .detach();
                        }
                        this.workbench = None;
                        this.table_tab_order
                            .retain(|id| this.parked_workbenches.iter().any(|b| &b.id == id));
                        this.structure_edit = None;
                        this.selected = 0;
                        this.connection_actions = None;
                        this.connection_delete_confirm = false;
                        this.expanded_conns.clear();
                        this.expanded_dbs.clear();
                        this.expanded_tables.clear();
                        this.conn_dbs.clear();
                        this.db_tables.clear();
                        this.table_structures.clear();
                        this.column_errors.clear();
                        this.sync_selection(cx);
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn preview_table_operation(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_ref() else {
            return;
        };
        if !bench.edits.is_empty() {
            return;
        }
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        match superlink_sql::table_mutation_sql(profile.sql_dialect(), &bench.object(), action) {
            Ok(sql) => {
                let scope = bench.database.clone();
                if !self.park_workbench(cx) {
                    return;
                }
                self.query_page.update(cx, |page, cx| {
                    page.load_table_query(profile, scope, sql, cx)
                });
            }
            Err(error) => {
                if let Some(bench) = self.workbench.as_mut() {
                    bench.error = Some(error.to_string().into());
                }
            }
        }
        cx.notify();
    }
}
