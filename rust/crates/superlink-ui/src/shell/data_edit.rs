use super::*;
use serde_json::{Map, Value};
use superlink_sql::{RowChange, TableChanges};

impl SuperLinkApp {
    pub(super) fn edit_button(
        &self,
        id: &'static str,
        label: &'static str,
        handler: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(12.0))
            .text_color(theme.text)
            .hover(move |s| s.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _, _, cx| handler(this, cx)))
            .child(label)
            .into_any_element()
    }
    pub(super) fn data_edit_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(bench) = &self.workbench else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.edit_button(
                        "data-add",
                        "新增行",
                        |this, cx| this.add_data_row(cx),
                        cx,
                    ))
                    .child(self.edit_button(
                        "data-delete",
                        "删除选中行",
                        |this, cx| this.delete_data_rows(cx),
                        cx,
                    ))
                    .child(self.edit_button(
                        "data-preview",
                        "SQL 预览",
                        |this, cx| this.preview_data(cx),
                        cx,
                    ))
                    .child(self.edit_button(
                        "data-commit",
                        "确认提交",
                        |this, cx| this.commit_data(cx),
                        cx,
                    ))
                    .child(self.edit_button(
                        "data-discard",
                        "撤销变更",
                        |this, cx| {
                            if let Some(b) = this.workbench.as_mut() {
                                b.edits.clear();
                                b.preview = None;
                            }
                            this.fetch_workbench_data(cx);
                        },
                        cx,
                    ))
                    .child(format!(
                        "{} 行待提交 · {} 行已选",
                        bench.edits.len(),
                        bench.selected.len()
                    )),
            )
            .children(bench.cell.map(|(row, column)| {
                let name = bench
                    .result
                    .as_ref()
                    .and_then(|r| r.columns.get(column))
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.0))
                    .child(format!("第 {} 行 · {name}", row + 1))
                    .child(div().flex_1().child(self.wb_editor.clone()))
                    .child(self.edit_button(
                        "data-cell-save",
                        "暂存",
                        |this, cx| this.stage_data_cell(false, cx),
                        cx,
                    ))
                    .child(self.edit_button(
                        "data-cell-null",
                        "设为 NULL",
                        |this, cx| this.stage_data_cell(true, cx),
                        cx,
                    ))
            }))
            .children(bench.preview.as_ref().map(|preview| {
                div()
                    .id("data-sql-preview")
                    .max_h(px(160.0))
                    .overflow_y_scroll()
                    .p_2()
                    .bg(self.theme.canvas)
                    .text_size(px(12.0))
                    .child(SharedString::from(preview.clone()))
            }))
            .into_any_element()
    }
    fn data_allowed(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(bench) = self.workbench.as_ref() else {
            return false;
        };
        let error = self
            .profiles
            .get(bench.conn)
            .ok_or_else(|| "连接已不存在".into())
            .and_then(|p| {
                superlink_app::execution_policy::authorize(p, "UPDATE table_data", true, "data")
            });
        if let Err(error) = error {
            if let Some(b) = self.workbench.as_mut() {
                b.error = Some(error.into());
            }
            cx.notify();
            return false;
        }
        true
    }
    pub(super) fn open_data_cell(&mut self, row: usize, column: usize, cx: &mut Context<Self>) {
        if !self.data_allowed(cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut() {
            if bench.loading || bench.result.as_ref().is_some_and(|r| r.truncated) {
                return;
            }
            let Some(value) = bench
                .result
                .as_ref()
                .and_then(|r| r.rows.get(row))
                .and_then(|r| r.get(column))
            else {
                return;
            };
            let text = value.as_str().map(str::to_owned).unwrap_or_else(|| {
                if value.is_null() {
                    String::new()
                } else {
                    value.to_string()
                }
            });
            bench.cell = Some((row, column));
            self.wb_editor.update(cx, |e, cx| e.set_text(text, cx));
            cx.notify();
        }
    }
    fn stage_data_cell(&mut self, null: bool, cx: &mut Context<Self>) {
        if !self.data_allowed(cx) {
            return;
        }
        let text = self.wb_editor.read(cx).text().to_string();
        let Some(bench) = self.workbench.as_mut() else {
            return;
        };
        let Some((row, column)) = bench.cell else {
            return;
        };
        let Some(result) = bench.result.as_mut() else {
            return;
        };
        let Some(field) = result.columns.get(column) else {
            return;
        };
        let name = field.name.clone();
        let metadata = bench.columns.iter().find(|c| c.name == name);
        if null && metadata.is_some_and(|m| !m.nullable) {
            bench.error = Some("此字段不允许 NULL".into());
            cx.notify();
            return;
        }
        let value = if null {
            Value::Null
        } else {
            Value::String(text)
        };
        let value = match superlink_sql::convert_value(
            metadata.map(|m| m.data_type.as_str()).unwrap_or("TEXT"),
            &value,
        ) {
            Ok(v) => v.to_json(),
            Err(e) => {
                bench.error = Some(e.to_string().into());
                cx.notify();
                return;
            }
        };
        let Some(values) = result.rows.get_mut(row) else {
            return;
        };
        let original: Map<String, Value> = result
            .columns
            .iter()
            .zip(values.iter())
            .map(|(c, v)| (c.name.clone(), v.clone()))
            .collect();
        let change = bench.edits.entry(row).or_insert_with(|| RowChange {
            kind: "update".into(),
            original,
            values: Map::new(),
        });
        if change.kind == "delete" {
            bench.error = Some("此行已标记删除，请先撤销变更".into());
            cx.notify();
            return;
        }
        if change.kind == "update" && change.original.get(&name) == Some(&value) {
            change.values.remove(&name);
        } else {
            change.values.insert(name, value.clone());
        }
        values[column] = value;
        if change.kind == "update" && change.values.is_empty() {
            bench.edits.remove(&row);
        }
        bench.preview = None;
        bench.error = None;
        cx.notify();
    }
    pub(super) fn add_data_row(&mut self, cx: &mut Context<Self>) {
        if !self.data_allowed(cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut()
            && let Some(result) = bench.result.as_mut()
        {
            if bench.edits.len() >= 1000 || bench.loading {
                return;
            }
            let row = result.rows.len();
            result.rows.push(vec![Value::Null; result.columns.len()]);
            bench.edits.insert(
                row,
                RowChange {
                    kind: "insert".into(),
                    ..Default::default()
                },
            );
            bench.preview = None;
            bench.selected.clear();
            bench.selected.insert(row);
            cx.notify();
        }
    }
    pub(super) fn delete_data_rows(&mut self, cx: &mut Context<Self>) {
        if !self.data_allowed(cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut()
            && let Some(result) = bench.result.as_mut()
        {
            let removed = bench
                .selected
                .iter()
                .copied()
                .filter(|row| bench.edits.get(row).is_some_and(|c| c.kind == "insert"))
                .collect::<Vec<_>>();
            for &row in &bench.selected {
                if let Some(change) = bench.edits.get_mut(&row) {
                    if change.kind == "insert" {
                        bench.edits.remove(&row);
                        continue;
                    }
                    change.kind = "delete".into();
                    change.values.clear();
                } else if let Some(values) = result.rows.get(row) {
                    let original = result
                        .columns
                        .iter()
                        .zip(values)
                        .map(|(c, v)| (c.name.clone(), v.clone()))
                        .collect();
                    bench.edits.insert(
                        row,
                        RowChange {
                            kind: "delete".into(),
                            original,
                            ..Default::default()
                        },
                    );
                }
            }
            for row in removed.into_iter().rev() {
                if row < result.rows.len() {
                    result.rows.remove(row);
                }
                let edits = std::mem::take(&mut bench.edits);
                bench.edits = edits
                    .into_iter()
                    .map(|(index, change)| (if index > row { index - 1 } else { index }, change))
                    .collect();
            }
            bench.selected.clear();
            bench.preview = None;
            cx.notify();
        }
    }
    fn data_request(&self) -> Option<(Profile, TableChanges, superlink_sql::TableInfo)> {
        let bench = self.workbench.as_ref()?;
        if bench.stale {
            return None;
        }
        let profile = self.profiles.get(bench.conn)?.clone();
        Some((
            profile.clone(),
            TableChanges {
                revision: profile.revision,
                object: bench.object(),
                rows: bench.edits.values().cloned().collect(),
                confirmation: "apply".into(),
                ..Default::default()
            },
            bench.info(),
        ))
    }
    fn preview_data(&mut self, cx: &mut Context<Self>) {
        let Some((profile, changes, info)) = self.data_request() else {
            return;
        };
        let result = self.queries.preview_data_changes(&profile, &changes, &info);
        if let Some(b) = self.workbench.as_mut() {
            match result {
                Ok(statements) => {
                    b.preview = Some(superlink_sql::preview_changes(&statements));
                    b.error = None;
                }
                Err(e) => b.error = Some(e.to_string().into()),
            }
        }
        cx.notify();
    }
    fn commit_data(&mut self, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_mut() else {
            return;
        };
        if bench.loading || bench.stale {
            return;
        }
        if bench.preview.is_none() {
            bench.error = Some("请先生成 SQL 预览".into());
            cx.notify();
            return;
        }
        let id = bench.id.clone();
        bench.loading = true;
        let cancel = superlink_app::QueryCancel::new();
        bench.operation = Some(cancel.clone());
        let Some((profile, changes, info)) = self.data_request() else {
            return;
        };
        let profiles = self.service.clone();
        let queries = self.queries.clone();
        let task = cx.background_executor().spawn(async move {
            let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
            if current.revision != profile.revision {
                return Err("连接配置已变化，请重新预览".into());
            }
            let credentials = profiles
                .credentials(&current)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            queries
                .apply_data_changes(&current, &credentials, &changes, &info, &cancel)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                let Some(bench) = this.workbench.as_mut() else {
                    return;
                };
                if bench.id != id {
                    return;
                }
                bench.loading = false;
                bench.operation = None;
                match outcome {
                    Ok(_) => {
                        bench.edits.clear();
                        bench.preview = None;
                        this.fetch_workbench_data(cx);
                    }
                    Err(e) => bench.error = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl SuperLinkApp {
    pub(super) fn fetch_workbench_ddl(&mut self, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_mut() else {
            return;
        };
        if bench.ddl_loading {
            return;
        }
        bench.ddl_loading = true;
        let id = bench.id.clone();
        let database = bench.database.clone();
        let table = bench.table.clone();
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        let identity = (profile.id.clone(), profile.revision);
        let profiles = self.service.clone();
        let queries = self.queries.clone();
        let task = cx.background_executor().spawn(async move {
            let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
            if current.revision != profile.revision {
                return Err("连接配置已修改，请重新打开表".into());
            }
            let credentials = profiles
                .credentials(&current)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            queries
                .table_ddl(&current, &credentials, &database, &table)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(bench) = this.workbench.as_mut()
                    && bench.id == id
                    && this.profiles.get(bench.conn).map(|p| (&p.id, p.revision))
                        == Some((&identity.0, identity.1))
                {
                    bench.ddl_loading = false;
                    match outcome {
                        Ok(ddl) => bench.ddl_source = Some(ddl),
                        Err(e) => bench.error = Some(e.into()),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
