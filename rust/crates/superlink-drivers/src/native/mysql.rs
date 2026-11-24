//! 进程内 MySQL 协议族驱动（MySQL / MariaDB / OceanBase / TiDB 等）。
//!
//! 走文本协议 + 按列类型解码，避免预处理语句对 DDL 的限制；
//! 行预算与 PostgreSQL 驱动保持一致。

use std::time::Instant;
mod bound;
mod streaming;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use mysql::consts::{ColumnFlags, ColumnType};
use mysql::prelude::Queryable;
use mysql::{ClientIdentity, Opts, OptsBuilder, Row, SslOpts, Value as MyValue};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};

use super::structure::{ColumnEntry, ForeignKeyEntry, IndexEntry, TableEntry, TableStructure};

use crate::session::DriverError;

/// 二进制字符集编号；非该字符集的文本列才按 UTF-8 输出。
const BINARY_CHARSET: u16 = 63;

/// 明文/加密三档，语义与 PostgreSQL 驱动保持一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SslMode {
    Plain,
    Require,
    Verify,
}

fn ssl_mode(config: &ConnectionConfig) -> Result<SslMode, DriverError> {
    let raw = config.ssl_mode.trim().to_ascii_lowercase();
    let raw = if raw.is_empty() {
        if config.use_ssl {
            "require".to_string()
        } else {
            "disable".to_string()
        }
    } else {
        raw
    };
    // 兼容旧版 Go 配置里的 required / disabled / preferred 等写法。
    match raw.as_str() {
        "disable" | "disabled" | "allow" | "prefer" | "preferred" | "false" | "off" | "none" => {
            Ok(SslMode::Plain)
        }
        "require" | "required" | "true" | "on" | "mandatory" | "strict" | "skip-verify"
        | "insecure" => Ok(SslMode::Require),
        "verify-ca" | "verify-full" | "verify_ca" | "verify_full" => Ok(SslMode::Verify),
        other => Err(DriverError::Native(format!("未知的 sslMode：{other}"))),
    }
}

pub struct NativeMysql {
    conn: mysql::Conn,
}

impl NativeMysql {
    pub fn set_cancellation(
        &self,
        cancel: &superlink_infra::QueryCancel,
        config: &ConnectionConfig,
    ) {
        let id = self.conn.connection_id();
        let config = config.clone();
        cancel.on_cancel(move || {
            if let Ok(mut control) = Self::open(&config) {
                let _ = control.conn.query_drop(format!("KILL QUERY {id}"));
            }
        });
    }
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        super::install_crypto_provider();
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let port = if config.port == 0 { 3306 } else { config.port };
        // 0 表示使用默认值；查询超时同时兜住握手挂起的服务端。
        let connect = std::time::Duration::from_secs(if config.timeout == 0 {
            10
        } else {
            u64::from(config.timeout)
        });
        let io = std::time::Duration::from_secs(if config.query_timeout == 0 {
            30
        } else {
            u64::from(config.query_timeout)
        });
        let source = if !config.dsn.is_empty() {
            &config.dsn
        } else {
            &config.uri
        };
        let base =
            if source.is_empty() {
                OptsBuilder::new()
            } else {
                OptsBuilder::from_opts(Opts::from_url(source).map_err(|_| {
                    DriverError::Native("MySQL DSN 应使用 mysql:// URI 格式".into())
                })?)
            };
        let base = if source.is_empty() {
            base.ip_or_hostname(Some(host)).tcp_port(port)
        } else {
            base
        };
        let mut builder = base
            .tcp_connect_timeout(Some(connect))
            .read_timeout(Some(io))
            .write_timeout(Some(io))
            .prefer_socket(false);
        if source.is_empty() && !config.user.trim().is_empty() {
            builder = builder.user(Some(config.user.trim()));
        }
        if source.is_empty() && !config.password.is_empty() {
            builder = builder.pass(Some(config.password.clone()));
        }
        if source.is_empty() && !config.database.trim().is_empty() {
            builder = builder.db_name(Some(config.database.trim()));
        }
        if let Some(ssl) = ssl_opts(config)? {
            builder = builder.ssl_opts(Some(ssl));
        }
        if let Some(charset) = config.extra.get("rustMysqlCharset").and_then(Value::as_str) {
            if !charset
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(DriverError::Native("MySQL 字符集无效".into()));
            }
            builder = builder.init(vec![format!("SET NAMES {charset}")]);
        }
        let opts = Opts::from(builder);
        let conn = mysql::Conn::new(opts)
            .map_err(|err| DriverError::Native(format!("连接 MySQL 失败：{err}")))?;
        Ok(Self { conn })
    }

    pub fn revision(&self) -> &'static str {
        "native-mysql"
    }

    pub fn query(
        &mut self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        if !args.is_empty() {
            return self.query_bound(sql, args, max_rows);
        }
        let started = Instant::now();
        let limit = if max_rows == 0 { u32::MAX } else { max_rows };

        let mut result = self
            .conn
            .query_iter(sql)
            .map_err(|err| DriverError::Native(format!("查询失败：{err}")))?;

        let meta = result.columns();
        let meta = meta.as_ref();
        let columns: Vec<Column> = meta
            .iter()
            .map(|column| Column {
                id: column.name_str().to_string(),
                name: column.name_str().to_string(),
            })
            .collect();
        // 列类型与字符集在消费结果时仍需要，先复制出来避免借用冲突。
        let kinds: Vec<(ColumnType, u16, bool)> = meta
            .iter()
            .map(|column| {
                let unsigned = column.flags().contains(ColumnFlags::UNSIGNED_FLAG);
                (column.column_type(), column.character_set(), unsigned)
            })
            .collect();

        let mut output: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        let mut memory = crate::budget::ResultBudget::default();
        for row in result.by_ref() {
            let row: Row = row.map_err(|err| DriverError::Native(format!("读取行失败：{err}")))?;
            if output.len() as u32 >= limit {
                truncated = true;
                break;
            }
            let mut values = Vec::with_capacity(columns.len());
            for (index, (kind, charset, unsigned)) in kinds.iter().enumerate() {
                values.push(match row.as_ref(index) {
                    Some(value) => decode_value(value, *kind, *charset, *unsigned),
                    None => Value::Null,
                });
            }
            if !memory.accept(&values) {
                truncated = true;
                break;
            }
            output.push(values);
        }
        // QueryResult::affected_rows 在无结果集时读不到 OK 包（返回 0），
        // 影响行数要取连接级的上一个 OK 包。
        let has_columns = !columns.is_empty();
        drop(result);
        let affected = if has_columns {
            0
        } else {
            self.conn.affected_rows()
        };

        Ok(ResultSet {
            columns,
            rows: output,
            messages: Vec::new(),
            rows_affected: affected as i64,
            truncated,
            duration_ns: started.elapsed().as_nanos() as u64,
        })
    }
}

