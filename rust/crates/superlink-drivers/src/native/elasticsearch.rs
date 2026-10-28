//! 进程内 Elasticsearch 驱动（子集实现）。
//!
//! 只移植 DevTools 常用路径：JSON DSL 或 `METHOD /index/_search` 单请求，
//! 端点白名单限定 `_search`/`_count`，其余（msearch、SQL 兼容、批量）
//! 仍由 Go agent 承担。响应解析与 Go 侧保持同一形状：
//! `_index`/`_id`/`_score` + 扁平化 `_source` + `_source` 原串 + `_aggregations`。

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Map, Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};

use crate::session::DriverError;

const MAX_SOURCE_BYTES: usize = 32 << 20;
const META_FIELDS: [&str; 4] = ["_index", "_id", "_score", "_aggregations"];

pub struct NativeElasticsearch {
    endpoint: String,
    auth: Option<String>,
    /// 连接里配置的索引；裸 JSON DSL 默认打到它。
    index: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EsRequest {
    pub method: String,
    pub path: String,
    pub body: String,
}

impl NativeElasticsearch {
    pub fn open(config: &ConnectionConfig) -> Result<Self, DriverError> {
        let host = if config.host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.host.trim()
        };
        let port = if config.port == 0 { 9200 } else { config.port };
        let tls = config.use_ssl
            || matches!(
                config.ssl_mode.to_ascii_lowercase().as_str(),
                "require" | "verify-ca" | "verify-full"
            );
        let scheme = if tls { "https" } else { "http" };
        let auth = if config.user.trim().is_empty() {
            None
        } else {
            Some(format!(
                "Basic {}",
                BASE64.encode(format!("{}:{}", config.user.trim(), config.password))
            ))
        };
        Ok(Self {
            endpoint: format!("{scheme}://{host}:{port}"),
            auth,
            index: config.database.trim().to_string(),
        })
    }

    pub fn revision(&self) -> &'static str {
        "native-elasticsearch"
    }

    pub fn query(
        &mut self,
        source: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        if !args.is_empty() {
            return Err(DriverError::Native("Elasticsearch 不支持 SQL 参数".into()));
        }
        let request = parse_request(source, &self.index)?;
        let started = Instant::now();
        // 丢弃查询串：避免通过 ?size= 绕过行预算。
        let path = request.path.split('?').next().unwrap_or(&request.path);
        let url = format!("{}{}", self.endpoint, path);
        // ES 对 _search/_count 同时接受 GET/POST；统一用 POST 以携带请求体。
        debug_assert!(matches!(request.method.as_str(), "GET" | "POST"));
        let mut builder = ureq::post(&url).header("Content-Type", "application/json");
        if let Some(auth) = &self.auth {
            builder = builder.header("Authorization", auth);
        }
        let mut response = builder
            .send(request.body.as_str())
            .map_err(|err| DriverError::Native(format!("Elasticsearch 请求失败：{err}")))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(64 << 20)
            .read_to_string()
            .map_err(|err| DriverError::Native(format!("读取响应失败：{err}")))?;
        if !(200..300).contains(&status) {
            return Err(DriverError::Native(format!(
                "Elasticsearch 返回 HTTP {status}：{}",
                snippet(&body)
            )));
        }
        let payload: Value = serde_json::from_str(&body)
            .map_err(|err| DriverError::Native(format!("响应不是 JSON：{err}")))?;
        let mut result = if request.path.ends_with("/_count") {
            count_result_set(&payload)
        } else {
            search_result_set(&payload, max_rows)
        }?;
        result.duration_ns = started.elapsed().as_nanos() as u64;
        Ok(result)
    }
}

fn snippet(body: &str) -> String {
    let text: String = body.chars().take(300).collect();
    if body.chars().count() > 300 {
        format!("{text}…")
    } else {
        text
    }
}

/// 解析 DevTools 风格输入；单请求，白名单端点。
pub(crate) fn parse_request(source: &str, default_index: &str) -> Result<EsRequest, DriverError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(DriverError::Native("请求体超过 32 MiB".into()));
    }
    let stripped = strip_full_line_comments(source);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return Err(DriverError::Native("请求为空".into()));
    }
    if trimmed.starts_with('{') {
        let path = if default_index.trim().is_empty() {
            "/_search".to_string()
        } else {
            format!("/{}/_search", default_index.trim())
        };
        return Ok(EsRequest {
            method: "POST".into(),
            path,
            body: trimmed.to_string(),
        });
    }
    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or_default().trim();
    let mut parts = first.splitn(3, char::is_whitespace);
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let path = parts.next().unwrap_or_default().trim().to_string();
    if !matches!(method.as_str(), "GET" | "POST") {
        return Err(DriverError::Native(format!(
            "仅支持 GET/POST 只读请求，收到 {method}"
        )));
    }
    if !path.starts_with('/') {
        return Err(DriverError::Native(format!("路径必须以 / 开头：{path}")));
    }
    let tail = parts.next().unwrap_or_default().trim();
    let body = if tail.is_empty() {
        lines.collect::<Vec<_>>().join("\n").trim().to_string()
    } else {
        format!("{tail}\n{}", lines.collect::<Vec<_>>().join("\n"))
            .trim()
            .to_string()
    };
    ensure_read_only(&path)?;
    Ok(EsRequest { method, path, body })
}

