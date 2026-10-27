use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::catalog::resolve;
use crate::error::{DomainError, DomainResult};

/// Driver-neutral connection settings. Field names follow the Go JSON tags so
/// existing profile rows deserialize unchanged. Unknown keys are preserved in
/// the extra map so a newer Go build can still read what this app writes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(rename = "savePassword", default, skip_serializing_if = "is_false")]
    pub save_password: bool,
    #[serde(default)]
    pub database: String,
    #[serde(rename = "readOnly", default, skip_serializing_if = "is_false")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<Value>,
    #[serde(rename = "useSSL", default, skip_serializing_if = "is_false")]
    pub use_ssl: bool,
    #[serde(rename = "sslMode", default, skip_serializing_if = "String::is_empty")]
    pub ssl_mode: String,
    #[serde(
        rename = "sslCAPath",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub ssl_ca_path: String,
    #[serde(
        rename = "sslCertPath",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub ssl_cert_path: String,
    #[serde(
        rename = "sslKeyPath",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub ssl_key_path: String,
    #[serde(rename = "useSSH", default, skip_serializing_if = "is_false")]
    pub use_ssh: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<Value>,
    #[serde(rename = "useProxy", default, skip_serializing_if = "is_false")]
    pub use_proxy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<Value>,
    #[serde(rename = "useHttpTunnel", default, skip_serializing_if = "is_false")]
    pub use_http_tunnel: bool,
    #[serde(
        rename = "httpTunnel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub http_tunnel: Option<Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dsn: String,
    #[serde(
        rename = "connectionParams",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub connection_params: String,
    #[serde(default)]
    pub timeout: u32,
    #[serde(rename = "queryTimeout", default)]
    pub query_timeout: u32,
    #[serde(rename = "keepAliveEnabled", default, skip_serializing_if = "is_false")]
    pub keep_alive_enabled: bool,
    #[serde(rename = "keepAliveIntervalMinutes", default)]
    pub keep_alive_interval_minutes: u32,
    #[serde(
        rename = "keepAliveSQL",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub keep_alive_sql: String,
    #[serde(rename = "redisDB", default, skip_serializing_if = "is_zero")]
    pub redis_db: u8,
    #[serde(
        rename = "redisSentinelMaster",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub redis_sentinel_master: String,
    #[serde(
        rename = "redisSentinelUser",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub redis_sentinel_user: String,
    #[serde(
        rename = "redisSentinelPassword",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub redis_sentinel_password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uri: String,
    #[serde(
        rename = "clickHouseProtocol",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub clickhouse_protocol: String,
    #[serde(
        rename = "oceanBaseProtocol",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub oceanbase_protocol: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub topology: String,
    #[serde(
        rename = "mysqlReplicaUser",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub mysql_replica_user: String,
    #[serde(
        rename = "mysqlReplicaPassword",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub mysql_replica_password: String,
    #[serde(
        rename = "replicaSet",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub replica_set: String,
    #[serde(
        rename = "authSource",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub auth_source: String,
    #[serde(
        rename = "readPreference",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub read_preference: String,
    #[serde(rename = "mongoSrv", default, skip_serializing_if = "is_false")]
    pub mongo_srv: bool,
    #[serde(
        rename = "mongoAuthMechanism",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub mongo_auth_mechanism: String,
    #[serde(
        rename = "mongoReplicaUser",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub mongo_replica_user: String,
    #[serde(
        rename = "mongoReplicaPassword",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub mongo_replica_password: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

impl ConnectionConfig {
    /// Removes every credential before metadata is persisted or serialized.
    /// Secrets belong to the vault and are addressed by Profile::secret_ref.
    pub fn without_secrets(mut self) -> Self {
        self.password.clear();
        self.mysql_replica_password.clear();
        self.mongo_replica_password.clear();
        self.redis_sentinel_password.clear();
        if let Some(ssh) = self.ssh.as_mut() {
            redact_object(ssh, &["password", "passphrase", "privateKey"]);
        }
        if let Some(proxy) = self.proxy.as_mut() {
            redact_object(proxy, &["password"]);
        }
        if let Some(tunnel) = self.http_tunnel.as_mut() {
            redact_object(tunnel, &["password"]);
        }
        self
    }
}

