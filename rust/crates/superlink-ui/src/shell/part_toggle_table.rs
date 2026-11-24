use super::*;

impl SuperLinkApp {
    pub(super) fn toggle_table(
        &mut self,
        index: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        if self.open_protocol_object(index, &database, &table, cx) {
            return;
        }
        let key = (index, database.clone(), table.clone());
        if self.expanded_tables.remove(&key) {
            cx.notify();
            return;
        }
        self.expanded_tables.insert(key);
        let cache_key = (index, database.clone(), table.clone());
        if !self.table_structures.contains_key(&cache_key)
            && !self
                .column_errors
                .contains_key(&(index, database.clone(), table.clone()))
        {
            self.fetch_structure(index, database, table, cx);
        }
        cx.notify();
    }

    pub(super) fn fetch_structure(
        &mut self,
        index: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let identity = (profile.id.clone(), profile.revision);
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let db_for_task = database.clone();
        let table_for_task = table.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = read_credentials(&profiles, &profile)?;
            queries.structure(&profile, &credentials, &db_for_task, &table_for_task)
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if this.profiles.get(index).map(|p| (&p.id, p.revision))
                    != Some((&identity.0, identity.1))
                {
                    return;
                }
                let key = (index, database.clone(), table.clone());
                match outcome {
                    Ok(structure) => {
                        if let Some(bench) = this.workbench.as_mut()
                            && bench.conn == index
                            && bench.database == database
                            && bench.table == table
                        {
                            bench.columns = structure.columns.clone();
                        }
                        this.table_structures.insert(key, structure);
                        this.fetch_workbench_data(cx);
                        this.column_errors
                            .remove(&(index, database.clone(), table.clone()));
                        this.sync_catalog(cx);
                    }
                    Err(err) => {
                        this.column_errors.insert(
                            (index, database.clone(), table),
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

    pub(super) fn open_structure_editor(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        if self.workbench.as_ref().is_some_and(|b| !b.edits.is_empty())
            || self
                .structure_edit
                .as_ref()
                .is_some_and(|e| !e.pending.is_empty())
        {
            self.error = Some("请先提交或撤销当前变更".into());
            cx.notify();
            return;
        }
        let name = cx.new(SqlEditor::new);
        let data_type = cx.new(SqlEditor::new);
        let index_name = cx.new(SqlEditor::new);
        let index_columns = cx.new(SqlEditor::new);
        name.update(cx, |editor, cx| editor.set_placeholder("列名", cx));
        index_name.update(cx, |editor, cx| editor.set_placeholder("索引名", cx));
        index_columns.update(cx, |editor, cx| editor.set_placeholder("列，逗号分隔", cx));
        data_type.update(cx, |editor, cx| editor.set_placeholder("类型，如 TEXT", cx));
        index_name.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("索引名", cx)
        });
        index_columns.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("列，逗号分隔", cx)
        });
        let (columns, indexes) = self
            .table_structures
            .get(&(conn, database.clone(), table.clone()))
            .map(|structure| (structure.columns.clone(), structure.indexes.clone()))
            .unwrap_or_default();
        self.structure_edit = Some(StructureEdit {
            stale: false,
            busy: false,
            conn,
            database,
            table,
            columns,
            indexes,
            name,
            data_type,
            index_name,
            index_columns,
            index_unique: false,
            nullable: true,
            pending: Vec::new(),
            preview: None,
            confirmed: false,
            pending_delete: None,
            rename_from: None,
            rename_editor: None,
            error: None,
            statements: Vec::new(),
        });
        cx.notify();
    }

    pub(super) fn open_groups(&mut self, cx: &mut Context<Self>) {
        self.declared_groups = self.groups.declared();
        self.group_note = None;
        self.group_rename = None;
        self.groups_open = true;
        cx.notify();
    }

    pub(super) fn close_groups(&mut self, cx: &mut Context<Self>) {
        self.groups_open = false;
        self.group_rename = None;
        cx.notify();
    }

    pub(super) fn create_group(&mut self, cx: &mut Context<Self>) {
        let name = self.group_new.read(cx).text().trim().to_string();
        self.group_job(
            move |groups| {
                groups.create(&name)?;
                Ok(format!("已创建分组「{name}」"))
            },
            cx,
        );
        self.group_new
            .update(cx, |e, cx| e.set_text(String::new(), cx));
    }

