//! Fyne's execution guard runs before opening any connection.
use superlink_domain::Profile;
use superlink_files::{cancel::NeverCancelled, sql_stream};
use superlink_sql::lexer::{TokenKind, tokenize};

const MAX_SQL: usize = 1 << 20;

pub fn split_sql(sql: &str, dialect: &str) -> Result<Vec<String>, String> {
    if sql.len() > MAX_SQL {
        return Err("SQL 最多 1 MiB".into());
    }
    let mut statements = Vec::new();
    sql_stream::statements(&NeverCancelled, sql.as_bytes(), dialect, |text, _| {
        if statements.len() >= 1_000 {
            return Err(superlink_files::FileError::invalid(
                "单次最多执行 1,000 条语句",
            ));
        }
        statements.push(text.to_string());
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    if statements.is_empty() {
        return Err("SQL 为空".into());
    }
    Ok(statements)
}

pub fn is_write(sql: &str) -> bool {
    let words: Vec<String> = tokenize(sql)
        .into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::String | TokenKind::Comment))
        .flat_map(|t| {
            t.text
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .filter(|s| !s.is_empty())
                .map(str::to_ascii_uppercase)
                .collect::<Vec<_>>()
        })
        .collect();
    let Some(first) = words.first() else {
        return false;
    };
    if matches!(first.as_str(), "SHOW" | "DESCRIBE" | "DESC") {
        return false;
    }
    if !matches!(
        first.as_str(),
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "VALUES" | "PRAGMA"
    ) {
        return true;
    }
    // Writable CTEs, SELECT INTO and locking reads need the same write gate.
    if words.iter().any(|w| {
        matches!(
            w.as_str(),
            "INSERT"
                | "UPDATE"
                | "DELETE"
                | "MERGE"
                | "CREATE"
                | "ALTER"
                | "DROP"
                | "TRUNCATE"
                | "INTO"
                | "CALL"
                | "COPY"
                | "ATTACH"
                | "DETACH"
        )
    }) {
        return true;
    }
    // PRAGMA assignments and argument-taking pragmas are conservatively writes.
    first == "PRAGMA" && (sql.contains('=') || sql.contains('('))
}

pub fn authorize(
    profile: &Profile,
    sql: &str,
    confirmed: bool,
    action: &str,
) -> Result<(), String> {
    if !requires_write(profile, sql)? {
        return Ok(());
    }
    if profile.read_only || profile.config.read_only {
        return Err("此连接为只读，不能执行写入".into());
    }
    let key = match action {
        "structure" => "restrictStructureEdit",
        "data" => "restrictDataEdit",
        "import" => "restrictDataImport",
        _ => "restrictScriptExecution",
    };
    if profile
        .config
        .protection
        .as_ref()
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        return Err("连接保护已禁止此操作，请先检查连接设置".into());
    }
    if !confirmed {
        return Err("此语句会修改数据或结构，请先预览并确认执行".into());
    }
    Ok(())
}