fn redact_object(value: &mut Value, keys: &[&str]) {
    if let Some(object) = value.as_object_mut() {
        for key in keys {
            if object.contains_key(*key) {
                object.insert((*key).to_string(), Value::String(String::new()));
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(rename = "createdAt", default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "group", default)]
    pub group: String,
    #[serde(default)]
    pub environment: String,
    #[serde(rename = "iconType", default, skip_serializing_if = "String::is_empty")]
    pub icon_type: String,
    #[serde(
        rename = "iconColor",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub icon_color: String,
    #[serde(
        rename = "databaseAllow",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub database_allow: Vec<String>,
    #[serde(
        rename = "databaseInclude",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub database_include: Vec<String>,
    #[serde(
        rename = "databaseExclude",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub database_exclude: Vec<String>,
    #[serde(rename = "readOnly", default)]
    pub read_only: bool,
    #[serde(default)]
    pub revision: i64,
    #[serde(skip)]
    pub secret_ref: String,
    #[serde(default)]
    pub config: ConnectionConfig,
    #[serde(skip)]
    pub scope: String,
}

impl Profile {
    /// OceanBase exposes two compatibility modes; everything else is the type key.
    pub fn sql_dialect(&self) -> &str {
        if self.config.kind == "custom" {
            return match self.config.driver.as_str() {
                "pgx" | "pq" | "postgres" => "postgres",
                "mysql" => "mysql",
                "sqlserver" => "sqlserver",
                "sqlite" | "sqlite3" => "sqlite",
                "godror" => "oracle",
                _ => self.config.kind.as_str(),
            };
        }
        if self.config.kind == "oceanbase"
            && self
                .config
                .oceanbase_protocol
                .eq_ignore_ascii_case("oracle")
        {
            return "oracle";
        }
        self.config.kind.as_str()
    }

    pub fn visible_databases(&self, names: &[String]) -> Vec<String> {
        names
            .iter()
            .filter(|name| {
                if !self.database_allow.is_empty()
                    && !self.database_allow.iter().any(|pattern| pattern == *name)
                {
                    return false;
                }
                if !self.database_include.is_empty()
                    && !self
                        .database_include
                        .iter()
                        .any(|pattern| match_name(pattern, name))
                {
                    return false;
                }
                !self
                    .database_exclude
                    .iter()
                    .any(|pattern| match_name(pattern, name))
            })
            .cloned()
            .collect()
    }

    pub fn validate_presentation(&self) -> DomainResult<()> {
        if !self.icon_color.is_empty() {
            let valid = Regex::new(r"^#[0-9a-fA-F]{6}$")
                .map(|re| re.is_match(&self.icon_color))
                .unwrap_or(false);
            if !valid {
                return Err(DomainError::validation("invalid connection icon color"));
            }
        }
        if !self.icon_type.is_empty() {
            resolve(&self.icon_type)?;
        }
        for values in [
            &self.database_allow,
            &self.database_include,
            &self.database_exclude,
        ] {
            if values.len() > 100 {
                return Err(DomainError::validation(
                    "at most 100 database display filters are allowed",
                ));
            }
            if values.iter().any(|value| value.len() > 256) {
                return Err(DomainError::validation("database filter exceeds 256 bytes"));
            }
        }
        Ok(())
    }
}

/// Glob matching with the Go semantics: * and % match any run, _ matches one
/// character, and a backslash escapes the next character.
fn match_name(pattern: &str, name: &str) -> bool {
    let mut expression = String::with_capacity(pattern.len() + 4);
    expression.push('^');
    let mut escape = false;
    for ch in pattern.chars() {
        if escape {
            expression.push_str(&regex::escape(&ch.to_string()));
            escape = false;
            continue;
        }
        match ch {
            '\\' => escape = true,
            '*' | '%' => expression.push_str(".*"),
            '_' => expression.push('.'),
            other => expression.push_str(&regex::escape(&other.to_string())),
        }
    }
    if escape {
        expression.push_str(&regex::escape("\\"));
    }
    expression.push('$');
    Regex::new(&expression)
        .map(|re| re.is_match(name))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_prefers_exact_allow_list() {
        let profile = Profile {
            database_allow: vec!["prod".into(), "stage".into()],
            database_exclude: vec!["stage*".into()],
            ..Default::default()
        };
        let names = [
            "prod".to_string(),
            "stage".to_string(),
            "stage_2".to_string(),
        ];
        assert_eq!(profile.visible_databases(&names), vec!["prod".to_string()]);
    }

    #[test]
    fn glob_matches_single_and_multi_character() {
        assert!(match_name("app_%", "app_prod"));
        assert!(!match_name("app?", "app1"));
        assert!(match_name(r"a\*b", "a*b"));
    }

    #[test]
    fn oceanbase_oracle_mode_reports_oracle_dialect() {
        let profile = Profile {
            config: ConnectionConfig {
                kind: "oceanbase".into(),
                oceanbase_protocol: "Oracle".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(profile.sql_dialect(), "oracle");
    }

    #[test]
    fn secrets_are_stripped_before_persistence() {
        let config = ConnectionConfig {
            password: "hunter2".into(),
            ssh: Some(serde_json::json!({"password": "x", "host": "h"})),
            ..Default::default()
        };
        let safe = config.without_secrets();
        assert!(safe.password.is_empty());
        let ssh = safe.ssh.unwrap();
        assert_eq!(ssh["password"], "");
        assert_eq!(ssh["host"], "h");
    }

    #[test]
    fn unknown_config_keys_round_trip() {
        let raw = r#"{"type":"mysql","futureFlag":true,"port":3306}"#;
        let config: ConnectionConfig = serde_json::from_str(raw).unwrap();
        let encoded = serde_json::to_string(&config).unwrap();
        assert!(encoded.contains("futureFlag"));
    }
}