fn strip_full_line_comments(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 只放行 _search / _count；路径中的索引名不允许出现额外层级。
fn ensure_read_only(path: &str) -> Result<(), DriverError> {
    let clean = path.split('?').next().unwrap_or(path);
    let clean = clean.trim_end_matches('/');
    let segments: Vec<&str> = clean.split('/').filter(|part| !part.is_empty()).collect();
    let allowed = match segments.as_slice() {
        ["_search"] | ["_count"] => true,
        [index, endpoint] if !index.contains('*') || *index == "*" => {
            matches!(*endpoint, "_search" | "_count")
        }
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(DriverError::Native(format!(
            "原生驱动只支持 _search/_count 只读端点，{path} 请使用 agent"
        )))
    }
}

fn count_result_set(payload: &Value) -> Result<ResultSet, DriverError> {
    let count = payload
        .get("count")
        .cloned()
        .ok_or_else(|| DriverError::Native("响应缺少 count 字段".into()))?;
    Ok(ResultSet {
        columns: vec![Column {
            id: "count".into(),
            name: "count".into(),
        }],
        rows: vec![vec![count]],
        messages: Vec::new(),
        rows_affected: 0,
        truncated: false,
        duration_ns: 0,
    })
}

/// 与 Go 侧 parseSearchResponseJSON 对齐：hits 展开、aggregations 附首行、列名排序且元字段置首。
pub(crate) fn search_result_set(payload: &Value, max_rows: u32) -> Result<ResultSet, DriverError> {
    let hits = payload
        .get("hits")
        .and_then(|hits| hits.get("hits"))
        .and_then(Value::as_array);
    let mut rows: Vec<BTreeMap<String, Value>> = Vec::new();
    let mut truncated = false;
    if let Some(hits) = hits {
        let limit = if max_rows == 0 { u32::MAX } else { max_rows } as usize;
        for hit in hits {
            if rows.len() >= limit {
                truncated = true;
                break;
            }
            let Some(object) = hit.as_object() else {
                continue;
            };
            let mut row = BTreeMap::new();
            if let Some(value) = object.get("_index") {
                row.insert("_index".into(), value.clone());
            }
            if let Some(value) = object.get("_id") {
                row.insert("_id".into(), value.clone());
            }
            if let Some(score) = object.get("_score").filter(|value| !value.is_null()) {
                row.insert("_score".into(), score.clone());
            }
            if let Some(source) = object.get("_source").and_then(Value::as_object) {
                flatten_source("", source, &mut row);
                row.insert(
                    "_source".into(),
                    json!(Value::Object(source.clone()).to_string()),
                );
            }
            if let Some(fields) = object.get("fields").and_then(Value::as_object) {
                for (key, value) in fields {
                    row.insert(key.clone(), normalize_field_value(value));
                }
            }
            rows.push(row);
        }
    }

    if let Some(aggregations) = payload
        .get("aggregations")
        .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()))
    {
        let rendered = serde_json::to_string_pretty(aggregations).unwrap_or_default();
        if let Some(first) = rows.first_mut() {
            first.insert("_aggregations".into(), json!(rendered));
        } else {
            let mut row = BTreeMap::new();
            row.insert("_aggregations".into(), json!(rendered));
            rows.push(row);
        }
    }

    let mut names: BTreeSet<String> = BTreeSet::new();
    for row in &rows {
        names.extend(row.keys().cloned());
    }
    let mut columns: Vec<String> = names.into_iter().collect();
    for meta in META_FIELDS.iter().rev() {
        if let Some(position) = columns.iter().position(|name| name == meta) {
            let value = columns.remove(position);
            columns.insert(0, value);
        }
    }

    let data = rows
        .into_iter()
        .map(|row| {
            columns
                .iter()
                .map(|name| row.get(name).cloned().unwrap_or(Value::Null))
                .collect()
        })
        .collect();

    Ok(ResultSet {
        columns: columns
            .into_iter()
            .map(|name| Column {
                id: name.clone(),
                name,
            })
            .collect(),
        rows: data,
        messages: Vec::new(),
        rows_affected: 0,
        truncated,
        duration_ns: 0,
    })
}

