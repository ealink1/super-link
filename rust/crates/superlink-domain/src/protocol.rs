//! Protocol form commands. Generation never executes or publishes a command.
use serde_json::{Value, json};

#[allow(clippy::too_many_arguments)]
pub fn command(
    kind: &str,
    operation: &str,
    target: &str,
    input: &str,
    group: &str,
    scope: &str,
    limit: u32,
) -> Result<String, String> {
    if target.len() > 4096 || input.len() > 65536 || group.len() > 4096 {
        return Err("输入超出命令大小限制".into());
    }
    let limit = limit.clamp(1, 100);
    let value = match kind {
        "redis" => {
            if ![
                "GET", "HGETALL", "LRANGE", "SMEMBERS", "ZRANGE", "XRANGE", "TYPE", "TTL",
            ]
            .contains(&operation)
            {
                return Err("请选择读取命令".into());
            }
            let args = match operation {
                "LRANGE" | "ZRANGE" => " 0 99",
                "XRANGE" => " - + COUNT 100",
                _ => "",
            };
            return Ok(format!(
                "{operation} '{}'{args}",
                target.replace('\'', "'\"'\"'")
            ));
        }
        "mongodb" => {
            let filter: Value =
                serde_json::from_str(if input.trim().is_empty() { "{}" } else { input })
                    .map_err(|_| "Filter JSON 无效")?;
            if !filter.is_object() {
                return Err("Filter 必须是 JSON 对象".into());
            }
            json!({"find":target,"filter":filter,"limit":100})
        }
        "chroma" | "qdrant" | "milvus" => match operation {
            "list" => json!({"list_collections":true}),
            "preview" if kind == "qdrant" => json!({"scroll":target,"limit":100}),
            "preview" => json!({"get":target,"limit":100}),
            _ => {
                let vector: Vec<f64> =
                    serde_json::from_str(input).map_err(|_| "请填写非空数值向量")?;
                if vector.is_empty()
                    || vector.len() > 65536
                    || vector.iter().any(|n| !n.is_finite())
                {
                    return Err("向量维度或数值无效".into());
                }
                if kind == "chroma" {
                    json!({"query":target,"query_embeddings":[vector],"n_results":10})
                } else {
                    json!({"search":target,"vector":vector,"limit":10})
                }
            }
        },
        "nacos" => {
            let group = if group.is_empty() {
                "DEFAULT_GROUP"
            } else {
                group
            };
            if operation == "publish" {
                json!({"op":"publish","namespace":scope,"group":group,"dataId":target,"content":input,"type":"text"})
            } else {
                json!({"op":operation,"namespace":scope,"group":group,"dataId":target,"service":target,"page":1})
            }
        }
        "kafka" | "rocketmq" | "pulsar" | "mqtt" | "rabbitmq" => {
            if operation == "read" {
                return Ok(format!(
                    "SELECT * FROM {}{} LIMIT {limit}",
                    serde_json::to_string(target).map_err(|_| "对象名称无效")?,
                    if ["kafka", "rocketmq", "pulsar"].contains(&kind) {
                        " LATEST"
                    } else {
                        ""
                    }
                ));
            }
            let mut message = json!({"publish":target,"payload":input});
            if ["kafka", "pulsar"].contains(&kind) {
                message["value"] = json!(input);
            }
            if kind == "mqtt" {
                message["qos"] = json!(0);
                message["retain"] = json!(false);
            }
            message
        }
        _ => return Err("此数据源不需要协议表单".into()),
    };
    serde_json::to_string_pretty(&value).map_err(|_| "无法生成命令".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_filters_and_vectors_and_keeps_protocol_fields() {
        assert!(command("mongodb", "read", "c", "[]", "", "", 10).is_err());
        assert!(command("milvus", "search", "c", "[]", "", "", 10).is_err());
        let q: Value = serde_json::from_str(
            &command("chroma", "search", "中文", "[0.1,0.2]", "", "", 10).unwrap(),
        )
        .unwrap();
        assert_eq!(q["query_embeddings"][0][1], 0.2);
        let q: Value =
            serde_json::from_str(&command("nacos", "get", "a", "", "CUSTOM", "ns", 10).unwrap())
                .unwrap();
        assert_eq!(q["group"], "CUSTOM");
        assert_eq!(q["namespace"], "ns");
        assert!(
            command("kafka", "read", "a\"b", "", "", "", 1000)
                .unwrap()
                .ends_with("LATEST LIMIT 100")
        );
        assert!(command("redis", "EVAL", "a", "", "", "", 10).is_err());
    }
}
