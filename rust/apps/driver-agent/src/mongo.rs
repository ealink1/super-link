use mongodb::{
    bson::{Bson, Document, doc},
    sync::Client,
};
use serde_json::Value;
use superlink_domain::{ConnectionConfig, ResultSet};
pub struct Mongo {
    client: Client,
    database: String,
}
impl Mongo {
    pub fn open(config: &ConnectionConfig) -> Result<Self, String> {
        let uri = if !config.uri.is_empty() {
            config.uri.clone()
        } else {
            let hosts = if config.hosts.is_empty() {
                format!(
                    "{}:{}",
                    config.host,
                    if config.port == 0 { 27017 } else { config.port }
                )
            } else {
                config.hosts.join(",")
            };
            let escape = crate::http::segment;
            let auth = if config.user.is_empty() {
                String::new()
            } else {
                format!("{}:{}@", escape(&config.user), escape(&config.password))
            };
            let mut params = url::form_urlencoded::Serializer::new(String::new());
            params.append_pair(
                "serverSelectionTimeoutMS",
                &(config.timeout.max(1) * 1000).to_string(),
            );
            params.append_pair(
                "connectTimeoutMS",
                &(config.timeout.max(1) * 1000).to_string(),
            );
            for (k, v) in [
                ("authSource", &config.auth_source),
                ("replicaSet", &config.replica_set),
                ("readPreference", &config.read_preference),
                ("authMechanism", &config.mongo_auth_mechanism),
                ("tlsCAFile", &config.ssl_ca_path),
            ] {
                if !v.is_empty() {
                    params.append_pair(k, v);
                }
            }
            if config.use_ssl {
                params.append_pair("tls", "true");
            }
            format!(
                "mongodb{}://{auth}{hosts}/{}?{}",
                if config.mongo_srv { "+srv" } else { "" },
                escape(&config.database),
                params.finish()
            )
        };
        let client = Client::with_uri_str(uri).map_err(|_| "MongoDB 配置或连接失败")?;
        let database = if config.database.is_empty() {
            "admin".into()
        } else {
            config.database.clone()
        };
        client
            .database(&database)
            .run_command(doc! {"ping":1})
            .run()
            .map_err(|_| "MongoDB 验证连接失败")?;
        Ok(Self { client, database })
    }
    pub fn query(&mut self, text: &str, limit: u32) -> Result<ResultSet, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|_| "MongoDB 命令必须是 JSON 对象")?;
        let mut command: Document = Bson::try_from(value.clone())
            .map_err(|_| "MongoDB Extended JSON 无效")?
            .as_document()
            .cloned()
            .ok_or("MongoDB 命令必须是对象")?;
        // MongoDB identifies a command by its first BSON key; serde_json maps
        // are sorted, so a filter must never precede `find` on the wire.
        let name = [
            "find",
            "aggregate",
            "insert",
            "update",
            "delete",
            "count",
            "distinct",
            "findAndModify",
            "create",
            "drop",
            "createIndexes",
            "dropIndexes",
            "listIndexes",
            "collStats",
            "dbStats",
            "ping",
            "explain",
        ]
        .into_iter()
        .find(|name| command.contains_key(*name))
        .ok_or("MongoDB 命令缺少受支持的操作")?;
        let verb = command.remove(name).ok_or("MongoDB 命令无效")?;
        let mut ordered = Document::new();
        ordered.insert(name, verb);
        ordered.extend(command);
        let mut command = ordered;
        if command.contains_key("find") {
            let count = value["limit"]
                .as_u64()
                .unwrap_or(1000)
                .min(if limit == 0 { 10000 } else { limit.into() })
                .clamp(1, 10000) as i64;
            command.insert("batchSize", count);
            command.insert("singleBatch", true);
            command.insert("limit", count);
        }
        if command.contains_key("aggregate") && !command.contains_key("cursor") {
            command.insert("cursor", doc! {"batchSize":256});
        }
        let document = self
            .client
            .database(&self.database)
            .run_command(command)
            .run()
            .map_err(|_| "MongoDB 命令执行失败")?;
        if document
            .get_array("writeErrors")
            .is_ok_and(|a| !a.is_empty())
            || document.contains_key("writeConcernError")
        {
            return Err("MongoDB 写入或写关注失败".into());
        }
        let mut value = serde_json::to_value(document).map_err(|_| "MongoDB 响应编码失败")?;
        if value.get("cursor").is_none() {
            return Ok(crate::rows::json_rows(value, limit));
        }
        let mut documents = Vec::new();
        let mut bytes = 0;
        let mut truncated = false;
        let bound = if limit == 0 { 10000 } else { limit as usize };
        let mut cursor;
        let mut collection = String::new();
        loop {
            cursor = value["cursor"]["id"].as_i64().unwrap_or(0);
            if let Some(namespace) = value["cursor"]["ns"].as_str() {
                collection = namespace
                    .split_once('.')
                    .map(|(_, c)| c)
                    .unwrap_or(namespace)
                    .into();
            }
            for document in value
                .pointer("/cursor/firstBatch")
                .or_else(|| value.pointer("/cursor/nextBatch"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let size = serde_json::to_vec(document)
                    .map_err(|_| "MongoDB 文档编码失败")?
                    .len();
                if documents.len() >= bound || bytes + size > 24 << 20 {
                    truncated = true;
                    break;
                }
                documents.push(document.clone());
                bytes += size;
            }
            if cursor == 0 || truncated {
                break;
            }
            if documents.len() >= bound {
                truncated = true;
                break;
            }
            let response = self
                .client
                .database(&self.database)
                .run_command(doc! {"getMore":cursor,"collection":&collection,"batchSize":256})
                .run()
                .map_err(|_| "MongoDB 游标读取失败")?;
            value = serde_json::to_value(response).map_err(|_| "MongoDB 响应编码失败")?;
        }
        if cursor != 0 {
            let _ = self
                .client
                .database(&self.database)
                .run_command(doc! {"killCursors":collection,"cursors":[cursor]})
                .run();
        }
        let mut result = crate::rows::json_rows(Value::Array(documents), limit);
        result.truncated |= truncated;
        Ok(result)
    }
    pub fn databases(&mut self) -> Result<Vec<String>, String> {
        self.client
            .list_database_names()
            .run()
            .map_err(|_| "MongoDB 数据库目录读取失败".into())
    }
    pub fn tables(&mut self, database: &str) -> Result<Vec<String>, String> {
        self.client
            .database(if database.is_empty() {
                &self.database
            } else {
                database
            })
            .list_collection_names()
            .run()
            .map(|v| v.into_iter().take(10000).collect())
            .map_err(|_| "MongoDB 集合目录读取失败".into())
    }
}