    pub(super) fn start_rename_group(&mut self, name: String, cx: &mut Context<Self>) {
        let editor = cx.new(SqlEditor::new);
        editor.update(cx, |editor, cx| {
            editor.set_text(name.clone(), cx);
            editor.set_placeholder("新分组名", cx)
        });
        self.group_rename = Some((name, editor));
        cx.notify();
    }

    pub(super) fn commit_rename_group(&mut self, from: String, cx: &mut Context<Self>) {
        let Some((_, editor)) = self.group_rename.as_ref() else {
            return;
        };
        let to = editor.read(cx).text().trim().to_string();
        self.group_rename = None;
        self.group_job(
            move |groups| {
                let count = groups.rename(&from, &to)?;
                Ok(format!(
                    "已重命名「{from}」→「{to}」，{count} 个连接随之迁移"
                ))
            },
            cx,
        );
    }

    pub(super) fn delete_group(&mut self, name: String, cx: &mut Context<Self>) {
        self.group_job(
            move |groups| {
                let count = groups.remove(&name)?;
                Ok(format!("已删除分组「{name}」，{count} 个连接回到默认分组"))
            },
            cx,
        );
    }

    /// 把连接移到下一个分组（循环）。
    pub(super) fn move_profile_next(&mut self, id: String, cx: &mut Context<Self>) {
        let summaries = superlink_app::groups::summarize(&self.profiles, &self.declared_groups);
        let Some(profile) = self.profiles.iter().find(|profile| profile.id == id) else {
            return;
        };
        let current = superlink_app::groups::display_group(&profile.group);
        let index = summaries
            .iter()
            .position(|group| group.name == current)
            .map(|position| position + 1)
            .unwrap_or(0);
        let Some(next) = summaries.get(index % summaries.len().max(1)) else {
            return;
        };
        let target = next.name.clone();
        self.group_job(
            move |groups| {
                groups.move_profile(&id, &target)?;
                Ok(format!("已移动到「{target}」"))
            },
            cx,
        );
    }

    pub(super) fn close_structure_editor(&mut self, cx: &mut Context<Self>) {
        if self.structure_edit.as_ref().is_some_and(|edit| edit.busy) {
            return;
        }
        self.structure_edit = None;
        cx.notify();
    }

    /// 双击对象树中的表：打开工作台（数据页）。
    pub(super) fn open_workbench(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        self.open_workbench_mode(conn, database, table, false, cx);
    }

