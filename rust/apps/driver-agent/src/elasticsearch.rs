use crate::http::{Rest, segment};
use reqwest::Method;
use serde_json::{Value, json};
use superlink_domain::{Column, ResultSet};
impl Rest {
    pub fn elasticsearch_console(&self, request: &Value) -> Result<Value, String> {
        if self.config.kind != "elasticsearch" {
            return Err("此连接不是 Elasticsearch".into());
        }
        let request = &request["elasticsearchRequest"];
        let method = request["method"].as_str().unwrap_or("");
        if !["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"].contains(&method) {
            return Err("HTTP 方法无效".into());
        }
        let path = request["path"].as_str().ok_or("请求路径无效")?;
        let body = request["body"].as_str().unwrap_or("");
        if body.len() > 1 << 20 {
            return Err("请求体超过 1 MiB".into());
        }
        let mut builder = self.request(
            Method::from_bytes(method.as_bytes()).map_err(|_| "HTTP 方法无效")?,
            path,
        )?;
        let kind = request["bodyKind"].as_str().unwrap_or("json");
        if !body.is_empty() {
            let content_type = match kind {
                "ndjson" => "application/x-ndjson",
                "json" => "application/json",
                _ => return Err("请求体类型无效".into()),
            };
            let body = if kind == "ndjson" && !body.ends_with('\n') {
                format!("{body}\n")
            } else {
                body.into()
            };
            builder = builder.header("Content-Type", content_type).body(body);
        }
        let start = std::time::Instant::now();
        let (status, bytes) = self.http.response(builder).map_err(|e| e.to_string())?;
        Ok(
            json!({"statusCode":status,"rawBody":String::from_utf8_lossy(&bytes),"duration":start.elapsed().as_nanos() as u64}),
        )
    }
    pub fn elasticsearch(&self, text: &str, limit: u32) -> Result<ResultSet, String> {
        let source = text
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        let source = source.trim();
        let (method, path, body) = if source.starts_with('{') {
            (
                "POST".to_string(),
                format!("/{}/_search", segment(&self.config.database)),
                source.to_string(),
            )
        } else if source.to_ascii_uppercase().starts_with("SELECT ") {
            let value = self.call(
                Method::POST,
                "/_sql?format=json",
                Some(&json!({"query":source,"fetch_size":limit.clamp(1,10000)})),
            )?;
            let fields = value["columns"]
                .as_array()
                .ok_or("SQL 响应缺少列")?
                .iter()
                .map(|c| c["name"].as_str().unwrap_or("").to_string())
                .collect::<Vec<_>>();
            let rows = value["rows"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|r| r.as_array().cloned())
                .collect();
            if let Some(cursor) = value["cursor"].as_str() {
                self.call(Method::POST, "/_sql/close", Some(&json!({"cursor":cursor})))?;
            }
            return Ok(ResultSet {
                columns: fields
                    .into_iter()
                    .map(|name| Column {
                        id: name.clone(),
                        name,
                    })
                    .collect(),
                rows,
                ..Default::default()
            });
        } else {
            let (header, body) = source.split_once('\n').unwrap_or((source, ""));
            let mut words = header.splitn(3, char::is_whitespace);
            let method = words.next().unwrap_or("").to_ascii_uppercase();
            let path = words.next().ok_or("请求缺少路径")?.to_string();
            let tail = words.next().unwrap_or("");
            (method, path, format!("{tail}\n{body}"))
        };
        let kind = if path
            .split('?')
            .next()
            .is_some_and(|p| p.ends_with("/_bulk") || p.ends_with("/_msearch"))
        {
            "ndjson"
        } else {
            "json"
        };
        let response=self.elasticsearch_console(&json!({"elasticsearchRequest":{"method":method,"path":path,"body":body,"bodyKind":kind}}))?;
        if response["statusCode"].as_u64().unwrap_or(500) >= 400 {
            return Err(format!(
                "Elasticsearch 返回 HTTP {}",
                response["statusCode"]
            ));
        }
        let raw = response["rawBody"].as_str().unwrap_or("");
        let value: Value = serde_json::from_str(raw).map_err(|_| "Elasticsearch 响应不是 JSON")?;
        if value.get("hits").is_some() || value.get("aggregations").is_some() {
            return superlink_drivers::native::elasticsearch::search_result_set(&value, limit)
                .map_err(|e| e.to_string());
        }
        Ok(crate::rows::json_rows(value, limit))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    #[test]
    fn console_preserves_http_errors_and_ndjson_without_retrying() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 8192);
            }
            let header = String::from_utf8(bytes).unwrap();
            assert!(header.starts_with("POST /_msearch HTTP/1.1"));
            assert!(header.contains("application/x-ndjson"));
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap();
            let mut body = vec![0; length];
            socket.read_exact(&mut body).unwrap();
            assert_eq!(body, b"{}\n{}\n");
            let body = r#"{"error":"bad query"}"#;
            write!(
                socket,
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let config = superlink_domain::ConnectionConfig {
            kind: "elasticsearch".into(),
            host: "127.0.0.1".into(),
            port,
            ..Default::default()
        };
        let rest = Rest::open(&config, &superlink_drivers::QueryCancel::new()).unwrap();
        let value=rest.elasticsearch_console(&json!({"elasticsearchRequest":{"method":"POST","path":"/_msearch","body":"{}\n{}","bodyKind":"ndjson"}})).unwrap();
        assert_eq!(value["statusCode"], 400);
        assert_eq!(value["rawBody"], r#"{"error":"bad query"}"#);
        assert!(
            rest.elasticsearch_console(
                &json!({"elasticsearchRequest":{"method":"GET","path":"//other.example/a"}})
            )
            .is_err()
        );
        worker.join().unwrap();
    }
}
