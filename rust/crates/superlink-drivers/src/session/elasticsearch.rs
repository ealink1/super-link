use super::*;
impl DriverSession {
    pub fn elasticsearch_console(&mut self, source: &str) -> Result<ResultSet, DriverError> {
        let request = console_request(source, &self.search_index)?;
        let value = self.agent.call(
            "executeElasticsearchConsoleRequest",
            named([
                ("elasticsearchRequest", request),
                ("timeoutMs", json!(self.timeout_ms)),
            ]),
        )?;
        let status = value
            .get("statusCode")
            .and_then(Value::as_u64)
            .ok_or(DriverError::Malformed("Elasticsearch HTTP status"))?;
        let mut body = value
            .get("rawBody")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let truncated = body.len() > ((1 << 20) - 128);
        if truncated {
            let mut end = (1 << 20) - 128;
            while !body.is_char_boundary(end) {
                end -= 1;
            }
            body.truncate(end);
            body.push_str("…（响应已截断）");
        }
        Ok(ResultSet {
            columns: vec![
                Column {
                    id: "http_status".into(),
                    name: "HTTP 状态".into(),
                },
                Column {
                    id: "response".into(),
                    name: "响应".into(),
                },
            ],
            rows: vec![vec![json!(status), json!(body)]],
            messages: if status >= 400 {
                vec![format!("HTTP {status}：请求未成功，请查看响应内容")]
            } else {
                Vec::new()
            },
            truncated,
            ..Default::default()
        })
    }
}
fn console_request(source: &str, index: &str) -> Result<Value, DriverError> {
    if source.len() > 1 << 20 {
        return Err(DriverError::Malformed(
            "Elasticsearch request exceeds 1 MiB",
        ));
    }
    let source = source.trim();
    if source.starts_with('{') {
        return Ok(
            json!({"method":"POST","path":if index.is_empty(){"/_search".into()}else{format!("/{index}/_search")},"body":source,"bodyKind":"json"}),
        );
    }
    let (header, rest) = source.split_once('\n').unwrap_or((source, ""));
    let mut words = header.splitn(3, char::is_whitespace);
    let method = words.next().unwrap_or_default().to_uppercase();
    let path = words.next().unwrap_or_default();
    if !["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str())
        || !path.starts_with('/')
        || path.starts_with("//")
    {
        return Err(DriverError::Malformed("Elasticsearch method/path"));
    }
    let tail = words.next().unwrap_or_default().trim();
    let body = if tail.is_empty() {
        rest.to_string()
    } else {
        format!("{tail}\n{rest}")
    };
    let kind = if body.trim().is_empty() {
        "none"
    } else if path
        .split('?')
        .next()
        .is_some_and(|p| p.ends_with("/_bulk") || p.ends_with("/_msearch"))
    {
        "ndjson"
    } else {
        "json"
    };
    Ok(json!({"method":method,"path":path,"body":body,"bodyKind":kind}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_method_path_and_ndjson_body_for_the_console_executor() {
        let request =
            console_request("POST /_msearch\n{}\n{\"query\":{\"match_all\":{}}}\n", "").unwrap();
        assert_eq!(request["method"], "POST");
        assert_eq!(request["path"], "/_msearch");
        assert_eq!(request["bodyKind"], "ndjson");
        assert!(console_request("GET https://other.example/", "").is_err());
        assert_eq!(
            console_request("GET /_cat/indices", "").unwrap()["bodyKind"],
            "none"
        );
    }
}
