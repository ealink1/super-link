use super::*;

impl SuperLinkApp {
    /// 结构变更后刷新面板与表树缓存。
    pub(super) fn refresh_structure(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return;
        };
        let identity = (profile.id.clone(), profile.revision);
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let db_for_task = database.clone();
        let name = table.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = read_credentials(&profiles, &profile)?;
            queries.structure(&profile, &credentials, &db_for_task, &name)
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if this.profiles.get(conn).map(|p| (&p.id, p.revision))
                    != Some((&identity.0, identity.1))
                {
                    return;
                }
                if let Ok(structure) = outcome {
                    this.table_structures
                        .insert((conn, database.clone(), table.clone()), structure.clone());
                    if let Some(edit) = this.structure_edit.as_mut()
                        && edit.conn == conn
                        && edit.table == table
                    {
                        edit.columns = structure.columns;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 新增列进入变更集（不立即执行）。
    pub(super) fn queue_add_column(&mut self, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        let Some(edit) = self.structure_edit.as_ref() else {
            return;
        };
        let name = edit.name.read(cx).text().trim().to_string();
        let data_type = edit.data_type.read(cx).text().trim().to_string();
        let nullable = edit.nullable;
        if name.is_empty() || data_type.is_empty() {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("列名和类型都不能为空".into());
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "addColumn".into(),
                column: superlink_sql::ColumnDefinition {
                    name: name.clone(),
                    data_type: data_type.clone(),
                    nullable: if nullable { "YES" } else { "NO" }.to_string(),
                    ..Default::default()
                },
                ..Default::default()
            });
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
            edit.name
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            edit.data_type
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        }
        cx.notify();
    }