    pub(super) fn open_workbench_mode(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        metadata_only: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|e| e.busy || !e.pending.is_empty())
        {
            self.error = Some("请先完成或撤销结构变更".into());
            cx.notify();
            return;
        }
        self.structure_edit = None;
        if self.open_protocol_object(conn, &database, &table, cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut()
            && !bench.stale
            && bench.conn == conn
            && bench.database == database
            && bench.table == table
        {
            bench.metadata_only = metadata_only;
            if !metadata_only {
                bench.tab = crate::workbench::WorkbenchTab::Data;
            }
            self.workspace = Workspace::Sql;
            if !metadata_only && self.workbench.as_ref().is_some_and(|b| b.result.is_none()) {
                self.fetch_workbench_data(cx);
            }
            cx.notify();
            return;
        }
        if !self.park_workbench(cx) {
            return;
        }
        if let Some(position) = self
            .parked_workbenches
            .iter()
            .position(|b| !b.stale && b.conn == conn && b.database == database && b.table == table)
        {
            self.workbench = Some(self.parked_workbenches.remove(position));
            if let Some(b) = self.workbench.as_mut() {
                b.metadata_only = metadata_only;
                if !metadata_only {
                    b.tab = crate::workbench::WorkbenchTab::Data;
                }
            }
            if !metadata_only && self.workbench.as_ref().is_some_and(|b| b.result.is_none()) {
                self.fetch_workbench_data(cx);
            }
            self.workspace = Workspace::Sql;
            self.structure_edit = None;
            cx.notify();
            return;
        }
        if self.table_tab_order.len() >= 16 {
            self.error = Some("最多保留 16 个表标签，请先关闭不需要的标签".into());
            cx.notify();
            return;
        }
        if self
            .structure_edit
            .as_ref()
            .is_some_and(|e| !e.pending.is_empty())
        {
            self.error = Some("请先完成结构变更".into());
            cx.notify();
            return;
        }
        self.workspace = Workspace::Sql;
        let (columns, _indexes) = self
            .table_structures
            .get(&(conn, database.clone(), table.clone()))
            .map(|structure| (structure.columns.clone(), structure.indexes.clone()))
            .unwrap_or_default();
        self.structure_edit = None;
        self.wb_column.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("列名", cx)
        });
        self.wb_value.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("值", cx)
        });
        self.wb_import
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        let cached_columns = !columns.is_empty();
        self.workbench = Some(crate::workbench::Workbench::new(
            conn,
            database.clone(),
            table.clone(),
            columns,
        ));
        if let Some(b) = self.workbench.as_mut() {
            b.metadata_only = metadata_only;
        }
        self.table_tab_order
            .push(self.workbench.as_ref().unwrap().id.clone());
        if cached_columns {
            self.fetch_workbench_data(cx);
        } else {
            // 结构还没缓存：先取一次结构，回来后再取数。
            let db = database.clone();
            let name = table.clone();
            self.fetch_structure(conn, db, name, cx);
            self.fetch_workbench_data(cx);
            cx.notify();
        }
    }

    pub(super) fn close_workbench(&mut self, cx: &mut Context<Self>) {
        if let Some(b) = self.workbench.as_mut()
            && (!b.edits.is_empty()
                || b.designer
                    .as_ref()
                    .is_some_and(|d| d.read(cx).is_dirty() || d.read(cx).is_busy()))
        {
            b.error = Some("有未保存变更，请先保存或放弃草稿（数据编辑可撤销变更）".into());
            cx.notify();
            return;
        }
        if let Some(cancel) = self.workbench.as_mut().and_then(|b| b.operation.take()) {
            cx.background_executor()
                .spawn(async move {
                    cancel.cancel();
                })
                .detach();
        }
        if let Some(bench) = self.workbench.take() {
            self.table_tab_order.retain(|id| id != &bench.id);
        }
        cx.notify();
    }

    pub(super) fn fetch_workbench_data(&mut self, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_ref() else {
            return;
        };
        if bench.stale || bench.metadata_only || bench.tab != crate::workbench::WorkbenchTab::Data {
            return;
        }
        if !bench.edits.is_empty() {
            return;
        }
        if bench.columns.is_empty() {
            return;
        }
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        let kind = profile.sql_dialect();
        let (data_sql, count_sql) = match bench.page_statements(kind) {
            Ok(statements) => statements,
            Err(err) => {
                if let Some(bench) = self.workbench.as_mut() {
                    bench.error = Some(SharedString::from(err.to_string()));
                }
                cx.notify();
                return;
            }
        };
        let identity = if let Some(bench) = self.workbench.as_mut() {
            bench.request = bench.request.wrapping_add(1);
            (bench.id.clone(), bench.request)
        } else {
            return;
        };
        let mut profile = profile;
        let database = self.workbench.as_ref().unwrap().database.clone();
        if !database.is_empty() {
            profile.config.database = database;
        }
        if let Some(bench) = self.workbench.as_mut() {
            bench.loading = true;
            bench.error = None;
        }
        let cancel = superlink_app::QueryCancel::new();
        if let Some(bench) = self.workbench.as_mut()
            && let Some(previous) = bench.operation.replace(cancel.clone())
        {
            cx.background_executor()
                .spawn(async move {
                    previous.cancel();
                })
                .detach();
        }
        let page_size = self.workbench.as_ref().unwrap().page_size;
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let result: Result<(superlink_domain::ResultSet, superlink_domain::ResultSet), String> =
                (|| {
                    let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
                    if current.revision != profile.revision {
                        return Err("连接配置已修改，请刷新表".into());
                    }
                    let credentials = profiles
                        .credentials(&current)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default();
                    let request = superlink_domain::Execution {
                        revision: profile.revision,
                        text: format!("{data_sql}; {count_sql}"),
                        scope: profile.config.database.clone(),
                        max_rows: page_size + 1,
                        ..Default::default()
                    };
                    let mut results = queries
                        .execute_request(&current, &credentials, &request, &cancel)
                        .map_err(|e| e.to_string())?;
                    if results.len() != 2 {
                        return Err("表数据或计数结果缺失".into());
                    }
                    let count = results.pop().unwrap();
                    let data = results.pop().unwrap();
                    Ok((data, count))
                })();
            match result {
                Ok((data, count)) => (Ok(data), Ok(count)),
                Err(error) => (Err(error.clone()), Err(error)),
            }
        });
        cx.spawn(async move |this, cx| {
            let (data, count) = task.await;
            this.update(cx, |this, cx| {
                if let Some(bench) = this.workbench.as_mut() {
                    if (bench.id.as_str(), bench.request) != (identity.0.as_str(), identity.1) {
                        return;
                    }
                    bench.loading = false;
                    bench.operation = None;
                    match (data, count) {
                        (Ok(data), Ok(count)) => {
                            bench.error = None;
                            bench.set_data(data, Some(&count));
                        }
                        (Err(err), _) | (_, Err(err)) => {
                            bench.error = Some(SharedString::from(err.to_string()));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn apply_workbench_filter(&mut self, cx: &mut Context<Self>) {
        if !self.can_reload_workbench(cx) {
            return;
        }
        let column = self.wb_column.read(cx).text().trim().to_string();
        let value = self.wb_value.read(cx).text().trim().to_string();
        let (committed, refresh) = {
            let Some(bench) = self.workbench.as_mut() else {
                return;
            };
            bench.draft.column = column;
            bench.draft.value = value;
            match bench.commit_draft() {
                Ok(()) => (true, bench.database.clone()),
                Err(err) => {
                    bench.error = Some(SharedString::from(err));
                    (false, String::new())
                }
            }
        };
        if !committed {
            cx.notify();
            return;
        }
        self.wb_column
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        self.wb_value
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        let _ = refresh;
        self.fetch_workbench_data(cx);
    }

    pub(super) fn cycle_workbench_operator(&mut self, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.draft.operator = (bench.draft.operator + 1) % crate::workbench::OPERATORS.len();
        }
        cx.notify();
    }

    pub(super) fn toggle_workbench_join(&mut self, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.draft.join_and = !bench.draft.join_and;
        }
        cx.notify();
    }

    pub(super) fn remove_workbench_filter(&mut self, position: usize, cx: &mut Context<Self>) {
        if !self.can_reload_workbench(cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut() {
            bench.filters.remove(position);
            bench.page = 1;
        }
        self.fetch_workbench_data(cx);
    }

    pub(super) fn workbench_sort(&mut self, column: String, cx: &mut Context<Self>) {
        if !self.can_reload_workbench(cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut() {
            bench.toggle_sort(&column);
        }
        self.fetch_workbench_data(cx);
    }

    pub(super) fn workbench_page(&mut self, delta: i32, cx: &mut Context<Self>) {
        if !self.can_reload_workbench(cx) {
            return;
        }
        if let Some(bench) = self.workbench.as_mut() {
            if delta < 0 {
                bench.go_prev();
            } else {
                bench.go_next();
            }
        }
        self.fetch_workbench_data(cx);
    }

    pub(super) fn set_workbench_tab(
        &mut self,
        tab: crate::workbench::WorkbenchTab,
        cx: &mut Context<Self>,
    ) {
        if let Some(bench) = self.workbench.as_mut() {
            if tab == crate::workbench::WorkbenchTab::Structure && !bench.edits.is_empty() {
                bench.error = Some("请先提交或撤销数据编辑，再修改表结构".into());
                cx.notify();
                return;
            }
            bench.tab = tab;
            if tab == crate::workbench::WorkbenchTab::Data {
                bench.metadata_only = false;
            }
        }
        if tab == crate::workbench::WorkbenchTab::Data
            && self.workbench.as_ref().is_some_and(|b| b.result.is_none())
        {
            self.fetch_workbench_data(cx);
        }
        if tab == crate::workbench::WorkbenchTab::Structure {
            self.ensure_field_designer(cx);
        }
        if tab == crate::workbench::WorkbenchTab::Ddl {
            self.fetch_workbench_ddl(cx);
        }
        cx.notify();
    }
}
