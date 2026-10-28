//! 进程内 PostgreSQL 驱动。
//!
//! 覆盖 Rust 端当前用到的 query 子集：类型化取值、行预算、耗时统计。
//! TLS 连接（sslmode=require 及以上）暂不支持，调用方应回退到 agent。

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use fallible_iterator::FallibleIterator;
use postgres::types::{FromSql, Type, WrongType};
use postgres::{Client, Config, NoTls, Row};
use serde_json::{Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};

use crate::session::DriverError;

pub struct NativePostgres {
    client: Client,
}

impl NativePostgres {
    /// 只有不需要 TLS 的连接才由本驱动接管。
    pub fn supports(config: &ConnectionConfig) -> bool {
        if config.use_ssl {
            return false;
        }
        !matches!(
            config.ssl_mode.to_ascii_lowercase().as_str(),
            "require" | "verify-ca" | "verify-full"
        )
    }

    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let mut params = Config::new();
        params
            .host(host)
            .port(if config.port == 0 { 5432 } else { config.port })
            .application_name("SuperLink");
        if !config.user.trim().is_empty() {
            params.user(config.user.trim());
        }
        if !config.password.is_empty() {
            params.password(&config.password);
        }
        if !config.database.trim().is_empty() {
            params.dbname(config.database.trim());
        }
        params.connect_timeout(Duration::from_secs(10));
        let client = params
            .connect(NoTls)
            .map_err(|err| DriverError::Native(format!("连接 PostgreSQL 失败：{err}")))?;
        Ok(Self { client })
    }

    pub fn revision(&self) -> &'static str {
        "native-postgres"
    }

    pub fn query(
        &mut self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        let started = Instant::now();
        let params = to_params(args)?;
        let limit = if max_rows == 0 { u32::MAX } else { max_rows };

        let statement = self
            .client
            .prepare(sql)
            .map_err(|err| DriverError::Native(format!("语句准备失败：{err}")))?;
        let columns: Vec<Column> = statement
            .columns()
            .iter()
            .map(|column| Column {
                id: column.name().to_string(),
                name: column.name().to_string(),
            })
            .collect();

        let mut rows = self
            .client
            .query_raw(&statement, params.iter().map(|value| value.as_ref()))
            .map_err(|err| DriverError::Native(format!("查询失败：{err}")))?;

        let mut output: Vec<Vec<Value>> = Vec::new();
        let mut truncated = false;
        loop {
            match rows.next() {
                Ok(Some(row)) => {
                    if output.len() as u32 >= limit {
                        truncated = true;
                        break;
                    }
                    let mut values = Vec::with_capacity(columns.len());
                    for index in 0..columns.len() {
                        values.push(to_json(&row, index)?);
                    }
                    output.push(values);
                }
                Ok(None) => break,
                Err(err) => return Err(DriverError::Native(format!("读取行失败：{err}"))),
            }
        }
        let affected = rows.rows_affected().unwrap_or(0) as i64;
        drop(rows);

        Ok(ResultSet {
            columns,
            rows: output,
            messages: Vec::new(),
            rows_affected: affected,
            truncated,
            duration_ns: started.elapsed().as_nanos() as u64,
        })
    }
}

fn wrong(err: postgres::Error) -> DriverError {
    DriverError::Native(format!("类型转换失败：{err}"))
}

/// 标量参数；复杂类型留给 agent。
fn to_params(args: &[Value]) -> Result<Vec<Box<dyn postgres::types::ToSql + Sync>>, DriverError> {
    args.iter()
        .map(
            |value| -> Result<Box<dyn postgres::types::ToSql + Sync>, DriverError> {
                Ok(match value {
                    Value::Null => Box::new(Option::<String>::None),
                    Value::Bool(flag) => Box::new(*flag),
                    Value::Number(number) => {
                        if let Some(int) = number.as_i64() {
                            Box::new(int)
                        } else {
                            Box::new(number.as_f64().unwrap_or_default())
                        }
                    }
                    Value::String(text) => Box::new(text.clone()),
                    _ => return Err(DriverError::Native("参数只支持标量".into())),
                })
            },
        )
        .collect()
}

