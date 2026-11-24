use super::*;
use superlink_app::{QueryCancel, execution_policy};
use superlink_domain::Execution;

impl QueryPage {
    pub(super) fn store_output(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.output = self.result.take();
            tab.other_results = std::mem::take(&mut self.other_results);
            tab.selected_result = self.selected_result;
            tab.error = self.error.take();
            tab.last_sql = self.last_sql.take();
            tab.last_request = self.last_request.take();
            tab.last_duration = self.last_duration;
            tab.parameters = self.parameters.read(cx).text().to_string();
            tab.scope = self.scope.clone();
            tab.schema = self.schema.clone();
        }
    }

    pub(super) fn restore_output(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            self.result = tab.output.take();
            self.other_results = std::mem::take(&mut tab.other_results);
            self.selected_result = tab.selected_result;
            self.error = tab.error.take();
            self.last_sql = tab.last_sql.take();
            self.last_request = tab.last_request.take();
            self.last_duration = tab.last_duration;
            self.parameters
                .update(cx, |editor, cx| editor.set_text(tab.parameters.clone(), cx));
            self.scope = tab.scope.clone();
            self.schema = tab.schema.clone();
        }
        self.display_page = 0;
        self.column_page = 0;
        self.column_widths.clear();
        self.inspected = None;
    }

    pub(super) fn trim_results(&mut self) {
        fn bytes(result: &ResultSet) -> usize {
            fn value(v: &Value) -> usize {
                match v {
                    Value::String(s) => s.len() + 32,
                    Value::Array(a) => a.iter().map(value).sum::<usize>() + a.len() * 32,
                    Value::Object(o) => o.iter().map(|(k, v)| k.len() + value(v) + 64).sum(),
                    _ => 32,
                }
            }
            result
                .rows
                .iter()
                .map(|r| r.iter().map(value).sum::<usize>())
                .sum::<usize>()
                + result
                    .columns
                    .iter()
                    .map(|c| c.name.len() + 64)
                    .sum::<usize>()
        }
        let mut remaining = (64usize << 20).saturating_sub(
            self.result.as_ref().map(bytes).unwrap_or(0)
                + self.other_results.iter().map(bytes).sum::<usize>(),
        );
        for tab in self.tabs.iter_mut().chain(
            self.tab_cache
                .values_mut()
                .flat_map(|(tabs, _)| tabs.iter_mut()),
        ) {
            let needed = tab.output.as_ref().map(bytes).unwrap_or(0)
                + tab.other_results.iter().map(bytes).sum::<usize>();
            if needed > remaining {
                tab.output = None;
                tab.other_results.clear();
                tab.selected_result = 0;
            } else {
                remaining -= needed;
            }
        }
    }

    pub(super) fn stop_query(&mut self, cx: &mut Context<Self>) {
        self.request_generation = self.request_generation.wrapping_add(1);
        if let Some(cancel) = self.operation.take() {
            cx.background_executor()
                .spawn(async move {
                    cancel.cancel();
                })
                .detach();
            self.status = "正在停止；写入结果请刷新确认".into();
        }
        self.running = false;
        self.editor
            .update(cx, |editor, cx| editor.set_enabled(true, cx));
        cx.notify();
    }

    pub(super) fn run(&mut self, cx: &mut Context<Self>) {
        self.run_impl(false, cx);
    }

    pub(super) fn run_impl(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(profile) = self.profile.clone() else {
            self.error = Some("请先选择连接".into());
            cx.notify();
            return;
        };
        self.save_active(cx);
        let sql = self.editor.read(cx).execution_text();
        if sql.trim().is_empty() {
            self.error = Some("请输入查询或命令".into());
            cx.notify();
            return;
        }
        if let Err(error) = execution_policy::authorize(&profile, &sql, true, "") {
            self.error = Some(error.into());
            cx.notify();
            return;
        }
        let raw = self.parameters.read(cx).text();
        let value: Value = if raw.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str(raw) {
                Ok(value) => value,
                Err(error) => {
                    self.error = Some(format!("参数 JSON 无效：{error}").into());
                    self.result_tab = ResultTab::Params;
                    cx.notify();
                    return;
                }
            }
        };
        if !value.is_null() && !value.is_object() && !value.is_array() {
            self.error = Some("参数必须是 JSON 对象或数组".into());
            self.result_tab = ResultTab::Params;
            cx.notify();
            return;
        }
        if !force
            && (execution_policy::requires_write(&profile, &sql).unwrap_or(true)
                || needs_execution_confirmation(
                    self.redacted_notice.is_some(),
                    placeholder_count(&sql),
                ))
        {
            self.confirm_run = true;
            self.last_sql = Some(sql);
            cx.notify();
            return;
        }
        self.confirm_run = false;
        let request = Execution {
            revision: profile.revision,
            text: sql.clone(),
            write: force,
            parameters: value.as_object().cloned().unwrap_or_default(),
            args: value.as_array().cloned().unwrap_or_default(),
            scope: self.scope.clone(),
            schema: self.schema.clone(),
            max_rows: self.settings.row_limit().max(100),
            ..Default::default()
        };
        self.last_request = Some(request.clone());
        let cancel = QueryCancel::new();
        self.operation = Some(cancel.clone());
        self.request_generation = self.request_generation.wrapping_add(1);
        let generation = self.request_generation;
        let tab_id = self.tabs.get(self.active).map(|t| t.id.clone());
        let profiles = self.profiles.clone();
        let queries = self.queries.clone();
        let history = self.history.clone();
        self.running = true;
        self.error = None;
        self.status = "执行中…".into();
        self.editor
            .update(cx, |editor, cx| editor.set_enabled(false, cx));
        let task = cx.background_executor().spawn(async move {
            let started = std::time::Instant::now();
            let outcome = (|| {
                let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
                if current.revision != profile.revision {
                    return Err("连接配置已修改，请重新执行".into());
                }
                let credentials = profiles
                    .credentials(&current)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                queries
                    .execute_request(&current, &credentials, &request, &cancel)
                    .map_err(|e| e.to_string())
            })();
            let duration = started.elapsed().as_nanos() as u64;
            let rows = outcome
                .as_ref()
                .map(|r| {
                    r.iter()
                        .map(|r| r.rows.len() as i64 + r.rows_affected)
                        .sum()
                })
                .unwrap_or(0);
            let entry = history.record(&profile.id, &sql, outcome.is_ok(), rows, duration);
            (outcome, duration, entry, sql)
        });
        cx.spawn(async move |this, cx| {
            let (outcome, duration, entry, sql) = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.request_generation != generation
                    || this.tabs.get(this.active).map(|t| &t.id) != tab_id.as_ref()
                {
                    return;
                }
                this.operation = None;
                this.running = false;
                this.last_duration = Some(duration);
                this.last_sql = Some(sql.clone());
                this.display_page = 0;
                this.column_page = 0;
                this.column_widths.clear();
                this.inspected = None;
                if let Some(id) = entry {
                    if this.recent_sql.len() >= 50
                        && let Some(oldest) = this.recent_sql.keys().min().copied()
                    {
                        this.recent_sql.remove(&oldest);
                    }
                    this.recent_sql.insert(id, sql);
                }
                match outcome {
                    Ok(mut results) => {
                        let sets = results.len();
                        for result in &mut results {
                            result.duration_ns = duration;
                        }
                        this.selected_result = results
                            .iter()
                            .rposition(|r| !r.columns.is_empty())
                            .unwrap_or(0);
                        this.result = if results.is_empty() {
                            None
                        } else {
                            Some(results.remove(this.selected_result))
                        };
                        this.other_results = results;
                        this.trim_results();
                        this.redacted_notice = None;
                        this.result_tab = ResultTab::Result;
                        this.status = format!(
                            "完成 {sets} 条语句 · {} 行 · {:.1} ms",
                            this.result.as_ref().map(|r| r.rows.len()).unwrap_or(0),
                            duration as f64 / 1_000_000.0
                        )
                        .into();
                    }
                    Err(error) => {
                        this.error = Some(error.into());
                        this.status = "执行未完成".into();
                    }
                }
                this.editor
                    .update(cx, |editor, cx| editor.set_enabled(true, cx));
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn cancel_execution(&mut self, cx: &mut Context<Self>) {
        self.confirm_run = false;
        cx.notify();
    }
    pub(super) fn confirm_execution(&mut self, cx: &mut Context<Self>) {
        self.run_impl(true, cx);
    }
}

impl Drop for QueryPage {
    fn drop(&mut self) {
        for cancel in [self.operation.take(), self.export_operation.take()]
            .into_iter()
            .flatten()
        {
            std::thread::spawn(move || cancel.cancel());
        }
    }
}
