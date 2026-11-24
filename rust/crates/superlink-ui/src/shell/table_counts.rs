use super::*;
use serde_json::Value;
use superlink_drivers::TableStats;

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
        self.table_stats(conn, database, table)
            .and_then(|stats| stats.rows)
            .map(|count| format!("≈{count}行"))
    }

    pub(super) fn table_stats(
        &self,
        conn: usize,
        database: &str,
        table: &str,
    ) -> Option<&TableStats> {
        self.table_counts
            .get(&(conn, database.into(), table.into()))
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
                        let Some(name) = row.first().and_then(Value::as_str) else {
                            continue;
                        };
                        let parsed = stats_from_row(row);
                        if parsed.rows.is_some() || parsed.engine.is_some() {
                            this.table_counts
                                .insert((conn, database.clone(), name.into()), parsed);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// `table_catalog_query` 的列顺序：名称、行数、数据长度、引擎、创建、修改、
/// 排序规则、注释、时区偏移（秒）。
fn stats_from_row(row: &[Value]) -> TableStats {
    let offset = row
        .get(8)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(0);
    let text = |index: usize| {
        row.get(index)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    TableStats {
        rows: count(row, 1),
        data_length: count(row, 2),
        engine: text(3),
        created: text(4).map(|value| with_offset(&value, offset)),
        updated: text(5).map(|value| with_offset(&value, offset)),
        collation: text(6),
        comment: merge_comment(text(7)),
    }
}

fn count(row: &[Value], index: usize) -> Option<u64> {
    row.get(index).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
    })
}

/// 非 UTC 实例的目录时间是本地挂钟时间，补上偏移才能对齐参考图的
/// `2026-10-20T11:26:03+08:00`。
fn with_offset(value: &str, offset_seconds: i64) -> String {
    let stamped = value.trim().replace(' ', "T");
    if offset_seconds == 0 || stamped.contains('+') {
        return stamped;
    }
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let abs = offset_seconds.abs();
    format!("{stamped}{sign}{:02}:{:02}", abs / 3600, (abs % 3600) / 60)
}

/// MySQL 对只有表级注释的表写入 `InnoDB free: ...` 之类的内部说明，不展示。
fn merge_comment(comment: Option<String>) -> Option<String> {
    comment.filter(|value| !value.trim().is_empty() && !value.starts_with("InnoDB free"))
}

/// 与参考对象页一致的 `16 KB` / `1.5 MB` 显示。
pub(super) fn format_bytes(bytes: u64) -> Option<String> {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let text = if value >= 100.0 || value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    Some(format!("{text} {}", UNITS[unit]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_the_catalog_row() {
        let row = [
            json!("test_tb"),
            json!(0),
            json!(16384),
            json!("InnoDB"),
            json!("2026-10-20 11:26:03"),
            json!("2026-10-20 12:53:41"),
            json!("utf8mb4_bin"),
            json!(""),
            json!(28800),
        ];
        let stats = stats_from_row(&row);
        assert_eq!(stats.rows, Some(0));
        assert_eq!(stats.data_length, Some(16384));
        assert_eq!(stats.engine.as_deref(), Some("InnoDB"));
        assert_eq!(stats.created.as_deref(), Some("2026-10-20T11:26:03+08:00"));
        assert_eq!(stats.collation.as_deref(), Some("utf8mb4_bin"));
        assert_eq!(stats.comment, None);
    }

    #[test]
    fn keeps_utc_timestamps_untouched() {
        assert_eq!(with_offset("2026-10-20 11:26:03", 0), "2026-10-20T11:26:03");
        assert_eq!(
            with_offset("2026-10-20 11:26:03", -18000),
            "2026-10-20T11:26:03-05:00"
        );
    }

    #[test]
    fn hides_engine_placeholders_and_formats_bytes() {
        assert_eq!(merge_comment(Some("InnoDB free: 8192 kB".into())), None);
        assert_eq!(format_bytes(16384).as_deref(), Some("16 KB"));
        assert_eq!(format_bytes(0).as_deref(), Some("0 KB"));
        assert_eq!(format_bytes(5 * 1024 * 1024), Some("5 MB".into()));
        assert_eq!(format_bytes(1536), Some("1.5 KB".into()));
    }
}
