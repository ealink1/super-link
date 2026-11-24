use super::*;
use superlink_app::table_design::design_object;

impl SchemaDesigner {
    fn start(&mut self, cx: &mut Context<Self>) -> (u64, QueryCancel) {
        self.generation = self.generation.wrapping_add(1);
        if let Some(old) = self.operation.take() {
            cx.background_executor()
                .spawn(async move {
                    old.cancel();
                })
                .detach();
        }
        let cancel = QueryCancel::new();
        self.operation = Some(cancel.clone());
        self.busy = true;
        self.enable_inputs(cx);
        cx.emit(DesignEvent::Changed);
        cx.notify();
        (self.generation, cancel)
    }
    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.stale {
            return;
        }
        if self.dirty && !self.discard_confirm {
            self.discard_confirm = true;
            self.note = Some("有未保存草稿；点击「放弃草稿并刷新」才会重新读取".into());
            cx.notify();
            return;
        }
        let (generation, cancel) = self.start(cx);
        let profile = self.profile.clone();
        let profiles = self.profiles.clone();
        let queries = self.queries.clone();
        let db = self.database.clone();
        let table = self.table.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = credentials(&profiles, &profile)?;
            queries
                .design_snapshot(&profile, &credentials, &db, &table, &cancel)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                this.operation = None;
                match result {
                    Ok(snapshot) => {
                        this.install(snapshot, cx);
                        this.note = Some("结构已读取".into());
                    }
                    Err(error) => this.note = Some(error.into()),
                }
                this.enable_inputs(cx);
                cx.emit(DesignEvent::Changed);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn discard(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(snapshot) = self.baseline.clone() {
            self.install(snapshot, cx);
            self.enable_inputs(cx);
            self.note = Some("草稿已放弃".into());
            cx.emit(DesignEvent::Changed);
            cx.notify();
        }
    }
    fn install(&mut self, snapshot: DesignSnapshot, cx: &mut Context<Self>) {
        self.rows = DesignDraft::from(&snapshot)
            .fields
            .into_iter()
            .map(|field| Self::row(field, cx))
            .collect();
        self.table_comment
            .update(cx, |e, cx| e.set_text(snapshot.comment.clone(), cx));
        self.baseline = Some(snapshot);
        self.selected.clear();
        self.field_page = 0;
        self.plan = None;
        self.dirty = false;
        self.discard_confirm = false;
        self.type_picker = None;
    }
    pub(super) fn prepare(&mut self, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let draft = self.draft(cx);
        let before = self.baseline.clone().unwrap();
        match superlink_sql::design::design_sql(
            self.profile.sql_dialect(),
            &design_object(self.profile.sql_dialect(), &self.database, &self.table),
            &before,
            &draft,
        ) {
            Ok(statements) if statements.is_empty() => {
                self.dirty = false;
                self.note = Some("没有结构变更".into());
                cx.emit(DesignEvent::Changed);
            }
            Ok(statements) => {
                self.plan = Some(Plan {
                    target: self.table.clone(),
                    before: Some(before),
                    draft,
                    statements,
                    copy: false,
                });
                self.note = Some("请核对下面的 SQL；确认执行后才会保存到数据库".into());
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
    }
    pub(super) fn prepare_copy(&mut self, cx: &mut Context<Self>) {
        let fields = self.selected_fields(cx);
        let target = self.target.read(cx).text().trim().to_string();
        if target.is_empty() || fields.is_empty() {
            self.note = Some("请选择字段并填写目标表名".into());
            cx.notify();
            return;
        }
        if target == self.table {
            self.note = Some("目标表必须与来源表不同".into());
            cx.notify();
            return;
        }
        if self.copy_new {
            let object = design_object(self.profile.sql_dialect(), &self.database, &target);
            match superlink_sql::build_create_table(self.profile.sql_dialect(), &object, &fields) {
                Ok(sql) => {
                    self.plan = Some(Plan {
                        target,
                        before: None,
                        draft: DesignDraft::default(),
                        statements: vec![sql],
                        copy: true,
                    });
                    self.note = Some("请核对新表字段 SQL，再确认执行".into());
                }
                Err(error) => self.note = Some(error.to_string().into()),
            }
            cx.notify();
            return;
        }
        let (generation, cancel) = self.start(cx);
        let profile = self.profile.clone();
        let profiles = self.profiles.clone();
        let queries = self.queries.clone();
        let db = self.database.clone();
        let destination = target.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = credentials(&profiles, &profile)?;
            let before = queries
                .design_snapshot(&profile, &credentials, &db, &destination, &cancel)
                .map_err(|e| e.to_string())?;
            let mut draft = DesignDraft::from(&before);
            for mut column in fields {
                column.key.clear();
                column.extra = column
                    .extra
                    .split_whitespace()
                    .filter(|s| !s.eq_ignore_ascii_case("auto_increment"))
                    .collect::<Vec<_>>()
                    .join(" ");
                draft.fields.push(DesignField {
                    original: None,
                    column,
                });
            }
            let statements = superlink_sql::design::design_sql(
                profile.sql_dialect(),
                &design_object(profile.sql_dialect(), &db, &destination),
                &before,
                &draft,
            )
            .map_err(|e| e.to_string())?;
            Ok::<_, String>(Plan {
                target: destination,
                before: Some(before),
                draft,
                statements,
                copy: true,
            })
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                this.operation = None;
                match result {
                    Ok(plan) => {
                        this.plan = Some(plan);
                        this.note = Some("请核对目标表追加字段的 SQL，再确认执行".into());
                    }
                    Err(error) => this.note = Some(error.into()),
                }
                this.enable_inputs(cx);
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn apply(&mut self, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let Some(plan) = self.plan.take() else {
            return;
        };
        let (generation, cancel) = self.start(cx);
        let profile = self.profile.clone();
        let profiles = self.profiles.clone();
        let queries = self.queries.clone();
        let db = self.database.clone();
        let target = plan.target.clone();
        let copied = plan.copy;
        let task = cx.background_executor().spawn(async move {
            let credentials = credentials(&profiles, &profile)?;
            if let Some(before) = plan.before {
                queries
                    .save_design(
                        &profile,
                        &credentials,
                        &db,
                        superlink_app::table_design::DesignEdit {
                            table: &plan.target,
                            before: &before,
                            draft: &plan.draft,
                        },
                        &cancel,
                    )
                    .map(Some)
                    .map_err(|e| e.to_string())
            } else {
                queries
                    .execute_design_sql(&profile, &credentials, &db, &plan.statements, &cancel)
                    .map(|_| None)
                    .map_err(|e| e.to_string())
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                this.operation = None;
                match result {
                    Ok(snapshot) => {
                        if !copied {
                            if let Some(snapshot) = snapshot {
                                this.install(snapshot, cx);
                            }
                        } else {
                            this.copy_open = false;
                        }
                        this.note = Some(format!("已保存 {target}；结构已刷新").into());
                        cx.emit(DesignEvent::Saved(target));
                    }
                    Err(error) => {
                        this.note = Some(
                            format!("保存失败：{error}；草稿已保留，请刷新确认数据库状态").into(),
                        )
                    }
                }
                this.enable_inputs(cx);
                cx.emit(DesignEvent::Changed);
                cx.notify();
            });
        })
        .detach();
    }
}
fn credentials(
    profiles: &ProfileService,
    profile: &Profile,
) -> Result<superlink_app::Credentials, String> {
    let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
    if current.revision != profile.revision {
        return Err("连接配置已变化，请重新打开设计器".into());
    }
    profiles
        .credentials(&current)
        .map(|v| v.unwrap_or_default())
        .map_err(|e| e.to_string())
}
