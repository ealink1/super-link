//! 进程内原生驱动。首个移植目标：SQLite。
//!
//! Go agent 覆盖 24 类数据源，但 Rust 端当前只用到 metadata/connect/query/close
//! 四个方法；这个模块从 SQLite 开始把该子集搬到进程内，其余驱动暂时仍走 agent。

use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, params_from_iter};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};

use crate::session::DriverError;

/// SQLite built into the process; no agent binary needed.
pub struct NativeSqlite {
    conn: Connection,
    timeout_ms: u64,
}

impl NativeSqlite {
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let path = config.database.trim();
        if path.is_empty() {
            return Err(DriverError::Native("sqlite 需要数据库文件路径".into()));
        }
        let conn = Connection::open(path)
            .map_err(|err| DriverError::Native(format!("打开 SQLite 失败：{err}")))?;
        Ok(Self {
            conn,
            timeout_ms: u64::from(config.query_timeout).saturating_mul(1000),
        })
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    pub fn revision(&self) -> &'static str {
        "native-sqlite"
    }

    /// 单条语句；多语句与事务会话仍由 agent 承担。
    pub fn query(
        &self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        let started = Instant::now();
        let params = to_sql_params(args)?;
        let mut statement = self
            .conn
            .prepare(sql)
            .map_err(|err| DriverError::Native(format!("SQL 准备失败：{err}")))?;

        if statement.column_count() == 0 {
            let affected = statement
                .execute(params_from_iter(params))
                .map_err(|err| DriverError::Native(format!("执行失败：{err}")))?;
            return Ok(ResultSet {
                columns: Vec::new(),
                rows: Vec::new(),
                messages: Vec::new(),
                rows_affected: affected as i64,
                truncated: false,
                duration_ns: started.elapsed().as_nanos() as u64,
            });
        }

        let columns: Vec<Column> = statement
            .column_names()
            .iter()
            .map(|name| Column {
                id: name.to_string(),
                name: name.to_string(),
            })
            .collect();

        let limit = if max_rows == 0 { u32::MAX } else { max_rows };
        let mut rows_out: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        let mut rows = statement
            .query(params_from_iter(params))
            .map_err(|err| DriverError::Native(format!("查询失败：{err}")))?;
        while let Some(row) = rows
            .next()
            .map_err(|err| DriverError::Native(format!("读取行失败：{err}")))?
        {
            if rows_out.len() as u32 >= limit {
                truncated = true;
                break;
            }
            let mut values = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                let raw = row
                    .get_ref(index)
                    .map_err(|err| DriverError::Native(format!("读取列失败：{err}")))?;
                values.push(to_json(raw));
            }
            rows_out.push(values);
        }

        Ok(ResultSet {
            columns,
            rows: rows_out,
            truncated,
            duration_ns: started.elapsed().as_nanos() as u64,
            ..Default::default()
        })
    }
}

/// 结构浏览用的表条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableEntry {
    pub name: String,
    pub kind: String,
}

/// 结构浏览用的列条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnEntry {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub default_value: Option<String>,
}

impl NativeSqlite {
    /// 只列用户对象，跳过 sqlite_ 内部表。
    pub fn tables(&self) -> Result<Vec<TableEntry>, DriverError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT name, type FROM sqlite_master
                 WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .map_err(|err| DriverError::Native(format!("读取表列表失败：{err}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(TableEntry {
                    name: row.get(0)?,
                    kind: row.get(1)?,
                })
            })
            .map_err(|err| DriverError::Native(format!("读取表列表失败：{err}")))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|err| DriverError::Native(format!("读取表名失败：{err}")))?);
        }
        Ok(entries)
    }

    /// PRAGMA table_info：cid, name, type, notnull, dflt_value, pk。
    pub fn columns(&self, table: &str) -> Result<Vec<ColumnEntry>, DriverError> {
        let quoted = table.replace('"', "\"\"");
        let mut statement = self
            .conn
            .prepare(&format!("PRAGMA table_info(\"{quoted}\")"))
            .map_err(|err| DriverError::Native(format!("读取列失败：{err}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(ColumnEntry {
                    name: row.get(1)?,
                    data_type: row.get::<_, String>(2).unwrap_or_default(),
                    nullable: row.get::<_, i64>(3).unwrap_or(0) == 0,
                    primary_key: row.get::<_, i64>(5).unwrap_or(0) > 0,
                    default_value: row.get(4)?,
                })
            })
            .map_err(|err| DriverError::Native(format!("读取列失败：{err}")))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|err| DriverError::Native(format!("读取列定义失败：{err}")))?);
        }
        Ok(entries)
    }
}

