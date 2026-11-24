//! Separate catalog scopes from transport service names and message defaults.
use superlink_domain::ConnectionConfig;
pub(crate) fn apply(config: &mut ConnectionConfig, scope: &str) -> Result<(), String> {
    if scope.is_empty()
        || matches!(config.kind.as_str(), "sqlite" | "duckdb")
        || config.kind == "custom"
            && matches!(config.driver.as_str(), "sqlite" | "sqlite3" | "duckdb")
        || matches!(
            config.kind.as_str(),
            "kafka" | "mqtt" | "pulsar" | "rocketmq"
        )
    {
        return Ok(());
    }
    let oracle = config.kind == "oracle"
        || config.kind == "oceanbase" && config.oceanbase_protocol.eq_ignore_ascii_case("oracle")
        || config.kind == "custom"
            && ["godror", "oracle", "go-ora", "oci8"].contains(&config.driver.as_str());
    if oracle && scope == config.database {
        return Ok(());
    }
    if oracle {
        config
            .extra
            .insert("rustSchema".into(), serde_json::json!(scope));
    } else {
        config.database = scope.into();
        if config.kind == "redis" {
            config.redis_db = scope.parse().map_err(|_| "Redis 数据库编号无效")?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oracle_schema_selection_does_not_replace_the_service_name() {
        let mut c = ConnectionConfig {
            kind: "oceanbase".into(),
            oceanbase_protocol: "Oracle".into(),
            database: "service_name".into(),
            ..Default::default()
        };
        apply(&mut c, "OWNER").unwrap();
        assert_eq!(c.database, "service_name");
        assert_eq!(c.extra["rustSchema"], "OWNER");
    }
    #[test]
    fn virtual_message_catalog_does_not_replace_configured_topic() {
        let mut c = ConnectionConfig {
            kind: "mqtt".into(),
            database: "devices/#".into(),
            ..Default::default()
        };
        apply(&mut c, "topics").unwrap();
        assert_eq!(c.database, "devices/#");
    }
    #[test]
    fn duckdb_catalog_selection_preserves_the_database_file_path() {
        let mut c = ConnectionConfig {
            kind: "duckdb".into(),
            database: "/private/work/test.duckdb".into(),
            ..Default::default()
        };
        apply(&mut c, "test").unwrap();
        assert_eq!(c.database, "/private/work/test.duckdb");
    }
}
