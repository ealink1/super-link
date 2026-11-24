use super::*;

impl SuperLinkApp {
    pub(super) fn table_row_label(
        &self,
        conn: usize,
        database: &str,
        table: &str,
    ) -> Option<String> {
        if let Some(count) = self
            .workbench
            .iter()
            .chain(self.parked_workbenches.iter())
            .find(|b| {
                b.conn == conn
                    && b.database == database
                    && b.table == table
                    && !b.stale
                    && b.filters.is_empty()
            })
            .and_then(|b| b.total)
        {
            return Some(format!("{count}行"));
        }
        self.table_counts
            .get(&(conn, database.into(), table.into()))
            .map(|count| format!("≈{count}行"))
    }

    pub(super) fn fetch_table_counts(
        &mut self,
        conn: usize,
        database: String,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return;
        };
        let sql = superlink_sql::table_catalog_query(profile.sql_dialect(), &database);
        if sql.is_empty() {
            return;
        }
        let identity = (profile.id.clone(), profile.revision);
        let queries = self.queries.clone();
        let profiles = self.service.clone();
        let db = database.clone();
        let task = cx.background_executor().spawn(async move {
            let credentials = read_credentials(&profiles, &profile)?;
            let request = superlink_domain::Execution {
                text: sql,
                scope: db,
                max_rows: 10000,
                revision: profile.revision,
                ..Default::default()
            };
            queries.execute_request(
                &profile,
                &credentials,
                &request,
                &superlink_app::QueryCancel::new(),
            )
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.profiles.get(conn).map(|p| (&p.id, p.revision))
                    != Some((&identity.0, identity.1))
                {
                    return;
                }
                this.table_counts
                    .retain(|(index, db, _), _| *index != conn || db != &database);
                if let Ok(results) = outcome
                    && let Some(result) = results.first()
                {
                    for row in result.rows.iter().take(10000) {
                        if let Some(name) = row.first().and_then(serde_json::Value::as_str)
                            && let Some(count) = row.get(1).and_then(|v| {
                                v.as_i64()
                                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                            })
                            && count >= 0
                        {
                            this.table_counts
                                .insert((conn, database.clone(), name.into()), count);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