/// require 只加密、不校验证书；verify 使用自定义根证书（若配置）或默认根。
fn ssl_opts(config: &ConnectionConfig) -> Result<Option<SslOpts>, DriverError> {
    let mode = ssl_mode(config)?;
    if mode == SslMode::Plain {
        return Ok(None);
    }
    let require = !config.ssl_cert_path.trim().is_empty() || !config.ssl_key_path.trim().is_empty();
    if require && (config.ssl_cert_path.trim().is_empty() || config.ssl_key_path.trim().is_empty())
    {
        return Err(DriverError::Native("客户端证书与私钥必须同时配置".into()));
    }
    let mut opts = SslOpts::default().with_danger_accept_invalid_certs(mode == SslMode::Require);
    if mode == SslMode::Verify && !config.ssl_ca_path.trim().is_empty() {
        opts = opts.with_root_cert_path(Some(std::path::PathBuf::from(config.ssl_ca_path.trim())));
    }
    if require {
        opts = opts.with_client_identity(Some(ClientIdentity::new(
            std::path::PathBuf::from(config.ssl_cert_path.trim()),
            std::path::PathBuf::from(config.ssl_key_path.trim()),
        )));
    }
    Ok(Some(opts))
}

/// information_schema.tables 的 table_type 归一化。
fn table_kind(raw: &str) -> &'static str {
    if raw.eq_ignore_ascii_case("VIEW") || raw.eq_ignore_ascii_case("SYSTEM VIEW") {
        "view"
    } else {
        "table"
    }
}

impl NativeMysql {
    /// 服务器上的全部数据库（含系统库，由 UI 决定是否展示）。
    pub fn databases(&mut self) -> Result<Vec<String>, DriverError> {
        let rows: Vec<String> = self
            .conn
            .query("SELECT schema_name FROM information_schema.schemata ORDER BY schema_name LIMIT 10000")
            .map_err(|err| DriverError::Native(format!("读取数据库列表失败：{err}")))?;
        Ok(rows)
    }

