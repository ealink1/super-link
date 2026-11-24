use crate::{
    http::{Rest, segment},
    sql::Sql,
};
use serde_json::{Value, json};
use superlink_drivers::{ColumnEntry, TableStructure};
impl Sql {
    pub fn object_structure(
        &mut self,
        _database: &str,
        table: &str,
    ) -> Result<TableStructure, String> {
        let fields: Vec<(String, String)> = match self {
            Self::Mongo(d) => {
                let result =
                    d.query(&json!({"find":table,"filter":{},"limit":1}).to_string(), 1)?;
                result
                    .columns
                    .into_iter()
                    .map(|c| (c.name, "JSON".into()))
                    .collect()
            }
            Self::Rest(rest) if rest.config.kind == "elasticsearch" => {
                let value = rest.call(
                    reqwest::Method::GET,
                    &format!("/{}/_mapping", segment(table)),
                    None,
                )?;
                let mut out = vec![];
                for (_, mapping) in value.as_object().ok_or("映射无效")? {
                    mapping_fields("", &mapping["mappings"]["properties"], &mut out)?;
                }
                out
            }
            Self::Rest(rest)
                if ["qdrant", "chroma", "milvus"].contains(&rest.config.kind.as_str()) =>
            {
                vec![
                    ("id".into(), "STRING".into()),
                    ("vector".into(), "VECTOR".into()),
                    ("payload".into(), "JSON".into()),
                ]
            }
            Self::Redis(_) => vec![
                ("key".into(), "STRING".into()),
                ("value".into(), "JSON".into()),
            ],
            Self::Kafka(_) | Self::Pulsar(_) | Self::RocketMq(_) | Self::Mqtt(_) => vec![
                ("topic".into(), "STRING".into()),
                ("value".into(), "STRING".into()),
                ("timestamp".into(), "TIMESTAMP".into()),
            ],
            Self::Rest(rest) if rest.config.kind == "nacos" => vec![
                ("dataId".into(), "STRING".into()),
                ("group".into(), "STRING".into()),
                ("content".into(), "TEXT".into()),
            ],
            Self::Rest(rest) if rest.config.kind == "rabbitmq" => vec![
                ("payload".into(), "STRING".into()),
                ("routing_key".into(), "STRING".into()),
            ],
            Self::Rest(rest) => {
                let result = rest.query(
                    &format!("DESCRIBE {}", crate::sql_catalog::qualified(table)),
                    4096,
                )?;
                result
                    .rows
                    .into_iter()
                    .map(|r| {
                        (
                            r.first().and_then(Value::as_str).unwrap_or("").into(),
                            r.get(1).and_then(Value::as_str).unwrap_or("").into(),
                        )
                    })
                    .collect()
            }
            _ => return Err("目录类型无效".into()),
        };
        Ok(TableStructure {
            columns: fields
                .into_iter()
                .map(|(name, data_type)| ColumnEntry {
                    name,
                    data_type,
                    nullable: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
    }
}
impl Rest {
    pub fn object_definition(&self, table: &str) -> Result<String, String> {
        let value = match self.config.kind.as_str() {
            "qdrant" => self.call(
                reqwest::Method::GET,
                &format!("/collections/{}", segment(table)),
                None,
            )?,
            "milvus" => self.call(
                reqwest::Method::POST,
                "/v2/vectordb/collections/describe",
                Some(&json!({"dbName":self.config.database,"collectionName":table})),
            )?,
            "chroma" => {
                return Ok(json!({"name":table,"database":self.config.database}).to_string());
            }
            _ => return Err("对象类型无效".into()),
        };
        serde_json::to_string_pretty(&value).map_err(|_| "对象定义编码失败".into())
    }
}
fn mapping_fields(
    prefix: &str,
    properties: &Value,
    output: &mut Vec<(String, String)>,
) -> Result<(), String> {
    if prefix.matches('.').count() > 32 {
        return Err("映射嵌套超过 32 层".into());
    }
    if let Some(properties) = properties.as_object() {
        for (name, value) in properties {
            if output.len() >= 4096 {
                return Err("映射列超过 4096".into());
            }
            let name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}.{name}")
            };
            output.push((
                name.clone(),
                value["type"].as_str().unwrap_or("object").into(),
            ));
            mapping_fields(&name, &value["properties"], output)?;
        }
    }
    Ok(())
}
