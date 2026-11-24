use super::*;
impl SuperLinkApp {
    pub(super) fn import_workbench_csv(&mut self, cx: &mut Context<Self>) {
        let path = self
            .wb_import
            .read(cx)
            .text()
            .trim()
            .trim_matches('"')
            .to_string();
        if !path.is_empty() {
            self.preview_import_file(path.into(), cx);
            return;
        }
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 CSV / TSV / JSON / XLSX 导入文件".into()),
        });
        let id = self.workbench.as_ref().map(|b| b.id.clone());
        cx.spawn(async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            if let Some(path) = path {
                let _ = this.update(cx, |this, cx| {
                    if this.workbench.as_ref().map(|b| &b.id) == id.as_ref() {
                        this.preview_import_file(path, cx);
                    }
                });
            }
        })
        .detach();
    }
    fn preview_import_file(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_mut() else {
            return;
        };
        if bench.loading || !bench.edits.is_empty() {
            bench.error = Some("请先完成当前加载或数据编辑".into());
            cx.notify();
            return;
        }
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        let id = bench.id.clone();
        let object = bench.object();
        let columns = bench.columns.clone();
        let cancel = superlink_app::QueryCancel::new();
        bench.operation = Some(cancel.clone());
        bench.loading = true;
        bench.import_plan = None;
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
            if current.revision != profile.revision {
                return Err("连接配置已修改，请刷新后导入".into());
            }
            queries.preview_import(&current, &path, object, &columns, &cancel)
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(bench) = this.workbench.as_mut()
                    && bench.id == id
                {
                    bench.loading = false;
                    bench.operation = None;
                    match outcome {
                        Ok(plan) => {
                            bench.error = None;
                            bench.import_plan = Some(Arc::new(plan));
                        }
                        Err(e) => bench.error = Some(e.into()),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }
    fn apply_import_file(&mut self, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_mut() else {
            return;
        };
        if bench.loading {
            return;
        }
        let Some(plan) = bench.import_plan.clone() else {
            return;
        };
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        let id = bench.id.clone();
        let cancel = superlink_app::QueryCancel::new();
        bench.loading = true;
        bench.operation = Some(cancel.clone());
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
            let credentials = profiles
                .credentials(&current)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            queries
                .apply_import(&current, &credentials, &plan, true, &cancel)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(bench) = this.workbench.as_mut().filter(|b| b.id == id) else {
                    return;
                };
                bench.loading = false;
                bench.operation = None;
                match outcome {
                    Ok(count) => {
                        bench.import_plan = None;
                        bench.error = None;
                        bench.page = 1;
                        this.notice = Some(format!("已导入 {count} 行").into());
                        this.fetch_workbench_data(cx);
                    }
                    Err(e) => bench.error = Some(format!("导入未完成：{e}").into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn import_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div().flex().flex_col().gap_2().px_3();
        if let Some(plan) = self.workbench.as_ref().and_then(|b| b.import_plan.as_ref()) {
            panel = panel
                .child(format!(
                    "将导入 {} 行 · 列：{}",
                    plan.rows,
                    plan.columns.join(", ")
                ))
                .child(plan.statement.clone())
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(
                            div()
                                .id("confirm-file-import")
                                .cursor_pointer()
                                .text_color(self.theme.accent)
                                .child("确认事务导入")
                                .on_click(cx.listener(|this, _, _, cx| this.apply_import_file(cx))),
                        )
                        .child(
                            div()
                                .id("discard-file-import")
                                .cursor_pointer()
                                .child("取消预览")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(b) = this.workbench.as_mut() {
                                        b.import_plan = None;
                                    }
                                    cx.notify();
                                })),
                        ),
                );
        }
        if self
            .workbench
            .as_ref()
            .is_some_and(|b| b.operation.is_some())
        {
            panel = panel.child(
                div()
                    .id("stop-table-task")
                    .cursor_pointer()
                    .child("停止当前任务")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(cancel) =
                            this.workbench.as_ref().and_then(|b| b.operation.clone())
                        {
                            cx.background_executor()
                                .spawn(async move {
                                    cancel.cancel();
                                })
                                .detach();
                        }
                    })),
            );
        }
        panel.into_any_element()
    }
    pub(super) fn export_table_file(&mut self, all: bool, cx: &mut Context<Self>) {
        if self
            .workbench
            .as_ref()
            .is_some_and(|b| b.loading || !b.edits.is_empty())
        {
            return;
        }
        let Some(bench) = self.workbench.as_ref() else {
            return;
        };
        let Some(result) = bench.result.clone() else {
            return;
        };
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        let table = match superlink_sql::object_name(profile.sql_dialect(), &bench.object()) {
            Ok(s) => s,
            Err(_) => return,
        };
        let sql = match bench.export_statement(profile.sql_dialect()) {
            Ok(sql) => sql,
            Err(_) => return,
        };
        let scope = bench.database.clone();
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let picker = cx.prompt_for_new_path(&superlink_app::data_root(), Some("表数据.xlsx"));
        let id = bench.id.clone();
        cx.spawn(async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(path))) => path,
                _ => return,
            };
            let cancel = superlink_app::QueryCancel::new();
            let operation = cancel.clone();
            if !this
                .update(cx, |this, cx| {
                    if let Some(bench) = this.workbench.as_mut().filter(|b| b.id == id) {
                        bench.loading = true;
                        bench.operation = Some(operation);
                        cx.notify();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
            {
                return;
            }
            let task = cx.background_executor().spawn(async move {
                let options =
                    superlink_app::transfer::export_options(&path, profile.sql_dialect(), &table)?;
                if all {
                    let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
                    let credentials = profiles
                        .credentials(&current)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default();
                    let request = superlink_domain::Execution {
                        revision: profile.revision,
                        text: sql,
                        scope,
                        ..Default::default()
                    };
                    return queries.export_query(
                        &current,
                        &credentials,
                        &request,
                        &path,
                        options,
                        &cancel,
                    );
                }
                superlink_app::transfer::export_result(&result, &path, options, &cancel)
                    .map_err(|e| e.to_string())
            });
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(bench) = this.workbench.as_mut().filter(|b| b.id == id) {
                    bench.loading = false;
                    bench.operation = None;
                    match outcome {
                        Ok(n) => {
                            this.notice = Some(
                                format!("已导出{} {n} 行", if all { "全部结果" } else { "当前页" })
                                    .into(),
                            )
                        }
                        Err(e) => bench.error = Some(e.into()),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
