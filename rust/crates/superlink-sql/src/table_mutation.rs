//! Whole-table operations exposed by the object tree.

use crate::error::{SqlError, SqlResult};
use crate::identifier::object_name;
use crate::model::Object;

/// Dialects whose table mutation syntax the workbench knows.
pub fn can_mutate_table(dialect: &str) -> bool {
    matches!(
        dialect,
        "mysql"
            | "mariadb"
            | "goldendb"
            | "oceanbase"
            | "postgres"
            | "kingbase"
            | "highgo"
            | "vastbase"
            | "opengauss"
            | "gaussdb"
            | "sqlite"
            | "sqlserver"
            | "oracle"
            | "dm"
            | "duckdb"
            | "clickhouse"
    )
}

/// Quotes metadata identifiers and preserves each operation's semantics.
pub fn table_mutation_sql(dialect: &str, object: &Object, action: &str) -> SqlResult<String> {
    if object.kind != "table" && object.kind != "sql" {
        return Err(SqlError::new("此操作仅支持表"));
    }
    if !can_mutate_table(dialect) {
        return Err(SqlError::new("此数据库暂不支持此表操作"));
    }
    let name = object_name(dialect, object)?;
    match action {
        "drop" => Ok(format!("DROP TABLE {name};")),
        "clear" => Ok(format!("DELETE FROM {name};")),
        "truncate" => {
            if dialect == "sqlite" {
                return Err(SqlError::new("SQLite 不支持截断表，请使用清空表"));
            }
            Ok(format!("TRUNCATE TABLE {name};"))
        }
        _ => Err(SqlError::new("未知表操作")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Object {
        Object {
            kind: "table".into(),
            scope: "db".into(),
            name: "a\x60b".into(),
            ..Default::default()
        }
    }

    #[test]
    fn quotes_identifiers_per_dialect() {
        for (dialect, action, want) in [
            ("mysql", "drop", "DROP TABLE \x60db\x60.\x60a\x60\x60b\x60;"),
            ("postgres", "clear", "DELETE FROM \"a\x60b\";"),
            ("sqlserver", "truncate", "TRUNCATE TABLE [a\x60b];"),
            ("sqlite", "clear", "DELETE FROM \"a\x60b\";"),
        ] {
            assert_eq!(
                table_mutation_sql(dialect, &table(), action).unwrap(),
                want,
                "{dialect} {action}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_operations() {
        for (dialect, action, kind, name) in [
            ("sqlite", "truncate", "table", "items"),
            ("redis", "drop", "table", "items"),
            ("mysql", "drop", "view", "items"),
            ("mysql", "drop", "table", ""),
            ("mysql", "unknown", "table", "items"),
        ] {
            let object = Object {
                kind: kind.into(),
                name: name.into(),
                ..Default::default()
            };
            assert!(
                table_mutation_sql(dialect, &object, action).is_err(),
                "{dialect} {action} {kind} {name}"
            );
        }
        assert!(!can_mutate_table("redis"));
        assert!(can_mutate_table("duckdb"));
    }
}