    /// 删除列进入变更集（两步确认）。
    pub(super) fn queue_drop_column(&mut self, column: String, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        let confirm_needed = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            edit.pending_delete.as_deref() != Some(column.as_str())
        };
        if confirm_needed {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.pending_delete = Some(column);
                edit.error = None;
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "dropColumn".into(),
                original_name: column.clone(),
                column: superlink_sql::ColumnDefinition {
                    name: column,
                    data_type: String::new(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
                ..Default::default()
            });
            edit.pending_delete = None;
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
        }
        cx.notify();
    }

    /// 改名进入变更集：第一次点击出现内联输入，第二次确认。
    pub(super) fn queue_rename_column(&mut self, column: String, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        let rename_requested = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            edit.rename_from.as_deref() == Some(column.as_str())
        };
        if !rename_requested {
            if let Some(edit) = self.structure_edit.as_mut() {
                let editor = cx.new(SqlEditor::new);
                editor.update(cx, |editor, cx| {
                    editor.set_text(column.clone(), cx);
                    editor.set_placeholder("新列名", cx)
                });
                edit.rename_from = Some(column);
                edit.rename_editor = Some(editor);
                edit.error = None;
            }
            cx.notify();
            return;
        }
        let new_name = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            let Some(editor) = edit.rename_editor.as_ref() else {
                return;
            };
            editor.read(cx).text().trim().to_string()
        };
        if new_name.is_empty() || new_name == column {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("请输入与原列名不同的新列名".into());
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            let original = edit
                .columns
                .iter()
                .find(|entry| entry.name == column)
                .cloned()
                .unwrap_or_default();
            edit.pending.push(superlink_sql::StructureChange {
                kind: "alterColumn".into(),
                original_name: column.clone(),
                column: superlink_sql::ColumnDefinition {
                    name: new_name,
                    data_type: original.data_type,
                    nullable: if original.nullable { "YES" } else { "NO" }.to_string(),
                    key: if original.primary_key { "PRI" } else { "" }.to_string(),
                    default: original.default_value.clone(),
                    has_default: original.default_value.is_some(),
                    comment: original.comment.clone().unwrap_or_default(),
                    ..Default::default()
                },
                ..Default::default()
            });
            edit.rename_from = None;
            edit.rename_editor = None;
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
        }
        cx.notify();
    }

    /// 添加索引进入变更集。
    pub(super) fn queue_add_index(&mut self, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        let Some(edit) = self.structure_edit.as_ref() else {
            return;
        };
        let name = edit.index_name.read(cx).text().trim().to_string();
        let columns_text = edit.index_columns.read(cx).text().trim().to_string();
        let unique = edit.index_unique;
        if name.is_empty() || columns_text.is_empty() {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("索引名和列都不能为空".into());
            }
            cx.notify();
            return;
        }
        let columns: Vec<superlink_sql::IndexColumn> = columns_text
            .split(',')
            .map(|column| superlink_sql::IndexColumn {
                name: column.trim().to_string(),
                descending: false,
            })
            .filter(|column| !column.name.is_empty())
            .collect();
        if columns.is_empty() {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("索引列不能为空".into());
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "addIndex".into(),
                index: superlink_sql::NewIndex {
                    name: name.clone(),
                    method: String::new(),
                    columns,
                    unique,
                },
                ..Default::default()
            });
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
            edit.index_name
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            edit.index_columns
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        }
        cx.notify();
    }

    /// 删除索引进入变更集（两步确认）。
    pub(super) fn queue_drop_index(&mut self, name: String, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        let confirm_needed = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            edit.pending_delete.as_deref() != Some(format!("index:{name}").as_str())
        };
        if confirm_needed {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.pending_delete = Some(format!("index:{name}"));
                edit.error = None;
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "dropIndex".into(),
                original_name: name.clone(),
                ..Default::default()
            });
            edit.pending_delete = None;
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
        }
        cx.notify();
    }

    pub(super) fn toggle_index_unique(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.index_unique = !edit.index_unique;
        }
        cx.notify();
    }

    pub(super) fn remove_pending_change(&mut self, position: usize, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut()
            && position < edit.pending.len()
        {
            edit.pending.remove(position);
            edit.preview = None;
            edit.confirmed = false;
        }
        cx.notify();
    }

    /// 受保护提交第一步：生成 SQL 预览。
    pub(super) fn build_structure_preview(&mut self, cx: &mut Context<Self>) {
        let payload = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            if edit.pending.is_empty() {
                if let Some(edit) = self.structure_edit.as_mut() {
                    edit.error = Some("没有结构变更".into());
                }
                cx.notify();
                return;
            }
            let kind = self
                .profiles
                .get(edit.conn)
                .map(|profile| profile.sql_dialect().to_string())
                .unwrap_or_default();
            let info = superlink_app::structure::table_info_from(&edit.columns, &edit.indexes);
            let (schema, name) = match edit.table.split_once('.') {
                Some((schema, name)) => (schema.to_string(), name.to_string()),
                None => (String::new(), edit.table.clone()),
            };
            let request = superlink_sql::StructureRequest {
                object: superlink_sql::Object {
                    name,
                    kind: "table".into(),
                    schema,
                    ..Default::default()
                },
                changes: edit.pending.clone(),
                ..Default::default()
            };
            superlink_sql::structure::build_structure_changes(&kind, &request, &info)
                .map(|statements| superlink_sql::structure::preview_structure(&statements))
        };
        if let Some(edit) = self.structure_edit.as_mut() {
            match payload {
                Ok(preview) => {
                    edit.preview = Some(preview);
                    edit.confirmed = false;
                    edit.error = None;
                }
                Err(err) => edit.error = Some(SharedString::from(err.to_string())),
            }
        }
        cx.notify();
    }

    /// 受保护提交第二步：确认后执行；DDL 原子性取决于数据库事务能力。
    pub(super) fn commit_structure(&mut self, cx: &mut Context<Self>) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|edit| edit.busy || edit.stale)
        {
            return;
        }
        let payload = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            if edit.preview.is_none() {
                if let Some(edit) = self.structure_edit.as_mut() {
                    edit.error = Some("请先生成 SQL 预览".into());
                }
                cx.notify();
                return;
            }
            if !edit.confirmed {
                if let Some(edit) = self.structure_edit.as_mut() {
                    edit.confirmed = true;
                }
                cx.notify();
                return;
            }
            (
                edit.conn,
                edit.database.clone(),
                edit.table.clone(),
                edit.pending.clone(),
            )
        };
        let (conn, database, table, changes) = payload;
        let refresh_db = database.clone();
        let refresh_table = table.clone();
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return;
        };
        let identity = (profile.id.clone(), profile.revision);
        let profiles = self.service.clone();
        let service = self.structure.clone();
        let flags = superlink_app::query::NativeFlags::from_env();
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.busy = true;
            edit.error = Some("正在提交结构变更…".into());
        }
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let mut current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
            if current.revision != profile.revision {
                return Err("连接配置已修改，请重新预览".to_string());
            }
            let credentials = profiles
                .credentials(&current)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            current.config = credentials.apply(&current.config);
            if !database.is_empty() {
                current.config.database = database.clone();
            }
            service
                .apply_with(&current, &database, &table, changes, flags)
                .map_err(|e| e.to_string())
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if this.profiles.get(conn).map(|p| (&p.id, p.revision))
                    != Some((&identity.0, identity.1))
                {
                    return;
                }
                if let Some(edit) = this.structure_edit.as_mut() {
                    if edit.conn != conn
                        || edit.database != refresh_db
                        || edit.table != refresh_table
                    {
                        return;
                    }
                    edit.busy = false;
                    match outcome {
                        Ok(statements) => {
                            edit.statements = statements;
                            edit.pending.clear();
                            edit.preview = None;
                            edit.confirmed = false;
                            edit.error = None;
                            this.refresh_structure(conn, refresh_db, refresh_table, cx);
                        }
                        Err(err) => edit.error = Some(SharedString::from(err.to_string())),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