/// JSON 参数只支持标量；BLOB 用 base64 与 Go 侧保持一致。
fn to_sql_params(args: &[Value]) -> Result<Vec<SqlValue>, DriverError> {
    args.iter()
        .map(|value| match value {
            Value::Null => Ok(SqlValue::Null),
            Value::Bool(flag) => Ok(SqlValue::Integer(i64::from(*flag))),
            Value::Number(number) => number
                .as_i64()
                .map(SqlValue::Integer)
                .or_else(|| number.as_f64().map(SqlValue::Real))
                .ok_or_else(|| DriverError::Native("不支持的数值参数".into())),
            Value::String(text) => Ok(SqlValue::Text(text.clone())),
            _ => Err(DriverError::Native("参数只支持标量".into())),
        })
        .collect()
}

fn to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => json!(number),
        ValueRef::Text(text) => json!(String::from_utf8_lossy(text)),
        ValueRef::Blob(bytes) => json!(BASE64.encode(bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config() -> (ConnectionConfig, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("superlink-native-{}.db", uuid::Uuid::new_v4()));
        (
            ConnectionConfig {
                kind: "sqlite".into(),
                database: path.to_string_lossy().into_owned(),
                ..Default::default()
            },
            path,
        )
    }

    #[test]
    fn runs_statements_and_reads_rows() {
        let (config, path) = temp_config();
        let driver = NativeSqlite::open(&config).unwrap();
        driver
            .query("CREATE TABLE t(id INTEGER, name TEXT)", &[], 0)
            .unwrap();
        let changed = driver
            .query("INSERT INTO t VALUES (1, 'a'), (2, 'b')", &[], 0)
            .unwrap();
        assert_eq!(changed.rows_affected, 2);

        let result = driver
            .query("SELECT id, name FROM t ORDER BY id", &[], 10)
            .unwrap();
        assert_eq!(result.columns.len(), 2);
        assert_eq!(
            result.rows,
            vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")],]
        );
        assert!(!result.truncated);
        drop(driver);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enforces_the_row_budget() {
        let (config, path) = temp_config();
        let driver = NativeSqlite::open(&config).unwrap();
        driver.query("CREATE TABLE t(x INTEGER)", &[], 0).unwrap();
        for value in 0..5 {
            driver
                .query("INSERT INTO t VALUES (?)", &[json!(value)], 0)
                .unwrap();
        }
        let result = driver.query("SELECT x FROM t", &[], 2).unwrap();
        assert_eq!(result.rows.len(), 2);
        assert!(result.truncated);
        drop(driver);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn encodes_blobs_as_base64() {
        let (config, path) = temp_config();
        let driver = NativeSqlite::open(&config).unwrap();
        let result = driver
            .query("SELECT CAST(x'0102' AS BLOB)", &[], 1)
            .unwrap();
        assert_eq!(result.rows[0][0], json!("AQI="));
        drop(driver);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reports_sql_errors_instead_of_panicking() {
        let (config, path) = temp_config();
        let driver = NativeSqlite::open(&config).unwrap();
        let error = driver.query("SELECT * FROM missing", &[], 0).unwrap_err();
        assert!(error.to_string().contains("missing"));
        drop(driver);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lists_tables_and_columns() {
        let (config, path) = temp_config();
        let driver = NativeSqlite::open(&config).unwrap();
        driver
            .query("CREATE TABLE alpha(id INTEGER PRIMARY KEY, name TEXT NOT NULL, score REAL DEFAULT 0)", &[], 0)
            .unwrap();
        driver
            .query("CREATE VIEW beta AS SELECT id FROM alpha", &[], 0)
            .unwrap();

        let tables = driver.tables().unwrap();
        assert_eq!(
            tables,
            vec![
                TableEntry {
                    name: "alpha".into(),
                    kind: "table".into()
                },
                TableEntry {
                    name: "beta".into(),
                    kind: "view".into()
                },
            ]
        );

        let columns = driver.columns("alpha").unwrap();
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].name, "id");
        assert!(columns[0].primary_key);
        assert_eq!(columns[1].name, "name");
        assert!(!columns[1].nullable, "NOT NULL 列不可空");
        assert_eq!(columns[2].default_value.as_deref(), Some("0"));
        drop(driver);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn requires_a_database_path() {
        let config = ConnectionConfig {
            kind: "sqlite".into(),
            ..Default::default()
        };
        assert!(NativeSqlite::open(&config).is_err());
    }
}