    /// 指定库的表与视图。
    pub fn tables(&mut self, database: &str) -> Result<Vec<TableEntry>, DriverError> {
        let rows: Vec<(String, String)> = self
            .conn
            .exec(
                "SELECT table_name, table_type FROM information_schema.tables
                 WHERE table_schema = ? ORDER BY table_name LIMIT 10000",
                (database,),
            )
            .map_err(|err| DriverError::Native(format!("读取表列表失败：{err}")))?;
        Ok(rows
            .into_iter()
            .map(|(name, kind)| TableEntry {
                name,
                kind: table_kind(&kind).to_string(),
            })
            .collect())
    }

    /// 列定义：名称、类型、可空、主键、默认值。
    pub fn columns(
        &mut self,
        database: &str,
        table: &str,
    ) -> Result<Vec<ColumnEntry>, DriverError> {
        let rows: Vec<(String, String, String, String, Option<String>, String)> = self
            .conn
            .exec(
                "SELECT column_name, column_type, is_nullable, column_key, column_default, extra
                 FROM information_schema.columns
                 WHERE table_schema = ? AND table_name = ?
                 ORDER BY ordinal_position LIMIT 4096",
                (database, table),
            )
            .map_err(|err| DriverError::Native(format!("读取列失败：{err}")))?;
        Ok(rows
            .into_iter()
            .map(
                |(name, data_type, nullable, key, default_value, extra)| ColumnEntry {
                    name,
                    data_type,
                    nullable: nullable.eq_ignore_ascii_case("YES"),
                    primary_key: key.eq_ignore_ascii_case("PRI"),
                    default_value,
                    extra,
                    comment: None,
                },
            )
            .collect())
    }
}

impl NativeMysql {
    /// 单事务执行多条语句。
    pub fn execute_batch(&mut self, statements: &[String]) -> Result<(), DriverError> {
        let mut transaction = self
            .conn
            .start_transaction(mysql::TxOpts::default())
            .map_err(|err| DriverError::Native(format!("开启事务失败：{err}")))?;
        for statement in statements {
            transaction
                .query_drop(statement.as_str())
                .map_err(|err| DriverError::Native(format!("执行失败：{err}")))?;
        }
        transaction
            .commit()
            .map_err(|err| DriverError::Native(format!("提交失败：{err}")))
    }
}

