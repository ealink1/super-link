//! 进程内 MySQL 协议族驱动（MySQL / MariaDB / OceanBase / TiDB 等）。
//!
//! 走文本协议 + 按列类型解码，避免预处理语句对 DDL 的限制；
//! 行预算与 PostgreSQL 驱动保持一致。

use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use mysql::consts::{ColumnFlags, ColumnType};
use mysql::prelude::Queryable;
use mysql::{ClientIdentity, Opts, OptsBuilder, Row, SslOpts, Value as MyValue};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};

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
    match raw.as_str() {
        "disable" | "allow" | "prefer" | "false" => Ok(SslMode::Plain),
        "require" | "true" => Ok(SslMode::Require),
        "verify-ca" | "verify-full" => Ok(SslMode::Verify),
        other => Err(DriverError::Native(format!("未知的 sslMode：{other}"))),
    }
}

pub struct NativeMysql {
    conn: mysql::Conn,
}

impl NativeMysql {
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let port = if config.port == 0 { 3306 } else { config.port };
        let mut builder = OptsBuilder::new()
            .ip_or_hostname(Some(host))
            .tcp_port(port)
            .tcp_connect_timeout(Some(std::time::Duration::from_secs(10)))
            .prefer_socket(false);
        if !config.user.trim().is_empty() {
            builder = builder.user(Some(config.user.trim()));
        }
        if !config.password.is_empty() {
            builder = builder.pass(Some(config.password.clone()));
        }
        if !config.database.trim().is_empty() {
            builder = builder.db_name(Some(config.database.trim()));
        }
        if let Some(ssl) = ssl_opts(config)? {
            builder = builder.ssl_opts(Some(ssl));
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
            return Err(DriverError::Native("MySQL 原生驱动暂不支持参数".into()));
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
            output.push(values);
        }
        let affected = result.affected_rows();
        drop(result);

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
        ColumnType::MYSQL_TYPE_FLOAT | ColumnType::MYSQL_TYPE_DOUBLE => {
            let text = String::from_utf8_lossy(bytes);
            text.parse::<f64>()
                .map(|value| json!(value))
                .unwrap_or_else(|_| json!(text.as_ref()))
        }
        ColumnType::MYSQL_TYPE_BIT
        | ColumnType::MYSQL_TYPE_GEOMETRY
        | ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB
        | ColumnType::MYSQL_TYPE_BLOB => {
            if charset == BINARY_CHARSET {
                json!(BASE64.encode(bytes))
            } else {
                json!(String::from_utf8_lossy(bytes))
            }
        }
        _ => json!(String::from_utf8_lossy(bytes)),
    }
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
mod tests {
    use super::*;

    fn config(ssl_mode: &str, use_ssl: bool) -> ConnectionConfig {
        ConnectionConfig {
            kind: "mysql".into(),
            ssl_mode: ssl_mode.into(),
            use_ssl,
            ..Default::default()
        }
    }

    #[test]
    fn maps_sslmode_to_a_plan() {
        assert_eq!(ssl_mode(&config("disable", false)).unwrap(), SslMode::Plain);
        assert_eq!(ssl_mode(&config("prefer", false)).unwrap(), SslMode::Plain);
        assert_eq!(
            ssl_mode(&config("require", false)).unwrap(),
            SslMode::Require
        );
        assert_eq!(
            ssl_mode(&config("verify-full", false)).unwrap(),
            SslMode::Verify
        );
        assert_eq!(ssl_mode(&config("", true)).unwrap(), SslMode::Require);
        assert!(ssl_mode(&config("bogus", false)).is_err());
    }

    #[test]
    fn decodes_integers_and_unsigned_values() {
        assert_eq!(
            decode_bytes(b"42", ColumnType::MYSQL_TYPE_LONG, 63, false),
            json!(42)
        );
        assert_eq!(
            decode_bytes(
                b"18446744073709551615",
                ColumnType::MYSQL_TYPE_LONGLONG,
                63,
                true
            ),
            json!(18446744073709551615u64)
        );
        assert_eq!(
            decode_bytes(b"abc", ColumnType::MYSQL_TYPE_LONG, 63, false),
            Value::Null
        );
    }

    #[test]
    fn decodes_floats_and_text() {
        assert_eq!(
            decode_bytes(b"1.5", ColumnType::MYSQL_TYPE_DOUBLE, 63, false),
            json!(1.5)
        );
        assert_eq!(
            decode_bytes(
                "你好".as_bytes(),
                ColumnType::MYSQL_TYPE_VAR_STRING,
                45,
                false
            ),
            json!("你好")
        );
        assert_eq!(
            decode_bytes(
                "12.30".as_bytes(),
                ColumnType::MYSQL_TYPE_NEWDECIMAL,
                45,
                false
            ),
            json!("12.30")
        );
    }

    #[test]
    fn encodes_binary_blobs_as_base64() {
        assert_eq!(
            decode_bytes(&[1, 2], ColumnType::MYSQL_TYPE_BLOB, BINARY_CHARSET, false),
            json!("AQI=")
        );
    }

    /// 需要真实服务：SUPERLINK_TEST_MYSQL=1 + HOST/PORT/USER/PASSWORD/DB 环境变量
    #[test]
    fn queries_a_live_server_when_configured() {
        if std::env::var("SUPERLINK_TEST_MYSQL").is_err() {
            eprintln!("skipping: SUPERLINK_TEST_MYSQL not set");
            return;
        }
        let config = ConnectionConfig {
            kind: "mysql".into(),
            host: std::env::var("SUPERLINK_TEST_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("SUPERLINK_TEST_MYSQL_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(3306),
            user: std::env::var("SUPERLINK_TEST_MYSQL_USER").unwrap_or_else(|_| "root".into()),
            password: std::env::var("SUPERLINK_TEST_MYSQL_PASSWORD").unwrap_or_default(),
            database: std::env::var("SUPERLINK_TEST_MYSQL_DB").unwrap_or_else(|_| "mysql".into()),
            ..Default::default()
        };
        let mut driver = NativeMysql::open(&config).unwrap();
        let result = driver
            .query("SELECT 1 AS one, 'x' AS txt, 1.5 AS f", &[], 10)
            .unwrap();
        assert_eq!(result.columns.len(), 3);
        assert_eq!(result.rows[0][0], json!(1));
        assert_eq!(result.rows[0][1], json!("x"));
        assert_eq!(result.rows[0][2], json!(1.5));
    }

    #[test]
    fn formats_dates_and_times() {
        let value = decode_value(
            &MyValue::Date(2026, 10, 27, 13, 5, 9, 0),
            ColumnType::MYSQL_TYPE_DATETIME,
            45,
            false,
        );
        assert_eq!(value, json!("2026-10-27T13:05:09"));
        let value = decode_value(
            &MyValue::Date(2026, 10, 27, 0, 0, 0, 0),
            ColumnType::MYSQL_TYPE_DATE,
            45,
            false,
        );
        assert_eq!(value, json!("2026-10-27"));
        let value = decode_value(
            &MyValue::Time(false, 1, 2, 3, 4, 0),
            ColumnType::MYSQL_TYPE_TIME,
            45,
            false,
        );
        assert_eq!(value, json!("26:03:04"));
    }
}
