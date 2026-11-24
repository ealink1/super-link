//! Shared HTTP transport: bounded responses, no redirects, TLS verification and cancellation.
use reqwest::Method;
use serde_json::{Value, json};
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::{QueryCancel, native::http::Http};
pub struct Rest {
    pub config: ConnectionConfig,
    pub base: String,
    pub http: Http,
    pub params: std::collections::HashMap<String, String>,
    pub chroma_api: std::sync::OnceLock<String>,
}
impl Rest {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        let mut config = config.clone();
        if config.query_timeout == 0 {
            config.query_timeout = 30;
        }
        let params = url::form_urlencoded::parse(config.connection_params.as_bytes())
            .into_owned()
            .collect();
        let base = if config.uri.starts_with("http://") || config.uri.starts_with("https://") {
            config.uri.trim_end_matches('/').into()
        } else {
            format!(
                "{}://{}:{}",
                if config.use_ssl { "https" } else { "http" },
                if config.host.is_empty() {
                    "127.0.0.1"
                } else {
                    &config.host
                },
                config.port
            )
        };
        let mut http = Http::new(&config).map_err(|e| e.to_string())?;
        http.set_cancellation(cancel);
        Ok(Self {
            config,
            base,
            http,
            params,
            chroma_api: std::sync::OnceLock::new(),
        })
    }
    pub fn request(&self, method: Method, path: &str) -> Result<reqwest::RequestBuilder, String> {
        if !path.starts_with('/') || path.starts_with("//") || path.contains("..") {
            return Err("HTTP 路径无效".into());
        }
        let mut request = self.http.request(method, &format!("{}{path}", self.base));
        if !self.config.user.is_empty() {
            request = request.basic_auth(&self.config.user, Some(&self.config.password));
        }
        if let Some(token) = self
            .params
            .get("token")
            .or_else(|| self.params.get("apiKey"))
        {
            request = request.bearer_auth(token);
        }
        if self.config.kind == "qdrant"
            && let Some(token) = self
                .params
                .get("apiKey")
                .or_else(|| self.params.get("api_key"))
        {
            request = request.header("api-key", token);
        }
        if self.config.kind == "milvus" && !self.config.user.is_empty() {
            request = request.bearer_auth(format!("{}:{}", self.config.user, self.config.password));
        }
        Ok(request)
    }
    pub fn call(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let mut request = self.request(method, path)?;
        if let Some(body) = body {
            request = request.json(body);
        }
        let bytes = self.http.bytes(request).map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        let value: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        if value
            .get("code")
            .and_then(Value::as_i64)
            .is_some_and(|c| c != 0 && c != 200)
        {
            return Err("服务端拒绝请求".into());
        }
        Ok(value)
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        let value = match self.config.kind.as_str() {
            "qdrant" => self.qdrant(text)?,
            "chroma" => self.chroma(text)?,
            "milvus" => self.milvus(text)?,
            "nacos" => self.nacos(text)?,
            "rabbitmq" => self.rabbitmq(text, limit)?,
            "tdengine" => self.tdengine(text)?,
            "trino" => return self.trino(text, limit),
            "elasticsearch" => return self.elasticsearch(text, limit),
            _ => return Err("HTTP 数据源类型无效".into()),
        };
        Ok(crate::rows::json_rows(value, limit))
    }
    pub fn databases(&mut self) -> Result<Vec<String>, String> {
        let value = match self.config.kind.as_str() {
            "nacos" => self.nacos(r#"{"op":"namespaces"}"#)?,
            "rabbitmq" => self.call(Method::GET, "/api/vhosts", None)?,
            "milvus" => self.call(
                Method::POST,
                "/v2/vectordb/databases/list",
                Some(&json!({})),
            )?["data"]
                .clone(),
            "tdengine" => return Ok(crate::rows::list(self.query("SHOW DATABASES", 10000)?)),
            "trino" => return Ok(crate::rows::list(self.query("SHOW CATALOGS", 10000)?)),
            "elasticsearch" => self.call(Method::GET, "/_cat/indices?format=json", None)?,
            _ => return Ok(vec![self.config.database.clone()]),
        };
        Ok(names(value, &["namespace", "name", "index"]))
    }
    pub fn tables(&mut self, database: &str) -> Result<Vec<String>, String> {
        if !database.is_empty() {
            self.config.database = database.into();
        }
        let value = match self.config.kind.as_str() {
            "elasticsearch" => self.call(Method::GET, "/_cat/indices?format=json", None)?,
            "qdrant" => self.qdrant(r#"{"list_collections":true}"#)?,
            "chroma" => self.chroma(r#"{"list_collections":true}"#)?,
            "milvus" => self.milvus(r#"{"list_collections":true}"#)?,
            "nacos" => self.nacos(&json!({"op":"configs","namespace":database}).to_string())?,
            "rabbitmq" => self.call(
                Method::GET,
                &format!(
                    "/api/queues/{}",
                    segment(if database.is_empty() { "/" } else { database })
                ),
                None,
            )?,
            "tdengine" | "trino" => {
                return Ok(crate::rows::list(self.query("SHOW TABLES", 10000)?));
            }
            _ => return Err("HTTP 目录类型无效".into()),
        };
        Ok(names(value, &["name", "dataId", "index"]))
    }
}
pub fn segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
pub fn command(text: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| "命令必须是 JSON")?;
    if !value.is_object() {
        return Err("命令必须是 JSON 对象".into());
    }
    Ok(value)
}
pub fn target<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a str, String> {
    keys.iter()
        .find_map(|k| value.get(k).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "命令缺少对象名称".into())
}
pub fn names(value: Value, keys: &[&str]) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.as_str()
                .or_else(|| keys.iter().find_map(|k| v.get(k).and_then(Value::as_str)))
        })
        .take(10000)
        .map(str::to_string)
        .collect()
}
