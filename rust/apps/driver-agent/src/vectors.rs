use crate::http::{Rest, command, segment, target};
use reqwest::Method;
use serde_json::{Value, json};
impl Rest {
    pub fn qdrant(&self, text: &str) -> Result<Value, String> {
        let c = command(text)?;
        if c.get("list_collections").is_some() {
            return Ok(
                self.call(Method::GET, "/collections", None)?["result"]["collections"].clone(),
            );
        }
        let name = target(
            &c,
            &[
                "scroll",
                "search",
                "count",
                "upsert",
                "delete",
                "create_collection",
                "delete_collection",
                "collection",
            ],
        )?;
        let root = format!("/collections/{}", segment(name));
        let limit = c["limit"].as_u64().unwrap_or(100).clamp(1, 10000);
        let (method, path, body) = if c.get("scroll").is_some() {
            (
                Method::POST,
                format!("{root}/points/scroll"),
                json!({"limit":limit,"offset":c["offset"],"filter":c["filter"],"with_payload":true,"with_vector":c["with_vector"].as_bool().unwrap_or(false)}),
            )
        } else if c.get("search").is_some() {
            (
                Method::POST,
                format!("{root}/points/search"),
                json!({"vector":c["vector"],"limit":limit,"filter":c["filter"],"with_payload":true}),
            )
        } else if c.get("count").is_some() {
            (
                Method::POST,
                format!("{root}/points/count"),
                json!({"filter":c["filter"],"exact":true}),
            )
        } else if c.get("upsert").is_some() {
            (
                Method::PUT,
                format!("{root}/points?wait=true"),
                json!({"points":c["points"]}),
            )
        } else if c.get("delete").is_some() {
            (
                Method::POST,
                format!("{root}/points/delete?wait=true"),
                json!({"points":c["points"],"filter":c["filter"]}),
            )
        } else if c.get("create_collection").is_some() {
            (Method::PUT, root, json!({"vectors":c["vectors"]}))
        } else if c.get("delete_collection").is_some() {
            (Method::DELETE, root, Value::Null)
        } else {
            return Err("Qdrant 操作无效".into());
        };
        let value = self.call(method, &path, (!body.is_null()).then_some(&body))?;
        Ok(value
            .pointer("/result/points")
            .or_else(|| value.get("result"))
            .cloned()
            .unwrap_or(value))
    }
    fn chroma_root(&self) -> Result<String, String> {
        if let Some(root) = self.chroma_api.get() {
            return Ok(root.clone());
        }
        let (status, _) = self
            .http
            .response(self.request(Method::GET, "/api/v2/heartbeat")?)
            .map_err(|e| e.to_string())?;
        let root = if status == 404 || status == 405 {
            "/api/v1".into()
        } else if (200..300).contains(&status) {
            format!(
                "/api/v2/tenants/{}/databases/{}",
                segment(
                    self.params
                        .get("tenant")
                        .map(String::as_str)
                        .unwrap_or("default_tenant")
                ),
                segment(if self.config.database.is_empty() {
                    "default_database"
                } else {
                    &self.config.database
                })
            )
        } else {
            return Err(format!("Chroma API 探测返回 HTTP {status}"));
        };
        let _ = self.chroma_api.set(root.clone());
        Ok(root)
    }
    pub fn chroma(&self, text: &str) -> Result<Value, String> {
        let c = command(text)?;
        let root = self.chroma_root()?;
        if c.get("list_collections").is_some() {
            return self.call(Method::GET, &format!("{root}/collections?limit=1000"), None);
        }
        let name = target(
            &c,
            &[
                "get",
                "query",
                "upsert",
                "add",
                "delete",
                "create_collection",
                "delete_collection",
                "collection",
            ],
        )?;
        if c.get("create_collection").is_some() {
            return self.call(Method::POST,&format!("{root}/collections"),Some(&json!({"name":name,"metadata":c["metadata"],"get_or_create":c["get_or_create"].as_bool().unwrap_or(false)})));
        }
        let collection = self.call(
            Method::GET,
            &format!("{root}/collections/{}", segment(name)),
            None,
        )?;
        let id = collection["id"].as_str().ok_or("Chroma 集合标识缺失")?;
        let path = format!("{root}/collections/{}", segment(id));
        if c.get("delete_collection").is_some() {
            return self.call(Method::DELETE, &path, None);
        }
        let action = ["get", "query", "upsert", "add", "delete"]
            .into_iter()
            .find(|k| c.get(k).is_some())
            .ok_or("Chroma 操作无效")?;
        let mut body = c.clone();
        for key in ["get", "query", "upsert", "add", "delete", "collection"] {
            body.as_object_mut().unwrap().remove(key);
        }
        if action == "get" {
            body["limit"] = json!(c["limit"].as_u64().unwrap_or(100).clamp(1, 10000));
        }
        self.call(Method::POST, &format!("{path}/{action}"), Some(&body))
            .map(chroma_rows)
    }
    pub fn milvus(&self, text: &str) -> Result<Value, String> {
        let c = command(text)?;
        if c.get("list_collections").is_some() {
            return Ok(self.call(
                Method::POST,
                "/v2/vectordb/collections/list",
                Some(&json!({"dbName":self.config.database})),
            )?["data"]
                .clone());
        }
        let key = [
            "get",
            "search",
            "insert",
            "upsert",
            "delete",
            "create_collection",
            "drop_collection",
            "create_index",
            "drop_index",
        ]
        .into_iter()
        .find(|k| c.get(k).is_some())
        .ok_or("Milvus 操作无效")?;
        let name = target(&c, &[key, "collection"])?;
        let action = match key {
            "get" => "entities/query",
            "search" => "entities/search",
            "create_collection" => "collections/create",
            "drop_collection" => "collections/drop",
            "create_index" => "indexes/create",
            "drop_index" => "indexes/drop",
            "insert" => "entities/insert",
            "upsert" => "entities/upsert",
            _ => "entities/delete",
        };
        let mut body = c.clone();
        let object = body.as_object_mut().unwrap();
        object.remove(key);
        object.remove("collection");
        body["collectionName"] = json!(name);
        body["dbName"] = json!(self.config.database);
        if key == "search" {
            body["data"] = json!([c["vector"]]);
            body.as_object_mut().unwrap().remove("vector");
        }
        if key == "get" {
            body["filter"] = json!(c["filter"].as_str().unwrap_or(""));
            body["outputFields"] = json!(["*"]);
        }
        Ok(
            self.call(Method::POST, &format!("/v2/vectordb/{action}"), Some(&body))?["data"]
                .clone(),
        )
    }
}
fn chroma_rows(value: Value) -> Value {
    let Some(ids) = value["ids"].as_array() else {
        return value;
    };
    let batches = if ids.first().is_some_and(Value::is_array) {
        ids.clone()
    } else {
        vec![Value::Array(ids.clone())]
    };
    let nested = ids.first().is_some_and(Value::is_array);
    let mut rows = Vec::new();
    for (batch, ids) in batches.iter().enumerate() {
        for (index, id) in ids.as_array().into_iter().flatten().enumerate() {
            if rows.len() >= 10000 {
                return Value::Array(rows);
            }
            let mut row = serde_json::Map::new();
            row.insert("id".into(), id.clone());
            for (source, target) in [
                ("documents", "document"),
                ("metadatas", "metadata"),
                ("embeddings", "embedding"),
                ("distances", "distance"),
            ] {
                let field = if nested {
                    &value[source][batch][index]
                } else {
                    &value[source][index]
                };
                if !field.is_null() {
                    row.insert(target.into(), field.clone());
                }
            }
            rows.push(Value::Object(row));
        }
    }
    Value::Array(rows)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chroma_query_batches_keep_ids_metadata_and_distances_aligned() {
        let rows = chroma_rows(
            json!({"ids":[["a","b"],["c"]],"documents":[["A","B"],["C"]],"distances":[[0.1,0.2],[0.3]]}),
        );
        assert_eq!(rows.as_array().unwrap().len(), 3);
        assert_eq!(rows[2]["id"], "c");
        assert_eq!(rows[1]["document"], "B");
        assert_eq!(rows[1]["distance"], 0.2);
    }
}
