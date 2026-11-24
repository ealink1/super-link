//! Read command previews for non-SQL objects, preserving the protocol's syntax.
use crate::{Family, resolve};
use serde_json::json;

pub fn object_read_command(kind: &str, scope: &str, name: &str) -> Option<String> {
    let descriptor = resolve(kind).ok()?;
    let value = match descriptor.family {
        Family::Sql => return None,
        Family::Cache => return Some(format!("GET '{}'", name.replace('\'', "'\"'\"'"))),
        Family::Document => json!({"find":name,"filter":{},"limit":100}),
        Family::Vector if kind == "qdrant" => json!({"scroll":name,"limit":100}),
        Family::Vector => json!({"get":name,"limit":100}),
        Family::Message => {
            return Some(format!(
                "SELECT * FROM {} LIMIT 10",
                serde_json::to_string(name).ok()?
            ));
        }
        Family::Configuration => {
            json!({"op":"get","namespace":scope,"group":"DEFAULT_GROUP","dataId":name})
        }
        Family::Search => {
            let path = name
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"_-.~*".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>();
            return Some(format!(
                "GET /{path}/_search\n{{\"size\":100,\"query\":{{\"match_all\":{{}}}}}}"
            ));
        }
    };
    serde_json::to_string_pretty(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_are_valid_protocol_commands_for_all_non_sql_families() {
        let source = "文档 ' \\\"\n/";
        for kind in ["mongodb", "qdrant", "chroma", "milvus", "nacos"] {
            let text = object_read_command(kind, "namespace", source).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(
                parsed
                    .as_object()
                    .unwrap()
                    .values()
                    .any(|v| v.as_str() == Some(source))
            );
        }
        assert_eq!(
            object_read_command("redis", "0", "a'b").unwrap(),
            "GET 'a'\"'\"'b'"
        );
        assert!(
            object_read_command("elasticsearch", "", source)
                .unwrap()
                .starts_with("GET /%E6")
        );
        assert!(object_read_command("mysql", "", "table").is_none());
    }
}
