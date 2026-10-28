use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value, json};
use superlink_domain::{Column, ConnectionConfig, ResultSet};
use superlink_infra::{AgentError, AgentFrame, DriverAgent};

use crate::registry::PROTOCOL;

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error("driver {kind} speaks {actual}, expected {PROTOCOL}")]
    Protocol { kind: String, actual: String },
    #[error("driver {kind} identified itself as {actual}")]
    Identity { kind: String, actual: String },
    #[error("driver returned {0}")]
    Malformed(&'static str),
    #[error("native driver: {0}")]
    Native(String),
}

/// Capabilities reported by connect; they gate parameter binding and cancel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConnectInfo {
    pub protocol_schema: String,
    pub in_flight_cancel: bool,
    pub typed_arguments: bool,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    #[serde(rename = "driverType", default)]
    driver_type: String,
    #[serde(rename = "agentRevision", default)]
    revision: String,
    #[serde(rename = "protocolSchema", default)]
    protocol_schema: String,
}

/// A connected driver process. One session owns one child process; dropping it
/// kills the child so a crashed query never leaves an orphan.
pub struct DriverSession {
    agent: DriverAgent,
    kind: String,
    revision: String,
    connect: ConnectInfo,
    timeout_ms: u64,
}

impl DriverSession {
    /// Spawns the agent and verifies its identity before any user data is sent.
    pub fn open(binary: impl AsRef<Path>, kind: &str) -> Result<Self, DriverError> {
        let mut agent = DriverAgent::spawn(binary, &[])?;
        let data = agent.call("metadata", Map::new())?;
        let metadata: Metadata =
            serde_json::from_value(data).map_err(|_| DriverError::Malformed("metadata"))?;
        if !metadata.driver_type.is_empty() && metadata.driver_type != kind {
            return Err(DriverError::Identity {
                kind: kind.to_string(),
                actual: metadata.driver_type,
            });
        }
        if !metadata.protocol_schema.is_empty() && metadata.protocol_schema != PROTOCOL {
            return Err(DriverError::Protocol {
                kind: kind.to_string(),
                actual: metadata.protocol_schema,
            });
        }
        Ok(Self {
            agent,
            kind: kind.to_string(),
            revision: metadata.revision,
            connect: ConnectInfo::default(),
            timeout_ms: 0,
        })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn connect_info(&self) -> &ConnectInfo {
        &self.connect
    }

    /// Sends the connection config. Credentials travel over the pipe and are
    /// never persisted or logged by this layer.
    pub fn connect(&mut self, config: &ConnectionConfig) -> Result<&ConnectInfo, DriverError> {
        // Go 侧语义：queryTimeout 为秒，0 表示不设自动截止。
        self.timeout_ms = u64::from(config.query_timeout).saturating_mul(1000);
        let config = serde_json::to_value(config).map_err(|_| DriverError::Malformed("config"))?;
        let data = self.agent.call("connect", named([("config", config)]))?;
        let object = data.as_object();
        self.connect = ConnectInfo {
            protocol_schema: object
                .and_then(|item| item.get("protocolSchema"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            in_flight_cancel: object
                .and_then(|item| item.get("inFlightCancel"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            typed_arguments: object
                .and_then(|item| item.get("typedArguments"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        Ok(&self.connect)
    }

    /// Runs one statement and materializes at most max_rows rows.
    pub fn query(
        &mut self,
        sql: &str,
        args: &[Value],
        max_rows: u32,
    ) -> Result<ResultSet, DriverError> {
        let fields = query_request(sql, args, max_rows, self.timeout_ms);
        let frame = self.agent.call_frame("query", fields)?;
        Ok(to_result_set(frame))
    }

    pub fn close(&mut self) -> Result<(), DriverError> {
        self.agent.call("close", Map::new())?;
        Ok(())
    }
}

fn named<const N: usize>(pairs: [(&str, Value); N]) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// 纯函数构建 query 请求帧，便于单测覆盖超时与预算字段。
fn query_request(sql: &str, args: &[Value], max_rows: u32, timeout_ms: u64) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("query".into(), json!(sql));
    if !args.is_empty() {
        fields.insert("args".into(), json!(args));
    }
    if max_rows > 0 {
        fields.insert(
            "rowBudget".into(),
            json!({ "maxRowsPerResult": max_rows, "maxTotalRows": max_rows }),
        );
    }
    if timeout_ms > 0 {
        fields.insert("timeoutMs".into(), json!(timeout_ms));
    }
    fields
}

#[cfg(test)]
mod query_request_tests {
    use super::*;

    #[test]
    fn includes_budget_and_timeout_only_when_set() {
        let without = query_request("SELECT 1", &[], 0, 0);
        assert!(without.get("rowBudget").is_none());
        assert!(without.get("timeoutMs").is_none());

        let with = query_request("SELECT 1", &[], 100, 5_000);
        assert_eq!(with["rowBudget"]["maxRowsPerResult"], 100);
        assert_eq!(with["timeoutMs"], 5_000);
        assert_eq!(with["query"], "SELECT 1");
    }
}

/// Rows arrive as objects keyed by column name; the envelope owns the order.
fn to_result_set(frame: AgentFrame) -> ResultSet {
    let mut columns: Vec<Column> = frame
        .fields
        .iter()
        .map(|name| Column {
            id: name.clone(),
            name: name.clone(),
        })
        .collect();
    let rows = match &frame.data {
        Value::Array(items) => items
            .iter()
            .map(|row| match row {
                Value::Object(object) => {
                    if columns.is_empty() {
                        columns = object
                            .keys()
                            .map(|name| Column {
                                id: name.clone(),
                                name: name.clone(),
                            })
                            .collect();
                    }
                    columns
                        .iter()
                        .map(|column| object.get(&column.name).cloned().unwrap_or(Value::Null))
                        .collect()
                }
                Value::Array(cells) => cells.clone(),
                other => vec![other.clone()],
            })
            .collect(),
        _ => Vec::new(),
    };
    ResultSet {
        columns,
        rows,
        messages: frame.messages,
        rows_affected: frame.rows_affected,
        truncated: frame.truncated || frame.budget_exhausted,
        duration_ns: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite_agent() -> Option<String> {
        let path = format!(
            "{}/../../bin/drivers/sqlite-driver-agent",
            env!("CARGO_MANIFEST_DIR")
        );
        Path::new(&path).is_file().then_some(path)
    }

    #[test]
    fn maps_columns_and_rows_into_domain_types() {
        let frame = AgentFrame {
            data: json!([{"a": 1, "b": "x"}, {"a": 2, "b": "y"}]),
            fields: vec!["a".into(), "b".into()],
            ..Default::default()
        };
        let result = to_result_set(frame);
        assert_eq!(result.columns.len(), 2);
        assert_eq!(
            result.rows,
            vec![vec![json!(1), json!("x")], vec![json!(2), json!("y")]]
        );
    }

    #[test]
    fn falls_back_to_object_keys_when_fields_are_missing() {
        let frame = AgentFrame {
            data: json!([{"a": 1}]),
            ..Default::default()
        };
        let result = to_result_set(frame);
        assert_eq!(result.columns[0].name, "a");
    }

    /// End-to-end against the real SQLite agent that ships with the repository.
    #[test]
    fn runs_a_round_trip_on_the_sqlite_agent() {
        let Some(binary) = sqlite_agent() else {
            eprintln!("skipping: no sqlite driver agent in this checkout");
            return;
        };
        let database =
            std::env::temp_dir().join(format!("superlink-driver-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&database);
        let config = ConnectionConfig {
            kind: "sqlite".into(),
            database: database.to_string_lossy().to_string(),
            ..Default::default()
        };

        let mut session = DriverSession::open(&binary, "sqlite").unwrap();
        session.connect(&config).unwrap();
        assert_eq!(session.connect_info().protocol_schema, PROTOCOL);
        session
            .query("CREATE TABLE t(a INTEGER, b TEXT)", &[], 100)
            .unwrap();
        session
            .query("INSERT INTO t VALUES (1,'x'),(2,'y')", &[], 100)
            .unwrap();
        let result = session
            .query("SELECT a,b FROM t ORDER BY a", &[], 100)
            .unwrap();
        assert_eq!(
            result
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(
            result.rows,
            vec![vec![json!(1), json!("x")], vec![json!(2), json!("y")]]
        );
        session.close().unwrap();
        let _ = std::fs::remove_file(&database);
    }

    #[test]
    fn refuses_a_binary_that_is_not_the_requested_driver() {
        let Some(binary) = sqlite_agent() else {
            return;
        };
        assert!(matches!(
            DriverSession::open(&binary, "mysql"),
            Err(DriverError::Identity { .. })
        ));
    }
}