pub fn requires_write(profile: &Profile, source: &str) -> Result<bool, String> {
    use superlink_domain::{Family, resolve};
    let family = resolve(&profile.config.kind)
        .map(|d| d.family)
        .unwrap_or(Family::Sql);
    match family {
        Family::Sql => Ok(is_write(source)),
        Family::Search => {
            let source = source.trim();
            if source.starts_with('{') {
                serde_json::from_str::<serde_json::Value>(source)
                    .map_err(|_| "Elasticsearch JSON 无效")?;
                return Ok(false);
            }
            let mut words = source.split_whitespace();
            let first = words.next().unwrap_or_default().to_ascii_uppercase();
            if matches!(first.as_str(), "DELETE" | "PUT" | "PATCH") {
                return Ok(true);
            }
            let path = if matches!(first.as_str(), "GET" | "POST" | "HEAD") {
                words.next().unwrap_or_default()
            } else {
                source.split_whitespace().next().unwrap_or_default()
            };
            let endpoint = path.split('?').next().unwrap_or_default();
            if matches!(first.as_str(), "GET" | "HEAD") {
                return Ok(false);
            }
            Ok(!endpoint
                .split('/')
                .any(|p| matches!(p, "_search" | "_count" | "_msearch" | "_sql")))
        }
        Family::Cache => {
            let command = source
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if [
                "KEYS",
                "MONITOR",
                "SUBSCRIBE",
                "PSUBSCRIBE",
                "EVAL",
                "EVALSHA",
                "SCRIPT",
                "CONFIG",
                "SHUTDOWN",
                "AUTH",
                "ACL",
                "MULTI",
                "EXEC",
            ]
            .contains(&command.as_str())
            {
                return Err("此命令不允许在交互控制台执行".into());
            }
            Ok(![
                "PING",
                "GET",
                "MGET",
                "TYPE",
                "TTL",
                "PTTL",
                "EXISTS",
                "STRLEN",
                "GETRANGE",
                "SCAN",
                "HGET",
                "HGETALL",
                "HMGET",
                "HKEYS",
                "HVALS",
                "HLEN",
                "HSCAN",
                "LLEN",
                "LRANGE",
                "LINDEX",
                "SCARD",
                "SISMEMBER",
                "SMEMBERS",
                "SSCAN",
                "ZCARD",
                "ZRANGE",
                "ZSCORE",
                "ZSCAN",
                "XLEN",
                "XRANGE",
                "INFO",
                "DBSIZE",
                "TIME",
                "ECHO",
            ]
            .contains(&command.as_str()))
        }
        Family::Document | Family::Vector | Family::Configuration => {
            let value: serde_json::Value =
                serde_json::from_str(source).map_err(|_| "请输入有效的 JSON 命令")?;
            let Some(object) = value.as_object() else {
                return Err("请输入 JSON 对象命令".into());
            };
            if family == Family::Configuration {
                return Ok(!matches!(
                    object.get("op").and_then(|v| v.as_str()),
                    Some(
                        "get"
                            | "list"
                            | "search"
                            | "services"
                            | "namespaces"
                            | "configs"
                            | "instances"
                    )
                ));
            }
            let writes = [
                "insert",
                "insertone",
                "insertmany",
                "update",
                "updateone",
                "updatemany",
                "delete",
                "deleteone",
                "deletemany",
                "remove",
                "replace",
                "replaceone",
                "drop",
                "upsert",
                "add",
                "publish",
                "create_index",
                "delete_index",
            ];
            if object
                .keys()
                .any(|k| writes.contains(&k.to_ascii_lowercase().as_str()))
            {
                return Ok(true);
            }
            if source.contains("\"$out\"") || source.contains("\"$merge\"") {
                return Ok(true);
            }
            Ok(!object.keys().any(|k| {
                matches!(
                    k.as_str(),
                    "find"
                        | "count"
                        | "aggregate"
                        | "distinct"
                        | "listCollections"
                        | "listIndexes"
                        | "list_collections"
                        | "scroll"
                        | "collStats"
                        | "dbStats"
                        | "serverStatus"
                        | "ping"
                        | "buildInfo"
                        | "hello"
                        | "explain"
                        | "search"
                        | "query"
                        | "get"
                        | "list"
                )
            }))
        }
        Family::Message => {
            if source.trim_start().starts_with('{') {
                let value: serde_json::Value =
                    serde_json::from_str(source).map_err(|_| "消息命令 JSON 无效")?;
                if value.get("publish").is_some() {
                    return Ok(true);
                }
                if profile.config.kind == "rabbitmq"
                    && (value.get("query").is_some()
                        || value
                            .get("op")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|op| matches!(op, "messages" | "peek" | "get")))
                {
                    return Ok(true);
                }
                let op = value.get("op").and_then(|v| v.as_str()).unwrap_or_default();
                return Ok(!matches!(
                    op,
                    "list" | "topics" | "messages" | "peek" | "search" | "get"
                ) && value.get("query").is_none());
            }
            let first = source
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            Ok(!matches!(first.as_str(), "SHOW" | "SELECT")
                || profile.config.kind == "rabbitmq" && first == "SELECT")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guards_writable_ctes_and_explain_analyze() {
        assert!(is_write(
            "WITH x AS (DELETE FROM t RETURNING *) SELECT * FROM x"
        ));
        assert!(is_write("EXPLAIN ANALYZE DELETE FROM t"));
        assert!(!is_write("SELECT 'DELETE FROM t' /* DROP TABLE */"));
        assert!(is_write("SELECT * INTO copied FROM t"));
    }
    #[test]
    fn validates_all_statements_before_any_can_run() {
        let items = split_sql("SELECT ';'; SELECT $$a;b$$;", "postgres").unwrap();
        assert_eq!(items.len(), 2);
        assert!(split_sql("SELECT 1; SELECT 'unclosed", "sqlite").is_err());
        let profile = Profile {
            read_only: true,
            ..Default::default()
        };
        assert!(authorize(&profile, "DELETE FROM t", true, "").is_err());
    }
    #[test]
    fn protocol_reads_and_side_effects_use_the_correct_confirmation_gate() {
        let mut p = Profile::default();
        p.config.kind = "qdrant".into();
        assert!(!requires_write(&p, r#"{"scroll":"c","limit":100}"#).unwrap());
        p.config.kind = "rabbitmq".into();
        assert!(requires_write(&p, "SELECT * FROM queue LIMIT 10").unwrap());
        assert!(!requires_write(&p, "SHOW QUEUES").unwrap());
        p.config.kind = "kafka".into();
        assert!(!requires_write(&p, "SELECT * FROM topic LATEST LIMIT 10").unwrap());
        assert!(requires_write(&p, "DROP TOPIC topic").unwrap());
    }
}
