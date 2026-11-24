use crate::http::{Rest, command, segment};
use reqwest::Method;
use serde_json::{Value, json};
use superlink_domain::{Column, ResultSet};
impl Rest {
    pub fn nacos(&self, text: &str) -> Result<Value, String> {
        let c = command(text)?;
        let op = c["op"].as_str().unwrap_or("configs");
        let namespace = c["namespace"].as_str().unwrap_or(&self.config.database);
        let mut fields = vec![
            ("tenant", namespace.to_string()),
            ("namespaceId", namespace.to_string()),
            (
                "group",
                c["group"].as_str().unwrap_or("DEFAULT_GROUP").into(),
            ),
            ("dataId", c["dataId"].as_str().unwrap_or("").into()),
        ];
        let token = if !self.config.user.is_empty() {
            let bytes = self
                .http
                .bytes(self.request(Method::POST, "/nacos/v1/auth/login")?.form(&[
                    ("username", &self.config.user),
                    ("password", &self.config.password),
                ]))
                .map_err(|e| e.to_string())?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Nacos 登录响应无效")?;
            value["accessToken"]
                .as_str()
                .ok_or("Nacos 登录未获得令牌")?
                .to_string()
        } else {
            String::new()
        };
        if !token.is_empty() {
            fields.push(("accessToken", token));
        }
        let (method, path) = match op {
            "namespaces" => (Method::GET, "/nacos/v1/console/namespaces"),
            "services" => {
                fields.extend([("pageNo", "1".into()), ("pageSize", "100".into())]);
                (Method::GET, "/nacos/v1/ns/service/list")
            }
            "instances" => {
                fields.push(("serviceName", c["service"].as_str().unwrap_or("").into()));
                (Method::GET, "/nacos/v1/ns/instance/list")
            }
            "publish" => {
                fields.push(("content", c["content"].as_str().unwrap_or("").into()));
                fields.push(("type", c["type"].as_str().unwrap_or("text").into()));
                (Method::POST, "/nacos/v1/cs/configs")
            }
            "delete" => (Method::DELETE, "/nacos/v1/cs/configs"),
            "get" => (Method::GET, "/nacos/v1/cs/configs"),
            "configs" => {
                fields.extend([
                    ("search", "blur".into()),
                    ("pageNo", c["page"].as_u64().unwrap_or(1).to_string()),
                    ("pageSize", "100".into()),
                ]);
                (Method::GET, "/nacos/v1/cs/configs")
            }
            _ => return Err("Nacos 操作无效".into()),
        };
        let request = self.request(method.clone(), path)?;
        let request = if method == Method::POST {
            request.form(&fields)
        } else {
            request.query(&fields)
        };
        let bytes = self.http.bytes(request).map_err(|e| e.to_string())?;
        if op == "get" {
            return Ok(Value::String(
                String::from_utf8(bytes).map_err(|_| "Nacos 内容不是 UTF-8")?,
            ));
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Nacos 响应无效")?;
        Ok(value
            .get("pageItems")
            .or_else(|| value.get("data"))
            .or_else(|| value.get("hosts"))
            .cloned()
            .unwrap_or(value))
    }
    pub fn rabbitmq(&self, text: &str, limit: u32) -> Result<Value, String> {
        let vhost = if self.config.database.is_empty() {
            "/"
        } else {
            &self.config.database
        };
        if text.trim_start().starts_with('{') {
            let c = command(text)?;
            let exchange = c["exchange"].as_str().unwrap_or("");
            let routing = c["routing_key"]
                .as_str()
                .or_else(|| c["publish"].as_str())
                .unwrap_or("");
            return self.call(Method::POST,&format!("/api/exchanges/{}/{}/publish",segment(vhost),segment(exchange)),Some(&json!({"properties":c.get("properties").cloned().unwrap_or(json!({})),"routing_key":routing,"payload":c["payload"].as_str().unwrap_or(""),"payload_encoding":"string"})));
        }
        let (queue, count, _) = message_read(text)?;
        self.call(Method::POST,&format!("/api/queues/{}/{}/get",segment(vhost),segment(&queue)),Some(&json!({"count":count.min(limit.max(1)),"ackmode":"ack_requeue_true","encoding":"auto","truncate":1<<20})))
    }
    pub fn tdengine(&self, text: &str) -> Result<Value, String> {
        let database = &self.config.database;
        let request = self
            .request(Method::POST, &format!("/rest/sql/{}", segment(database)))?
            .body(text.to_string());
        let value = self.http.json(request).map_err(|e| e.to_string())?;
        if value["code"].as_i64().is_some_and(|code| code != 0) {
            return Err("TDengine SQL 执行失败".into());
        }
        let names: Vec<_> = value["column_meta"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c[0].as_str())
            .collect();
        let rows = value["data"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| {
                Value::Object(
                    names
                        .iter()
                        .enumerate()
                        .map(|(n, name)| (name.to_string(), row[n].clone()))
                        .collect(),
                )
            })
            .collect();
        Ok(Value::Array(rows))
    }
    pub fn trino(&self, text: &str, limit: u32) -> Result<ResultSet, String> {
        let mut request = self
            .request(Method::POST, "/v1/statement")?
            .header(
                "X-Trino-User",
                if self.config.user.is_empty() {
                    "superlink"
                } else {
                    &self.config.user
                },
            )
            .body(text.to_string());
        let mut scope = self.config.database.splitn(2, '.');
        if let Some(catalog) = scope.next().filter(|s| !s.is_empty()) {
            request = request.header("X-Trino-Catalog", catalog);
        }
        if let Some(schema) = scope.next() {
            request = request.header("X-Trino-Schema", schema);
        }
        let mut value = self.http.json(request).map_err(|e| e.to_string())?;
        let mut result = ResultSet::default();
        let mut budget = superlink_drivers::budget::ResultBudget::default();
        loop {
            if value.get("error").is_some() {
                return Err("Trino SQL 执行失败".into());
            }
            if result.columns.is_empty() {
                result.columns = value["columns"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["name"].as_str())
                    .map(|name| Column {
                        id: name.into(),
                        name: name.into(),
                    })
                    .collect();
            }
            for row in value["data"].as_array().into_iter().flatten() {
                let row = row.as_array().ok_or("Trino 行格式无效")?.clone();
                if (limit > 0 && result.rows.len() >= limit as usize) || !budget.accept(&row) {
                    result.truncated = true;
                    break;
                }
                result.rows.push(row);
            }
            let Some(next) = value["nextUri"].as_str() else {
                break;
            };
            let url = url::Url::parse(next).map_err(|_| "Trino 分页地址无效")?;
            if url.origin()
                != url::Url::parse(&self.base)
                    .map_err(|_| "Trino 地址无效")?
                    .origin()
            {
                return Err("Trino 分页跨源，已拒绝发送凭据".into());
            }
            let path = format!(
                "{}{}",
                url.path(),
                url.query().map(|q| format!("?{q}")).unwrap_or_default()
            );
            if result.truncated {
                let _ = self.call(Method::DELETE, &path, None);
                break;
            }
            value = self.call(Method::GET, &path, None)?;
        }
        result.rows_affected = value["updateCount"].as_i64().unwrap_or(0);
        Ok(result)
    }
}
pub fn message_read(text: &str) -> Result<(String, u32, bool), String> {
    let words = shlex::split(text).ok_or("消息预览引号无效")?;
    let from = words
        .iter()
        .position(|w| w.eq_ignore_ascii_case("FROM"))
        .ok_or("消息预览必须包含 FROM")?;
    if !words
        .first()
        .is_some_and(|w| w.eq_ignore_ascii_case("SELECT"))
    {
        return Err("消息预览必须使用 SELECT".into());
    }
    let name = words.get(from + 1).ok_or("消息预览缺少对象")?.clone();
    let count = words
        .iter()
        .position(|w| w.eq_ignore_ascii_case("LIMIT"))
        .and_then(|n| words.get(n + 1))
        .and_then(|s| s.trim_end_matches(';').parse::<u32>().ok())
        .unwrap_or(10)
        .clamp(1, 100);
    Ok((
        name,
        count,
        words.iter().any(|w| w.eq_ignore_ascii_case("LATEST")),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_preview_is_bounded() {
        assert_eq!(
            message_read("SELECT * FROM \"topic a\" LATEST LIMIT 100000").unwrap(),
            ("topic a".into(), 100, true)
        );
        assert!(message_read("DELETE FROM q").is_err());
    }
}