/// information_schema.statistics 的索引定义。
impl NativeMysql {
    pub fn structure(
        &mut self,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, DriverError> {
        Ok(TableStructure {
            columns: self.columns(database, table)?,
            indexes: self.indexes(database, table)?,
            foreign_keys: self.foreign_keys(database, table)?,
        })
    }

    fn indexes(&mut self, database: &str, table: &str) -> Result<Vec<IndexEntry>, DriverError> {
        let rows: Vec<(String, i64, Option<String>)> = self
            .conn
            .exec(
                "SELECT index_name, non_unique, column_name
                 FROM information_schema.statistics
                 WHERE table_schema = ? AND table_name = ?
                 ORDER BY index_name, seq_in_index LIMIT 65536",
                (database, table),
            )
            .map_err(|err| DriverError::Native(format!("读取索引失败：{err}")))?;
        let mut list: Vec<IndexEntry> = Vec::new();
        for (name, non_unique, column) in rows {
            let unique = non_unique == 0;
            match list.iter_mut().find(|entry| entry.name == name) {
                Some(entry) => {
                    if let Some(column) = column {
                        entry.columns.push(column);
                    }
                }
                None => list.push(IndexEntry {
                    columns: column.into_iter().collect(),
                    name,
                    unique,
                }),
            }
        }
        Ok(list)
    }

    fn foreign_keys(
        &mut self,
        database: &str,
        table: &str,
    ) -> Result<Vec<ForeignKeyEntry>, DriverError> {
        // (约束名, 本地列, 引用表, 引用列)
        type FkRow = (String, Option<String>, Option<String>, Option<String>);
        let rows: Vec<FkRow> = self
            .conn
            .exec(
                "SELECT constraint_name, column_name, referenced_table_name, referenced_column_name
                 FROM information_schema.key_column_usage
                 WHERE table_schema = ? AND table_name = ?
                   AND referenced_table_name IS NOT NULL
                 ORDER BY constraint_name, ordinal_position LIMIT 65536",
                (database, table),
            )
            .map_err(|err| DriverError::Native(format!("读取外键失败：{err}")))?;
        let mut list: Vec<ForeignKeyEntry> = Vec::new();
        for (name, column, referenced_table, referenced_column) in rows {
            match list.iter_mut().find(|entry| entry.name == name) {
                Some(entry) => {
                    if let Some(column) = column {
                        entry.columns.push(column);
                    }
                    if let Some(column) = referenced_column {
                        entry.referenced_columns.push(column);
                    }
                }
                None => list.push(ForeignKeyEntry {
                    columns: column.into_iter().collect(),
                    name,
                    referenced_table: referenced_table.unwrap_or_default(),
                    referenced_columns: referenced_column.into_iter().collect(),
                }),
            }
        }
        Ok(list)
    }
}

/// 文本协议下所有值都是字节，按列类型还原成 JSON 标量。
fn decode_value(value: &MyValue, kind: ColumnType, charset: u16, unsigned: bool) -> Value {
    match value {
        MyValue::NULL => Value::Null,
        MyValue::Int(int) => json!(int),
        MyValue::UInt(uint) => json!(uint),
        MyValue::Float(float) => json!(float),
        MyValue::Double(double) => json!(double),
        MyValue::Date(year, month, day, hour, minute, second, micro) => json!(format_date(
            *year, *month, *day, *hour, *minute, *second, *micro
        )),
        MyValue::Time(negative, days, hours, minutes, seconds, micro) => json!(format_time(
            *negative, *days, *hours, *minutes, *seconds, *micro
        )),
        MyValue::Bytes(bytes) => decode_bytes(bytes, kind, charset, unsigned),
    }
}

fn decode_bytes(bytes: &[u8], kind: ColumnType, charset: u16, unsigned: bool) -> Value {
    match kind {
        ColumnType::MYSQL_TYPE_TINY
        | ColumnType::MYSQL_TYPE_SHORT
        | ColumnType::MYSQL_TYPE_LONG
        | ColumnType::MYSQL_TYPE_INT24
        | ColumnType::MYSQL_TYPE_LONGLONG
        | ColumnType::MYSQL_TYPE_YEAR => decode_integer(bytes, unsigned),
        ColumnType::MYSQL_TYPE_DATE
        | ColumnType::MYSQL_TYPE_DATETIME
        | ColumnType::MYSQL_TYPE_TIMESTAMP => {
            let text = String::from_utf8_lossy(bytes);
            json!(normalize_temporal(&text))
        }
        ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => {
            let text = String::from_utf8_lossy(bytes);
            text.parse::<f64>()
                .map(|value| json!(value))
                .unwrap_or_else(|_| json!(text.as_ref()))
        }
        // BINARY/VARBINARY 也按 VAR_STRING/STRING + 二进制字符集上报，
        // 因此判定依据是字符集而不是列类型；DECIMAL 与日期时间仍是文本。
        ColumnType::MYSQL_TYPE_BIT
        | ColumnType::MYSQL_TYPE_GEOMETRY
        | ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB
        | ColumnType::MYSQL_TYPE_BLOB
        | ColumnType::MYSQL_TYPE_STRING
        | ColumnType::MYSQL_TYPE_VAR_STRING
        | ColumnType::MYSQL_TYPE_VARCHAR => {
            if charset == BINARY_CHARSET {
                json!(BASE64.encode(bytes))
            } else {
                json!(String::from_utf8_lossy(bytes))
            }
        }
        _ => json!(String::from_utf8_lossy(bytes)),
    }
}

/// 文本协议返回 "YYYY-MM-DD HH:MM:SS"，统一成 ISO 的 T 分隔。
fn normalize_temporal(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.len() >= 19 && bytes[10] == b' ' {
        let mut owned = text.to_string();
        owned.replace_range(10..11, "T");
        return owned;
    }
    text.to_string()
}

fn decode_integer(bytes: &[u8], unsigned: bool) -> Value {
    let text = String::from_utf8_lossy(bytes);
    if unsigned {
        text.parse::<u64>()
            .map(|value| json!(value))
            .unwrap_or(Value::Null)
    } else {
        text.parse::<i64>()
            .map(|value| json!(value))
            .unwrap_or(Value::Null)
    }
}

fn format_date(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    micro: u32,
) -> String {
    if hour == 0 && minute == 0 && second == 0 && micro == 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else if micro == 0 {
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{micro:06}")
    }
}

fn format_time(
    negative: bool,
    days: u32,
    hours: u8,
    minutes: u8,
    seconds: u8,
    micro: u32,
) -> String {
    let sign = if negative { "-" } else { "" };
    let total_hours = days * 24 + u32::from(hours);
    if micro == 0 {
        format!("{sign}{total_hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{sign}{total_hours:02}:{minutes:02}:{seconds:02}.{micro:06}")
    }
}

#[cfg(test)]
mod tests;
