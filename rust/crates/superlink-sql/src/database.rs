//! Bounded database creation statements.

use crate::error::{SqlError, SqlResult};
use crate::identifier::quote_identifier;

/// Dialects that can create a database from the workbench.
pub fn can_create_database(dialect: &str) -> bool {
    matches!(
        dialect,
        "mysql"
            | "mariadb"
            | "goldendb"
            | "oceanbase"
            | "diros"
            | "starrocks"
            | "postgres"
            | "opengauss"
            | "gaussdb"
            | "highgo"
            | "kingbase"
            | "sqlserver"
            | "clickhouse"
            | "tdengine"
    )
}

/// True when the dialect accepts CHARACTER SET / COLLATE options.
pub fn database_charset_supported(dialect: &str) -> bool {
    matches!(
        dialect,
        "mysql" | "mariadb" | "goldendb" | "oceanbase" | "diros"
    )
}

pub fn create_database_sql(dialect: &str, name: &str) -> SqlResult<String> {
    if !can_create_database(dialect) {
        return Err(SqlError::new("当前数据源不支持新建库"));
    }
    let name = name.trim();
    if name.is_empty() || name.len() > 128 || name.contains(['\0', '\r', '\n']) {
        return Err(SqlError::new("请输入有效的库名（最多 128 字节）"));
    }
    let quoted = quote_identifier(dialect, name)?;
    Ok(format!("CREATE DATABASE {quoted}"))
}

pub fn create_database_with_options_sql(
    dialect: &str,
    name: &str,
    charset: &str,
    collation: &str,
) -> SqlResult<String> {
    let mut sql = create_database_sql(dialect, name)?;
    if charset.is_empty() && collation.is_empty() {
        return Ok(sql);
    }
    if !database_charset_supported(dialect) {
        return Err(SqlError::new("当前数据源不支持字符集与排序规则选项"));
    }
    for value in [charset, collation] {
        if !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(SqlError::new("无效的字符集或排序规则"));
        }
    }
    if !charset.is_empty() {
        sql.push_str(" CHARACTER SET ");
        sql.push_str(charset);
    }
    if !collation.is_empty() {
        sql.push_str(" COLLATE ");
        sql.push_str(collation);
    }
    Ok(sql)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_literal_names() {
        for (dialect, name, want) in [
            ("mysql", "a\x60b", "CREATE DATABASE \x60a\x60\x60b\x60"),
            ("postgres", "a\"b", "CREATE DATABASE \"a\"\"b\""),
            ("sqlserver", "a]b", "CREATE DATABASE [a]]b]"),
            (
                "mysql",
                "x; DROP DATABASE y",
                "CREATE DATABASE \x60x; DROP DATABASE y\x60",
            ),
        ] {
            assert_eq!(create_database_sql(dialect, name).unwrap(), want);
        }
        for (dialect, name) in [
            ("sqlite", "db"),
            ("oracle", "db"),
            ("mysql", " "),
            ("mysql", "a\0b"),
            ("postgres", "a\nb"),
        ] {
            assert!(
                create_database_sql(dialect, name).is_err(),
                "accepted {dialect} {name:?}"
            );
        }
    }

    #[test]
    fn validates_charset_options() {
        assert_eq!(
            create_database_with_options_sql("mysql", "demo", "utf8mb4", "utf8mb4_unicode_ci")
                .unwrap(),
            "CREATE DATABASE \x60demo\x60 CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci"
        );
        for (dialect, charset, collation) in [
            ("mysql", "utf8; DROP TABLE x", ""),
            ("mysql", "utf8", "x\x60"),
            ("postgres", "utf8", ""),
        ] {
            assert!(create_database_with_options_sql(dialect, "demo", charset, collation).is_err());
        }
    }
}
