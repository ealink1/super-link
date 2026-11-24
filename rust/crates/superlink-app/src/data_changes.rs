//! Atomic parameterized row edits. An optimistic mismatch rolls back the batch.
use crate::{Credentials, QueryCancel, QueryError, QueryService};
use superlink_domain::Profile;
use superlink_sql::{ArgValue, Statement, TableChanges, TableInfo};

impl QueryService {
    pub fn preview_data_changes(
        &self,
        profile: &Profile,
        changes: &TableChanges,
        info: &TableInfo,
    ) -> Result<Vec<Statement>, QueryError> {
        if changes.revision != profile.revision {
            return Err(QueryError::Policy("连接配置已修改，请刷新表".into()));
        }
        crate::execution_policy::authorize(profile, "UPDATE table_data", true, "data")
            .map_err(QueryError::Policy)?;
        superlink_sql::build_changes(profile.sql_dialect(), changes, info)
            .map_err(|e| QueryError::Policy(e.to_string()))
    }

    pub fn apply_data_changes(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        changes: &TableChanges,
        info: &TableInfo,
        cancel: &QueryCancel,
    ) -> Result<usize, QueryError> {
        let statements = self.preview_data_changes(profile, changes, info)?;
        if changes.confirmation != "apply" {
            return Err(QueryError::Policy("请先预览并确认变更".into()));
        }
        let mut live = profile.clone();
        crate::scope::apply(&mut live.config, &changes.object.scope).map_err(QueryError::Policy)?;
        let _operation = crate::execution::Operation::new(cancel, live.config.query_timeout);
        let mut connection = self.open_operation(&live, credentials, cancel)?;
        if !connection.transactional() {
            return Err(QueryError::Policy(
                "此驱动不支持事务编辑，未写入数据".into(),
            ));
        }
        connection.query("BEGIN", &[], 1)?;
        let outcome = (|| {
            for statement in &statements {
                if cancel.is_cancelled() {
                    return Err(QueryError::Cancelled);
                }
                let args = statement
                    .args
                    .iter()
                    .map(|value| match value {
                        ArgValue::Bytes(bytes) => superlink_drivers::arguments::bytes(bytes),
                        other => other.to_json(),
                    })
                    .collect::<Vec<_>>();
                let result = connection.query(&statement.sql, &args, 1)?;
                if statement.require_one && result.rows_affected != 1 {
                    return Err(QueryError::Policy(
                        "原数据已变化或定位不唯一，已回滚，请刷新后重试".into(),
                    ));
                }
            }
            connection.query("COMMIT", &[], 1)?;
            Ok(statements.len())
        })();
        if outcome.is_err() {
            let _ = connection.query("ROLLBACK", &[], 1);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn a_conflict_rolls_back_preceding_edits() {
        let path = std::env::temp_dir().join(format!("sl-edits-{}.db", uuid::Uuid::new_v4()));
        let profile = Profile {
            revision: 1,
            config: superlink_domain::ConnectionConfig {
                kind: "sqlite".into(),
                database: path.to_string_lossy().into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let service = QueryService::new("/missing");
        let credentials = Credentials::default();
        service.execute(&profile,&credentials,"CREATE TABLE t(id INTEGER PRIMARY KEY,v TEXT); INSERT INTO t VALUES(1,'a'),(2,'b');",10).unwrap();
        let info = TableInfo {
            columns: vec![
                superlink_sql::ColumnDefinition {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    key: "PRI".into(),
                    ..Default::default()
                },
                superlink_sql::ColumnDefinition {
                    name: "v".into(),
                    data_type: "TEXT".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let changes = TableChanges {
            revision: 1,
            object: superlink_sql::Object {
                name: "t".into(),
                kind: "table".into(),
                ..Default::default()
            },
            confirmation: "apply".into(),
            rows: vec![
                superlink_sql::RowChange {
                    kind: "update".into(),
                    original: json!({"id":1,"v":"a"}).as_object().unwrap().clone(),
                    values: json!({"v":"changed"}).as_object().unwrap().clone(),
                },
                superlink_sql::RowChange {
                    kind: "delete".into(),
                    original: json!({"id":99,"v":"missing"}).as_object().unwrap().clone(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert!(
            service
                .apply_data_changes(&profile, &credentials, &changes, &info, &QueryCancel::new())
                .is_err()
        );
        assert_eq!(
            service
                .execute(&profile, &credentials, "SELECT v FROM t WHERE id=1", 10)
                .unwrap()
                .rows,
            vec![vec![json!("a")]]
        );
        let _ = std::fs::remove_file(path);
    }
}
