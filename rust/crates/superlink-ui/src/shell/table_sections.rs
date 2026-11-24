use super::*;
use crate::workbench::{TableSection, WorkbenchTab};

impl SuperLinkApp {
    pub(super) fn table_sections(
        &self,
        conn: usize,
        database: &str,
        table: &str,
        table_indent: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut list = div().flex().flex_col().flex_shrink_0();
        for section in TableSection::ALL {
            let db = database.to_owned();
            let name = table.to_owned();
            let selected = self.workbench.as_ref().is_some_and(|b| {
                b.conn == conn
                    && b.database == database
                    && b.table == table
                    && b.tab == WorkbenchTab::Structure
                    && b.section == section
            });
            list = list.child(
                div()
                    .id(SharedString::from(format!(
                        "table-section-{conn}-{database}-{table}-{section:?}"
                    )))
                    .flex()
                    .items_center()
                    .gap(px(tree_style::GAP))
                    .h(px(tree_style::ROW))
                    .flex_shrink_0()
                    .pl(px(table_indent
                        + tree_style::INDENT
                        + 12.
                        + tree_style::GAP))
                    .text_size(px(tree_style::FONT))
                    .text_color(theme.text)
                    .cursor_pointer()
                    .rounded_md()
                    .bg(if selected {
                        gpui::Hsla::from(gpui::rgb(0x17823b)).opacity(0.18)
                    } else {
                        gpui::transparent_black()
                    })
                    .hover(move |s| s.bg(theme.border.opacity(0.22)))
                    .child(icons::icon(section.icon(), tree_style::ICON, theme.muted))
                    .child(section.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_table_section(conn, db.clone(), name.clone(), section, cx)
                    })),
            );
        }
        list.into_any_element()
    }

    fn open_table_section(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        section: TableSection,
        cx: &mut Context<Self>,
    ) {
        if self.workbench.as_ref().is_some_and(|b| {
            b.loading || b.ddl_loading || b.designer.as_ref().is_some_and(|d| d.read(cx).is_busy())
        }) {
            self.error = Some("请等待当前操作完成".into());
            cx.notify();
            return;
        }
        if self.workbench.as_ref().is_some_and(|b| {
            b.conn == conn && b.database == database && b.table == table && !b.edits.is_empty()
        }) {
            self.error = Some("请先提交或撤销数据编辑，再修改表结构".into());
            cx.notify();
            return;
        }
        self.open_workbench_mode(conn, database.clone(), table.clone(), true, cx);
        if let Some(b) = self.workbench.as_mut() {
            if b.conn != conn || b.database != database || b.table != table || b.stale {
                return;
            }
            b.section = section;
            b.tab = WorkbenchTab::Structure;
        } else {
            return;
        }
        if section == TableSection::Columns {
            self.ensure_field_designer(cx);
        }
        if section == TableSection::Triggers {
            self.fetch_table_triggers(cx);
        }
        cx.notify();
    }

    pub(super) fn select_table_section(&mut self, section: TableSection, cx: &mut Context<Self>) {
        if let Some(b) = self.workbench.as_mut() {
            if b.loading || b.designer.as_ref().is_some_and(|d| d.read(cx).is_busy()) {
                return;
            }
            b.section = section;
        }
        if section == TableSection::Columns {
            self.ensure_field_designer(cx);
        }
        if section == TableSection::Triggers {
            self.fetch_table_triggers(cx);
        }
        cx.notify();
    }

    pub(super) fn refresh_workbench(&mut self, cx: &mut Context<Self>) {
        let Some(b) = self.workbench.as_mut() else {
            return;
        };
        if b.loading || b.stale {
            return;
        }
        match b.tab {
            WorkbenchTab::Data => self.fetch_workbench_data(cx),
            WorkbenchTab::Ddl => {
                b.ddl_source = None;
                self.fetch_workbench_ddl(cx);
            }
            WorkbenchTab::Structure => {
                let (conn, db, table, triggers) = (
                    b.conn,
                    b.database.clone(),
                    b.table.clone(),
                    b.section == TableSection::Triggers,
                );
                b.triggers = None;
                self.fetch_structure(conn, db, table, cx);
                if triggers {
                    self.fetch_table_triggers(cx);
                }
            }
        }
    }

    fn fetch_table_triggers(&mut self, cx: &mut Context<Self>) {
        let Some(b) = self.workbench.as_mut() else {
            return;
        };
        if b.stale || b.loading || b.triggers.is_some() {
            return;
        }
        let Some(profile) = self.profiles.get(b.conn).cloned() else {
            return;
        };
        let Some(sql) =
            superlink_sql::table_triggers::table_triggers_query(profile.sql_dialect(), &b.object())
        else {
            b.trigger_error = Some("该数据源尚不支持读取触发器目录".into());
            cx.notify();
            return;
        };
        let database = b.database.clone();
        b.request = b.request.wrapping_add(1);
        let identity = (b.id.clone(), b.request);
        b.loading = true;
        b.trigger_error = None;
        let cancel = superlink_app::QueryCancel::new();
        b.operation = Some(cancel.clone());
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = read_credentials(&profiles, &profile).map_err(|e| e.to_string())?;
            let request = superlink_domain::Execution {
                text: sql,
                scope: database,
                revision: profile.revision,
                max_rows: 1000,
                ..Default::default()
            };
            queries
                .execute_request(&profile, &credentials, &request, &cancel)
                .map_err(|e| e.to_string())?
                .into_iter()
                .next()
                .ok_or_else(|| "触发器目录响应缺失".to_string())
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(b) = this.workbench.as_mut()
                    && (b.id.as_str(), b.request) == (identity.0.as_str(), identity.1)
                {
                    b.loading = false;
                    b.operation = None;
                    match outcome {
                        Ok(result) => b.triggers = Some(result),
                        Err(error) => b.trigger_error = Some(error.into()),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sqlite_sections_read_real_columns_indexes_keys_and_triggers_without_writes() {
        let config = superlink_domain::ConnectionConfig {
            kind: "sqlite".into(),
            database: ":memory:".into(),
            ..Default::default()
        };
        let driver = superlink_drivers::NativeSqlite::open(&config).unwrap();
        driver.execute_batch(&[
            "CREATE TABLE parent(id INTEGER PRIMARY KEY)".into(),
            "CREATE TABLE \"child'table\"(id INTEGER PRIMARY KEY,parent_id INTEGER REFERENCES parent(id))".into(),
            "CREATE INDEX by_parent ON \"child'table\"(parent_id)".into(),
            "CREATE TRIGGER child_insert AFTER INSERT ON \"child'table\" BEGIN SELECT NEW.id; END".into(),
        ]).unwrap();
        let structure = driver.structure("child'table").unwrap();
        assert_eq!(structure.columns.len(), 2);
        assert_eq!(structure.indexes[0].name, "by_parent");
        assert_eq!(structure.foreign_keys[0].referenced_table, "parent");
        let object = superlink_sql::Object {
            name: "child'table".into(),
            ..Default::default()
        };
        let sql = superlink_sql::table_triggers::table_triggers_query("sqlite", &object).unwrap();
        let triggers = driver.query(&sql, &[], 1000).unwrap();
        assert_eq!(triggers.rows.len(), 1);
        assert_eq!(triggers.rows[0][0], "child_insert");
        assert!(
            triggers.rows[0][1]
                .as_str()
                .unwrap()
                .contains("AFTER INSERT")
        );
        let rows = driver
            .query("SELECT COUNT(*) FROM \"child'table\"", &[], 1)
            .unwrap();
        assert_eq!(rows.rows[0][0], 0);
    }
}