fn to_json(row: &Row, index: usize) -> Result<Value, DriverError> {
    let kind = row.columns()[index].type_().clone();
    match kind {
        Type::BOOL => Ok(json!(row.try_get::<_, Option<bool>>(index).map_err(wrong)?)),
        Type::INT2 => Ok(json!(row.try_get::<_, Option<i16>>(index).map_err(wrong)?)),
        Type::INT4 => Ok(json!(row.try_get::<_, Option<i32>>(index).map_err(wrong)?)),
        Type::INT8 => Ok(json!(row.try_get::<_, Option<i64>>(index).map_err(wrong)?)),
        Type::FLOAT4 => Ok(json!(row.try_get::<_, Option<f32>>(index).map_err(wrong)?)),
        Type::FLOAT8 => Ok(json!(row.try_get::<_, Option<f64>>(index).map_err(wrong)?)),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::CHAR => Ok(json!(
            row.try_get::<_, Option<String>>(index).map_err(wrong)?
        )),
        Type::BYTEA => Ok(json!(
            row.try_get::<_, Option<Vec<u8>>>(index)
                .map_err(wrong)?
                .map(|bytes| BASE64.encode(bytes))
        )),
        Type::JSON | Type::JSONB => Ok(row
            .try_get::<_, Option<Value>>(index)
            .map_err(wrong)?
            .unwrap_or(Value::Null)),
        Type::DATE => Ok(json!(
            row.try_get::<_, Option<chrono::NaiveDate>>(index)
                .map_err(wrong)?
                .map(|value| value.format("%Y-%m-%d").to_string())
        )),
        Type::TIMESTAMP => Ok(json!(
            row.try_get::<_, Option<chrono::NaiveDateTime>>(index)
                .map_err(wrong)?
                .map(|value| value.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
        )),
        Type::TIMESTAMPTZ => Ok(json!(
            row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(index)
                .map_err(wrong)?
                .map(|value| value.to_rfc3339())
        )),
        Type::TIME => Ok(json!(
            row.try_get::<_, Option<chrono::NaiveTime>>(index)
                .map_err(wrong)?
                .map(|value| value.format("%H:%M:%S%.f").to_string())
        )),
        Type::NUMERIC => Ok(json!(
            row.try_get::<_, Option<Numeric>>(index)
                .map_err(wrong)?
                .map(|value| value.0)
        )),
        Type::UUID => Ok(json!(
            row.try_get::<_, Option<PgUuid>>(index)
                .map_err(wrong)?
                .map(|value| value.0)
        )),
        _ => Err(DriverError::Native(format!(
            "暂不支持的 PostgreSQL 类型：{}",
            kind.name()
        ))),
    }
}

/// PostgreSQL NUMERIC 的二进制解码：base-10000 数字 + 标度。
///
/// 布局：int16 ndigits, int16 weight, uint16 sign, uint16 dscale, ndigits × int16。
pub struct Numeric(pub String);

impl FromSql<'_> for Numeric {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty != Type::NUMERIC {
            return Err(Box::new(WrongType::new::<Numeric>(ty.clone())));
        }
        Ok(Numeric(decode_numeric(raw)?))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::NUMERIC
    }
}

fn decode_numeric(raw: &[u8]) -> Result<String, String> {
    if raw.len() < 8 {
        return Err("numeric 帧过短".into());
    }
    let ndigits = i16::from_be_bytes([raw[0], raw[1]]) as usize;
    let weight = i16::from_be_bytes([raw[2], raw[3]]);
    let sign = u16::from_be_bytes([raw[4], raw[5]]);
    let dscale = u16::from_be_bytes([raw[6], raw[7]]) as usize;
    match sign {
        0xC000 => return Ok("NaN".into()),
        0xD000 => return Ok("Infinity".into()),
        0xF000 => return Ok("-Infinity".into()),
        _ => {}
    }
    if raw.len() < 8 + ndigits * 2 {
        return Err("numeric 位数与帧长不符".into());
    }
    let digits: Vec<i16> = (0..ndigits)
        .map(|index| i16::from_be_bytes([raw[8 + index * 2], raw[9 + index * 2]]))
        .collect();

    let mut integer = String::new();
    if weight >= 0 {
        for position in 0..=weight as usize {
            let digit = digits.get(position).copied().unwrap_or(0);
            if position == 0 {
                integer.push_str(&digit.to_string());
            } else {
                integer.push_str(&format!("{digit:04}"));
            }
        }
    } else {
        integer.push('0');
    }
    while integer.len() > 1 && integer.starts_with('0') {
        integer.remove(0);
    }

    let mut fraction = String::new();
    if dscale > 0 {
        let mut position = weight + 1;
        while fraction.len() < dscale {
            let digit = if position >= 0 {
                digits.get(position as usize).copied().unwrap_or(0)
            } else {
                0
            };
            fraction.push_str(&format!("{digit:04}"));
            position += 1;
        }
        fraction.truncate(dscale);
    }

    let negative = sign == 0x4000;
    let mut output = String::new();
    if negative && !(integer == "0" && fraction.chars().all(|ch| ch == '0')) {
        output.push('-');
    }
    output.push_str(&integer);
    if !fraction.is_empty() {
        output.push('.');
        output.push_str(&fraction);
    }
    Ok(output)
}

