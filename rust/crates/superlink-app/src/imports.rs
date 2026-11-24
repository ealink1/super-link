//! Bounded import preview followed by one parameterized, cancellable transaction.
use crate::{Credentials, QueryCancel, QueryError, QueryService};
use std::path::Path;
use superlink_domain::Profile;
use superlink_files::{
    cancel::Cancellation,
    import::{Dataset, ReadOptions, read_file},
};
use superlink_sql::{ArgValue, Object};

#[derive(Debug)]
pub struct ImportPlan {
    pub rows: usize,
    pub columns: Vec<String>,
    pub statement: String,
    pub revision: i64,
    pub profile_id: String,
    pub object: Object,
    dataset: Dataset,
    mapping: Vec<(usize, String)>,
}
struct Cancel<'a>(&'a QueryCancel);
impl Cancellation for Cancel<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

impl QueryService {
    pub fn preview_import(
        &self,
        profile: &Profile,
        path: &Path,
        object: Object,
        columns: &[superlink_drivers::ColumnEntry],
        cancel: &QueryCancel,
    ) -> Result<ImportPlan, String> {
        crate::execution_policy::authorize(profile, "INSERT INTO import_target", true, "import")?;
        if !superlink_domain::resolve(&profile.config.kind)
            .is_ok_and(|d| d.family == superlink_domain::Family::Sql)
        {
            return Err("文件导入需要 SQL 数据源".into());
        }
        let dataset =
            read_file(&Cancel(cancel), path, &ReadOptions::default()).map_err(|e| e.to_string())?;
        if dataset.rows.is_empty() {
            return Err("导入文件没有数据行".into());
        }
        let mut mapping = Vec::new();
        let mut names = Vec::new();
        for (index, header) in dataset.columns.iter().enumerate() {
            let column = columns
                .iter()
                .find(|column| column.name.eq_ignore_ascii_case(header))
                .ok_or_else(|| format!("文件列 {header} 不在目标表中，请调整文件表头"))?;
            if column.extra.to_lowercase().contains("generated")
                || column.extra.to_lowercase().contains("identity always")
            {
                return Err(format!("生成字段 {} 不能导入", column.name));
            }
            mapping.push((index, column.data_type.clone()));
            names.push(column.name.clone());
        }
        let dialect = profile.sql_dialect();
        let table = superlink_sql::object_name(dialect, &object).map_err(|e| e.to_string())?;
        let quoted = names
            .iter()
            .map(|c| superlink_sql::quote_identifier(dialect, c).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let placeholders = (1..=names.len())
            .map(|i| superlink_sql::placeholder(dialect, i))
            .collect::<Vec<_>>();
        let statement = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            quoted.join(", "),
            placeholders.join(", ")
        );
        // Validate conversion before opening a connection or changing any row.
        for (row_index, row) in dataset.rows.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err("导入预览已停止".into());
            }
            for (index, kind) in &mapping {
                superlink_sql::convert_value(
                    kind,
                    &row[*index].to_json().map_err(|e| e.to_string())?,
                )
                .map_err(|_| format!("第 {} 行的列 {} 类型不匹配", row_index + 2, names[*index]))?;
            }
        }
        Ok(ImportPlan {
            rows: dataset.rows.len(),
            columns: names,
            statement,
            revision: profile.revision,
            profile_id: profile.id.clone(),
            object,
            dataset,
            mapping,
        })
    }
    pub fn apply_import(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        plan: &ImportPlan,
        confirmed: bool,
        cancel: &QueryCancel,
    ) -> Result<usize, QueryError> {
        if !confirmed || plan.profile_id != profile.id || plan.revision != profile.revision {
            return Err(QueryError::Policy("请重新预览并确认导入".into()));
        }
        crate::execution_policy::authorize(profile, "INSERT INTO import_target", true, "import")
            .map_err(QueryError::Policy)?;
        let mut profile = profile.clone();
        crate::scope::apply(&mut profile.config, &plan.object.scope).map_err(QueryError::Policy)?;
        let _operation = crate::execution::Operation::new(cancel, profile.config.query_timeout);
        let mut connection = self.open_operation(&profile, credentials, cancel)?;
        if !connection.transactional() {
            return Err(QueryError::Policy(
                "此驱动不支持事务导入，未写入数据".into(),
            ));
        }
        connection.query("BEGIN", &[], 1)?;
        let result = (|| {
            for row in &plan.dataset.rows {
                if cancel.is_cancelled() {
                    return Err(QueryError::Cancelled);
                }
                let args = plan
                    .mapping
                    .iter()
                    .map(|(index, kind)| {
                        let value = row[*index]
                            .to_json()
                            .map_err(|e| QueryError::Policy(e.to_string()))?;
                        let arg = superlink_sql::convert_value(kind, &value)
                            .map_err(|e| QueryError::Policy(e.to_string()))?;
                        Ok(match arg {
                            ArgValue::Bytes(bytes) => superlink_drivers::arguments::bytes(&bytes),
                            other => other.to_json(),
                        })
                    })
                    .collect::<Result<Vec<_>, QueryError>>()?;
                connection.query(&plan.statement, &args, 1)?;
            }
            connection.query("COMMIT", &[], 1)?;
            Ok(plan.rows)
        })();
        if result.is_err() {
            let _ = connection.query("ROLLBACK", &[], 1);
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_import_rolls_back_and_protected_import_never_opens_file() {
        let dir = std::env::temp_dir().join(format!("sl-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let service = QueryService::new("/missing");
        let credentials = Credentials::default();
        let cancel = QueryCancel::new();
        let mut profile = Profile {
            revision: 1,
            config: superlink_domain::ConnectionConfig {
                kind: "sqlite".into(),
                database: dir.join("db").to_string_lossy().into(),
                ..Default::default()
            },
            ..Default::default()
        };
        service
            .execute(
                &profile,
                &credentials,
                "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT)",
                1,
            )
            .unwrap();
        std::fs::write(dir.join("data.csv"), "id,name\n1,甲\n1,乙\n").unwrap();
        let columns = service
            .structure(&profile, &credentials, "", "t")
            .unwrap()
            .columns;
        let object = Object {
            name: "t".into(),
            kind: "table".into(),
            ..Default::default()
        };
        let plan = service
            .preview_import(
                &profile,
                &dir.join("data.csv"),
                object.clone(),
                &columns,
                &cancel,
            )
            .unwrap();
        assert!(
            service
                .apply_import(&profile, &credentials, &plan, true, &cancel)
                .is_err()
        );
        assert_eq!(
            service
                .execute(&profile, &credentials, "SELECT count(*) FROM t", 1)
                .unwrap()
                .rows[0][0],
            serde_json::json!(0)
        );
        profile.read_only = true;
        let error = service
            .preview_import(
                &profile,
                &dir.join("missing.csv"),
                object,
                &columns,
                &cancel,
            )
            .err()
            .unwrap();
        assert!(error.contains("只读"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