fn flatten_source(prefix: &str, source: &Map<String, Value>, row: &mut BTreeMap<String, Value>) {
    for (key, value) in source {
        let next = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Object(child) => flatten_source(&next, child, row),
            Value::Array(_) => {
                row.insert(next, json!(value.to_string()));
            }
            Value::Null => {
                row.insert(next, Value::Null);
            }
            other => {
                row.insert(next, other.clone());
            }
        }
    }
}

fn normalize_field_value(value: &Value) -> Value {
    match value {
        Value::Array(items) if items.len() == 1 => items[0].clone(),
        Value::Array(_) => json!(value.to_string()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_json_dsl() {
        let request = parse_request("{\"query\":{\"match_all\":{}}}", "logs").unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/logs/_search");
        assert!(request.body.contains("match_all"));
    }

    #[test]
    fn parses_devtools_style() {
        let source = "GET /logs/_search\n{\"size\": 1}";
        let request = parse_request(source, "").unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/logs/_search");
        assert_eq!(request.body, "{\"size\": 1}");
    }

    #[test]
    fn strips_full_line_comments() {
        let source = "# 注释\nPOST /_count";
        let request = parse_request(source, "").unwrap();
        assert_eq!(request.path, "/_count");
        assert!(request.body.is_empty());
    }

    #[test]
    fn rejects_write_and_unknown_endpoints() {
        assert!(parse_request("DELETE /logs", "").is_err());
        assert!(parse_request("PUT /logs/_doc/1 {}", "").is_err());
        assert!(parse_request("GET /_cat/indices", "").is_err());
        assert!(parse_request("GET /logs/_mapping", "").is_err());
        assert!(parse_request("GET /a/b/_search", "").is_err());
    }

    #[test]
    fn flattens_nested_sources_and_meta_first() {
        let payload = json!({
            "hits": {"hits": [
                {"_index": "logs", "_id": "1", "_score": 1.5,
                 "_source": {"user": {"name": "甲"}, "tags": ["a", "b"], "n": 3}}
            ]}
        });
        let result = search_result_set(&payload, 10).unwrap();
        let names: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "_index",
                "_id",
                "_score",
                "_source",
                "n",
                "tags",
                "user.name"
            ]
        );
        assert_eq!(result.rows[0][4], json!(3));
        assert_eq!(result.rows[0][5], json!("[\"a\",\"b\"]"));
        assert_eq!(result.rows[0][6], json!("甲"));
    }

    #[test]
    fn attaches_aggregations_to_the_first_row() {
        let payload = json!({
            "hits": {"hits": [{"_index": "i", "_id": "1", "_source": {"a": 1}}]},
            "aggregations": {"total": {"value": 7}}
        });
        let result = search_result_set(&payload, 10).unwrap();
        assert!(
            result.rows[0]
                .iter()
                .any(|value| value.as_str().unwrap_or_default().contains("total"))
        );
    }

    #[test]
    fn counts_from_the_count_endpoint() {
        let result = count_result_set(&json!({"count": 42, "_shards": {}})).unwrap();
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.rows[0][0], json!(42));
    }

    /// 需要真实服务：SUPERLINK_TEST_ES=1 + HOST/PORT/USER/PASSWORD/INDEX
    #[test]
    fn queries_a_live_server_when_configured() {
        if std::env::var("SUPERLINK_TEST_ES").is_err() {
            eprintln!("skipping: SUPERLINK_TEST_ES not set");
            return;
        }
        let config = ConnectionConfig {
            kind: "elasticsearch".into(),
            host: std::env::var("SUPERLINK_TEST_ES_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("SUPERLINK_TEST_ES_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(9200),
            user: std::env::var("SUPERLINK_TEST_ES_USER").unwrap_or_default(),
            password: std::env::var("SUPERLINK_TEST_ES_PASSWORD").unwrap_or_default(),
            database: std::env::var("SUPERLINK_TEST_ES_INDEX").unwrap_or_else(|_| "_all".into()),
            ..Default::default()
        };
        let mut driver = NativeElasticsearch::open(&config).unwrap();

        let count = driver.query("POST /_count", &[], 10).unwrap();
        assert_eq!(count.columns[0].name, "count");

        let search = driver
            .query("{\"query\":{\"match_all\":{}},\"size\":1}", &[], 10)
            .unwrap();
        // 空索引也可能返回 0 行；只要求请求成功且列结构自洽。
        for row in &search.rows {
            assert_eq!(row.len(), search.columns.len());
        }
    }

    #[test]
    fn builds_basic_auth_and_endpoint() {
        let config = ConnectionConfig {
            kind: "elasticsearch".into(),
            host: "es.local".into(),
            port: 9200,
            user: "elastic".into(),
            password: "pw".into(),
            ..Default::default()
        };
        let driver = NativeElasticsearch::open(&config).unwrap();
        assert_eq!(driver.endpoint, "http://es.local:9200");
        assert_eq!(
            driver.auth.unwrap(),
            format!("Basic {}", BASE64.encode("elastic:pw"))
        );
    }
}