/// UUID 的二进制形态是 16 字节，直接转成带连字符的十六进制。
pub struct PgUuid(pub String);

impl FromSql<'_> for PgUuid {
    fn from_sql(ty: &Type, raw: &[u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if *ty != Type::UUID {
            return Err(Box::new(WrongType::new::<PgUuid>(ty.clone())));
        }
        if raw.len() != 16 {
            return Err("uuid 长度不是 16 字节".into());
        }
        let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(PgUuid(format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::UUID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(ssl_mode: &str, use_ssl: bool) -> ConnectionConfig {
        ConnectionConfig {
            kind: "postgresql".into(),
            ssl_mode: ssl_mode.into(),
            use_ssl,
            ..Default::default()
        }
    }

    #[test]
    fn only_plaintext_connections_are_supported() {
        assert!(NativePostgres::supports(&config("disable", false)));
        assert!(NativePostgres::supports(&config("prefer", false)));
        assert!(!NativePostgres::supports(&config("require", false)));
        assert!(!NativePostgres::supports(&config("verify-full", false)));
        assert!(!NativePostgres::supports(&config("disable", true)));
    }

    #[test]
    fn decodes_numeric_values() {
        // 123.45 → ndigits=2, weight=0, sign=0, dscale=2, digits=[123, 4500]
        let raw = [0, 2, 0, 0, 0, 0, 0, 2, 0, 123, 0x11, 0x94];
        assert_eq!(decode_numeric(&raw).unwrap(), "123.45");
        // -0.5 → ndigits=1, weight=-1, sign=0x4000, dscale=1, digits=[5000]
        let raw = [0, 1, 0xFF, 0xFF, 0x40, 0x00, 0, 1, 0x13, 0x88];
        assert_eq!(decode_numeric(&raw).unwrap(), "-0.5");
        // 42 → ndigits=1, weight=0, dscale=0, digits=[42]
        let raw = [0, 1, 0, 0, 0, 0, 0, 0, 0, 42];
        assert_eq!(decode_numeric(&raw).unwrap(), "42");
        // NaN
        let raw = [0, 0, 0, 0, 0xC0, 0x00, 0, 0];
        assert_eq!(decode_numeric(&raw).unwrap(), "NaN");
    }

    /// 需要真实服务：SUPERLINK_TEST_PG_DSN=host=127.0.0.1 port=5432 user=postgres dbname=postgres
    #[test]
    fn queries_a_live_server_when_configured() {
        let Ok(dsn) = std::env::var("SUPERLINK_TEST_PG_DSN") else {
            eprintln!("skipping: SUPERLINK_TEST_PG_DSN not set");
            return;
        };
        let _ = dsn;
        let config = ConnectionConfig {
            kind: "postgresql".into(),
            host: std::env::var("SUPERLINK_TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("SUPERLINK_TEST_PG_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5432),
            user: std::env::var("SUPERLINK_TEST_PG_USER").unwrap_or_else(|_| "postgres".into()),
            password: std::env::var("SUPERLINK_TEST_PG_PASSWORD").unwrap_or_default(),
            database: std::env::var("SUPERLINK_TEST_PG_DB").unwrap_or_else(|_| "postgres".into()),
            ..Default::default()
        };
        let mut driver = NativePostgres::open(&config).unwrap();
        let result = driver
            .query(
                "SELECT 1::int4 AS i, 2.5::numeric AS n, 'x'::text AS t, true AS b",
                &[],
                10,
            )
            .unwrap();
        assert_eq!(result.columns.len(), 4);
        assert_eq!(result.rows[0][0], json!(1));
        assert_eq!(result.rows[0][1], json!("2.5"));
        assert_eq!(result.rows[0][2], json!("x"));
        assert_eq!(result.rows[0][3], json!(true));
    }

    #[test]
    fn formats_uuid_bytes() {
        let raw = [
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88,
        ];
        let value = PgUuid::from_sql(&Type::UUID, &raw).unwrap();
        assert_eq!(value.0, "12345678-9abc-def0-1122-334455667788");
    }
}
