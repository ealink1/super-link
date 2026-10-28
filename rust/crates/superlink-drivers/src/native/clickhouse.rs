//! 进程内 ClickHouse 驱动（HTTP 接口 + JSONCompact）。
//!
//! 行预算交给服务端设置（max_result_rows + result_overflow_mode=break），
//! 再按返回行数判断是否截断，避免把大结果整包拉回客户端。

use std::time::Instant;

use serde_json::Value;
use superlink_domain::{Column, ConnectionConfig, ResultSet};

use crate::session::DriverError;

pub struct NativeClickHouse {
    endpoint: String,
    user: String,
    password: String,
    database: String,
}

impl NativeClickHouse {
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let port = if config.port == 0 { 8123 } else { config.port };
        let scheme = if config.use_ssl
            || matches!(
                config.ssl_mode.to_ascii_lowercase().as_str(),
                "require" | "verify-ca" | "verify-full"
            ) {
            "https"
        } else {
            "http"
        };
        Ok(Self {
            endpoint: format!("{scheme}://{host}:{port}/"),
            user: config.user.trim().to_string(),
            password: config.password.clone(),
            database: config.database.trim().to_string(),
        })
    }

    pub fn revision(&self) -> &'static str {
        "native-clickhouse"
    }

    pub fn query(
        &mut self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        if !args.is_empty() {
            return Err(DriverError::Native(
                "ClickHouse 原生驱动暂不支持参数".into(),
            ));
        }
        let started = Instant::now();
        let limit = if max_rows == 0 { u32::MAX } else { max_rows };
        // 多取一行用于判断是否被服务端截断。
        let probe = u64::from(limit.saturating_add(1));
        let mut request = ureq::post(&self.endpoint)
            .header("X-ClickHouse-User", &self.user)
            .header("X-ClickHouse-Key", &self.password)
            .query("default_format", "JSONCompact")
            .query("max_result_rows", probe.to_string())
            .query("result_overflow_mode", "break");
        if !self.database.is_empty() {
            request = request.header("X-ClickHouse-Database", &self.database);
        }
        let mut response = request
            .send(sql)
            .map_err(|err| DriverError::Native(format!("ClickHouse 请求失败：{err}")))?;
        let body = response
            .body_mut()
            .with_config()
            .limit(64 << 20)
            .read_to_string()
            .map_err(|err| DriverError::Native(format!("读取响应失败：{err}")))?;
        let payload: Value = serde_json::from_str(&body)
            .map_err(|err| DriverError::Native(format!("响应不是 JSON：{err}")))?;

        let mut result = to_result_set(&payload, limit)?;
        result.duration_ns = started.elapsed().as_nanos() as u64;
        Ok(result)
    }
}

/// 解析 JSONCompact：meta[].name 决定列名，data 是二维数组。
pub(crate) fn to_result_set(payload: &Value, limit: u32) -> Result<ResultSet, DriverError> {
    let meta = payload
        .get("meta")
        .and_then(Value::as_array)
        .ok_or_else(|| DriverError::Native("响应缺少 meta".into()))?;
    let columns: Vec<Column> = meta
        .iter()
        .filter_map(|item| item.get("name").and_then(Value::as_str))
        .map(|name| Column {
            id: name.to_string(),
            name: name.to_string(),
        })
        .collect();
    let data = payload
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut truncated = false;
    let mut rows = Vec::with_capacity(data.len().min(limit as usize));
    for row in data {
        if rows.len() as u32 >= limit {
            truncated = true;
            break;
        }
        match row {
            Value::Array(values) => rows.push(values),
            _ => return Err(DriverError::Native("行数据不是数组".into())),
        }
    }
    let affected = payload
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or(rows.len() as u64);
    Ok(ResultSet {
        columns,
        rows,
        messages: Vec::new(),
        rows_affected: if affected <= limit as u64 {
            affected as i64
        } else {
            0
        },
        truncated,
        duration_ns: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "meta": [
                {"name": "id", "type": "UInt64"},
                {"name": "name", "type": "String"},
                {"name": "score", "type": "Float64"},
                {"name": "amount", "type": "Decimal(10,2)"}
            ],
            "data": [
                [1, "甲", 1.5, "12.30"],
                [2, "乙", 2.5, "0.50"]
            ],
            "rows": 2
        })
    }

    #[test]
    fn maps_json_compact_to_result_set() {
        let result = to_result_set(&payload(), 10).unwrap();
        assert_eq!(result.columns.len(), 4);
        assert_eq!(result.columns[1].name, "name");
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][0], json!(1));
        assert_eq!(result.rows[0][1], json!("甲"));
        assert_eq!(result.rows[1][3], json!("0.50"));
        assert!(!result.truncated);
    }

    #[test]
    fn marks_truncation_when_data_exceeds_the_budget() {
        let result = to_result_set(&payload(), 1).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert!(result.truncated);
        assert_eq!(result.rows_affected, 0);
    }

    #[test]
    fn rejects_payloads_without_meta() {
        assert!(to_result_set(&json!({"data": []}), 10).is_err());
    }

    #[test]
    fn builds_http_endpoints() {
        let config = ConnectionConfig {
            kind: "clickhouse".into(),
            host: "ch.local".into(),
            port: 8123,
            ..Default::default()
        };
        let driver = NativeClickHouse::open(&config).unwrap();
        assert_eq!(driver.endpoint, "http://ch.local:8123/");

        let tls = ConnectionConfig {
            kind: "clickhouse".into(),
            host: "ch.local".into(),
            port: 8443,
            ssl_mode: "require".into(),
            ..Default::default()
        };
        assert_eq!(
            NativeClickHouse::open(&tls).unwrap().endpoint,
            "https://ch.local:8443/"
        );
    }

    /// 需要真实服务：SUPERLINK_TEST_CLICKHOUSE=1 + HOST/PORT/USER/PASSWORD/DB
    #[test]
    fn queries_a_live_server_when_configured() {
        if std::env::var("SUPERLINK_TEST_CLICKHOUSE").is_err() {
            eprintln!("skipping: SUPERLINK_TEST_CLICKHOUSE not set");
            return;
        }
        let config = ConnectionConfig {
            kind: "clickhouse".into(),
            host: std::env::var("SUPERLINK_TEST_CLICKHOUSE_HOST")
                .unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("SUPERLINK_TEST_CLICKHOUSE_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8123),
            user: std::env::var("SUPERLINK_TEST_CLICKHOUSE_USER")
                .unwrap_or_else(|_| "default".into()),
            password: std::env::var("SUPERLINK_TEST_CLICKHOUSE_PASSWORD").unwrap_or_default(),
            database: std::env::var("SUPERLINK_TEST_CLICKHOUSE_DB")
                .unwrap_or_else(|_| "default".into()),
            ..Default::default()
        };
        let mut driver = NativeClickHouse::open(&config).unwrap();
        let result = driver.query("SELECT 1 AS x, 'a' AS s", &[], 10).unwrap();
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.rows[0][0], json!(1));
        assert_eq!(result.rows[0][1], json!("a"));

        let limited = driver
            .query("SELECT number FROM numbers(10)", &[], 3)
            .unwrap();
        assert_eq!(limited.rows.len(), 3);
        assert!(limited.truncated);
    }

    #[test]
    fn defaults_to_8123() {
        let config = ConnectionConfig {
            kind: "clickhouse".into(),
            ..Default::default()
        };
        assert!(
            NativeClickHouse::open(&config)
                .unwrap()
                .endpoint
                .ends_with(":8123/")
        );
    }
}
