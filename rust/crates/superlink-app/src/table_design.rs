//! Whole-field designer reads and writes. No UI state or credentials are persisted.
use crate::{Credentials, QueryCancel, QueryError, QueryService};
use serde_json::Value;
use superlink_domain::{Execution, Profile};
use superlink_sql::design::{DesignDraft, DesignSnapshot, design_sql};
use superlink_sql::{ColumnDefinition, Object, text_literal};

pub struct DesignEdit<'a> {
    pub table: &'a str,
    pub before: &'a DesignSnapshot,
    pub draft: &'a DesignDraft,
}

impl QueryService {
    pub fn design_snapshot(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        table: &str,
        cancel: &QueryCancel,
    ) -> Result<DesignSnapshot, QueryError> {
        let kind = profile.sql_dialect();
        if superlink_sql::structure_dialect(kind) == "mysql" {
            let scope = if database.is_empty() {
                &profile.config.database
            } else {
                database
            };
            let sql = format!(
                "SELECT COLUMN_NAME,COLUMN_TYPE,IS_NULLABLE,COLUMN_KEY,COLUMN_DEFAULT,EXTRA,COLUMN_COMMENT,CHARACTER_SET_NAME,COLLATION_NAME FROM information_schema.COLUMNS WHERE TABLE_SCHEMA={} AND TABLE_NAME={} ORDER BY ORDINAL_POSITION LIMIT 4096; SELECT TABLE_COMMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA={} AND TABLE_NAME={} LIMIT 1",
                text_literal(kind, scope),
                text_literal(kind, table),
                text_literal(kind, scope),
                text_literal(kind, table)
            );
            let result = self.execute_request(
                profile,
                credentials,
                &Execution {
                    text: sql,
                    scope: database.into(),
                    max_rows: 4097,
                    revision: profile.revision,
                    ..Default::default()
                },
                cancel,
            )?;
            let columns = result
                .first()
                .ok_or_else(|| QueryError::Policy("字段目录响应缺失".into()))?
                .rows
                .iter()
                .map(|row| {
                    let get = |i| {
                        row.get(i)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    };
                    let extra = get(5);
                    let default = row.get(4).filter(|v| !v.is_null()).map(|v| {
                        let text = v
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string());
                        if extra.to_lowercase().contains("default_generated") {
                            text
                        } else {
                            text_literal("sqlite", &text)
                        }
                    });
                    ColumnDefinition {
                        name: get(0),
                        data_type: get(1),
                        nullable: get(2),
                        key: get(3),
                        has_default: default.is_some(),
                        default,
                        extra,
                        comment: get(6),
                        charset: get(7),
                        collation: get(8),
                    }
                })
                .collect::<Vec<_>>();
            if columns.is_empty() {
                return Err(QueryError::Policy("目标表不存在或没有读取权限".into()));
            }
            let comment = result
                .get(1)
                .and_then(|r| r.rows.first())
                .and_then(|r| r.first())
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into();
            return Ok(DesignSnapshot { columns, comment });
        }
        let structure = self.structure(profile, credentials, database, table)?;
        let info = crate::structure::table_info_from(&structure.columns, &structure.indexes);
        let comment = if superlink_sql::structure_dialect(kind) == "postgres" {
            let object = design_object(kind, database, table);
            let path = superlink_sql::object_name(kind, &object)
                .map_err(|e| QueryError::Policy(e.to_string()))?;
            let results = self.execute_request(
                profile,
                credentials,
                &Execution {
                    text: format!(
                        "SELECT obj_description(to_regclass({}),'pg_class')",
                        text_literal(kind, &path)
                    ),
                    scope: database.into(),
                    max_rows: 1,
                    revision: profile.revision,
                    ..Default::default()
                },
                cancel,
            )?;
            results
                .first()
                .and_then(|r| r.rows.first())
                .and_then(|r| r.first())
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        } else {
            String::new()
        };
        Ok(DesignSnapshot {
            columns: info
                .columns
                .into_iter()
                .zip(structure.columns)
                .map(|(mut c, entry)| {
                    c.comment = entry.comment.unwrap_or_default();
                    if c.key == "PRI" {
                        c.nullable = "NO".into();
                    }
                    c
                })
                .collect(),
            comment,
        })
    }

    pub fn save_design(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        edit: DesignEdit<'_>,
        cancel: &QueryCancel,
    ) -> Result<DesignSnapshot, QueryError> {
        let DesignEdit {
            table,
            before,
            draft,
        } = edit;
        crate::execution_policy::authorize(profile, "ALTER TABLE object", true, "structure")
            .map_err(QueryError::Policy)?;
        let fresh = self.design_snapshot(profile, credentials, database, table, cancel)?;
        if &fresh != before {
            return Err(QueryError::Policy(
                "数据库字段结构已被修改，请重新读取后合并草稿".into(),
            ));
        }
        let object = design_object(profile.sql_dialect(), database, table);
        let statements = design_sql(profile.sql_dialect(), &object, before, draft)
            .map_err(|e| QueryError::Policy(e.to_string()))?;
        self.execute_design_sql(profile, credentials, database, &statements, cancel)?;
        self.design_snapshot(profile, credentials, database, table, cancel)
    }

    pub fn execute_design_sql(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        statements: &[String],
        cancel: &QueryCancel,
    ) -> Result<(), QueryError> {
        if statements.is_empty() {
            return Ok(());
        }
        let mut sql = statements.join(";\n");
        if superlink_sql::structure_dialect(profile.sql_dialect()) == "mysql" {
            sql = format!("{};\n{sql}", superlink_sql::identifier::MYSQL_DDL_SESSION);
        }
        if matches!(
            superlink_sql::structure_dialect(profile.sql_dialect()),
            "sqlite" | "postgres"
        ) {
            sql = format!("BEGIN;\n{sql};\nCOMMIT");
        }
        self.execute_request(
            profile,
            credentials,
            &Execution {
                text: sql,
                scope: database.into(),
                write: true,
                action: "structure".into(),
                revision: profile.revision,
                max_rows: 1,
                ..Default::default()
            },
            cancel,
        )?;
        Ok(())
    }
}

pub fn design_object(kind: &str, database: &str, table: &str) -> Object {
    let (schema, name) = if superlink_sql::structure_dialect(kind) == "postgres" {
        table
            .split_once('.')
            .map(|(s, n)| (s.to_string(), n.to_string()))
            .unwrap_or_else(|| (String::new(), table.into()))
    } else {
        (String::new(), table.into())
    };
    Object {
        name,
        schema,
        scope: database.into(),
        kind: "table".into(),
    }
}

#[cfg(test)]
#[path = "table_design_tests.rs"]
mod tests;
