mod elasticsearch;
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
    search_index: String,
    session_id: String,
    agent: DriverAgent,
    kind: String,
    revision: String,
    connect: ConnectInfo,
    timeout_ms: u64,
}

impl DriverSession {
    pub fn catalog(
        &mut self,
        method: &str,
        database: &str,
        table: &str,
    ) -> Result<Value, DriverError> {
        Ok(self.agent.call(
            method,
            named([
                ("dbName", json!(database)),
                ("tableName", json!(table)),
                ("timeoutMs", json!(self.timeout_ms)),
            ]),
        )?)
    }
    pub fn set_cancellation(&self, cancel: &superlink_infra::QueryCancel) {
        self.agent.set_cancellation(cancel);
    }
    /// Spawns the agent and verifies its identity before any user data is sent.
    pub fn open(binary: impl AsRef<Path>, kind: &str) -> Result<Self, DriverError> {
        Self::open_with_cancel(binary, kind, &superlink_infra::QueryCancel::new())
    }
    pub fn open_with_cancel(
        binary: impl AsRef<Path>,
        kind: &str,
        cancel: &superlink_infra::QueryCancel,
    ) -> Result<Self, DriverError> {
        let mut agent = DriverAgent::spawn(binary, &[])?;
        agent.set_cancellation(cancel);
        let data = agent.call("metadata", Map::new())?;
        let metadata: Metadata =
            serde_json::from_value(data).map_err(|_| DriverError::Malformed("metadata"))?;
        if metadata.driver_type != kind {
            return Err(DriverError::Identity {
                kind: kind.to_string(),
                actual: metadata.driver_type,
            });
        }
        if metadata.protocol_schema != PROTOCOL {
            return Err(DriverError::Protocol {
                kind: kind.to_string(),
                actual: metadata.protocol_schema,
            });
        }
        if metadata.revision.is_empty() {
            return Err(DriverError::Malformed("driver revision"));
        }
        Ok(Self {
            search_index: String::new(),
            session_id: String::new(),
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
        self.search_index = config.database.clone();
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
        self.validate_arguments(args)?;
        let mut fields = query_request(sql, args, max_rows, self.timeout_ms);
        if !self.session_id.is_empty() {
            fields.insert("sessionId".into(), json!(self.session_id));
        }
        let frame = self.agent.call_frame("query", fields)?;
        Ok(to_result_set(frame))
    }

    pub fn close(&mut self) -> Result<(), DriverError> {
        if !self.session_id.is_empty() {
            self.agent.call(
                "closeSession",
                named([("sessionId", json!(self.session_id))]),
            )?;
            self.session_id.clear();
        }
        self.agent.call("close", Map::new())?;
        Ok(())
    }

    pub fn open_sql_session(&mut self) -> Result<bool, DriverError> {
        match self.agent.call(
            "openSession",
            named([("timeoutMs", json!(self.timeout_ms))]),
        ) {
            Ok(value) => {
                self.session_id = value
                    .as_str()
                    .ok_or(DriverError::Malformed("SQL session"))?
                    .into();
                Ok(true)
            }
            Err(AgentError::Remote(error)) if error.contains("不支持 SQL 编辑器托管事务") => {
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }
    /// Delivers bounded frames directly to a consumer, retaining no earlier rows.
    pub fn stream_query(
        &mut self,
        sql: &str,
        mut consume: impl FnMut(&[String], &[Vec<Value>]) -> Result<(), String>,
    ) -> Result<u64, DriverError> {
        let mut fields = query_request(sql, &[], 0, self.timeout_ms);
        if !self.session_id.is_empty() {
            fields.insert("sessionId".into(), json!(self.session_id));
        }
        let id = self.agent.send_request("streamQuery", fields)?;
        let mut columns = Vec::new();
        let mut count = 0u64;
        loop {
            let frame = self.agent.read_frame(id)?;
            match frame.chunk_type.as_str() {
                "columns" if columns.is_empty() && !frame.fields.is_empty() => {
                    columns = frame.fields;
                    consume(&columns, &[]).map_err(DriverError::Native)?;
                }
                "rows" if !columns.is_empty() => {
                    let mut frame = frame;
                    frame.fields = columns.clone();
                    let result = to_result_set(frame);
                    count = count.saturating_add(result.rows.len() as u64);
                    consume(&columns, &result.rows).map_err(DriverError::Native)?;
                }
                "done" if !columns.is_empty() => return Ok(count),
                _ => return Err(DriverError::Malformed("stream frame")),
            }
        }
    }

    fn validate_arguments(&self, args: &[Value]) -> Result<(), DriverError> {
        if !args.is_empty() && !self.connect.typed_arguments {
            return Err(DriverError::Malformed(
                "driver does not support typed parameters",
            ));
        }
        crate::arguments::wire(args).map(|_| ())
    }
    pub fn has_sql_session(&self) -> bool {
        !self.session_id.is_empty()
    }
    pub fn exec(&mut self, sql: &str, args: &[Value]) -> Result<ResultSet, DriverError> {
        self.validate_arguments(args)?;
        let mut fields = query_request(sql, args, 0, self.timeout_ms);
        if !self.session_id.is_empty() {
            fields.insert("sessionId".into(), json!(self.session_id));
        }
        Ok(to_result_set(self.agent.call_frame("exec", fields)?))
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
    if !args.is_empty()
        && let Ok((values, types)) = crate::arguments::wire(args)
    {
        fields.insert("args".into(), json!(values));
        fields.insert("argTypes".into(), json!(types));
    }
    if max_rows > 0 {
        fields.insert(
            "rowBudget".into(),
            json!({ "maxRowsPerResult": max_rows, "maxTotalRows": max_rows, "maxTotalBytes": 24<<20, "maxFieldBytes": 1<<20 }),
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
            "{}/../../../bin/drivers/sqlite-driver-agent",
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
    fn streams_more_than_the_display_budget_on_one_sql_session() {
        let Some(binary) = sqlite_agent() else {
            return;
        };
        let mut session = DriverSession::open(binary, "sqlite").unwrap();
        session
            .connect(&ConnectionConfig {
                kind: "sqlite".into(),
                database: ":memory:".into(),
                ..Default::default()
            })
            .unwrap();
        assert!(session.open_sql_session().unwrap());
        session.exec("CREATE TABLE t(a INTEGER)", &[]).unwrap();
        session.exec("BEGIN", &[]).unwrap();
        session.exec("INSERT INTO t VALUES (7)", &[]).unwrap();
        session.exec("ROLLBACK", &[]).unwrap();
        assert_eq!(
            session
                .query("SELECT count(*) n FROM t", &[], 1)
                .unwrap()
                .rows[0][0],
            json!(0)
        );
        let mut count = 0;
        let rows = session.stream_query("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<2500) SELECT x FROM n", |columns, rows| {
            assert_eq!(columns, &["x"]);
            assert!(rows.len() <= 256);
            count += rows.len();
            Ok(())
        }).unwrap();
        assert_eq!(rows, 2500);
        assert_eq!(count, 2500);
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
