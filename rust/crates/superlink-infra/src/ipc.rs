use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Map, Value, json};

const MAX_LINE: usize = 8 << 20;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("driver agent failed to start: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("driver agent I/O failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("driver agent frame exceeds 8 MiB")]
    FrameTooLarge,
    #[error("driver agent returned an invalid frame")]
    InvalidFrame,
    #[error("driver agent request id mismatch")]
    IdMismatch,
    #[error("{0}")]
    Remote(String),
}

pub type AgentResult<T> = Result<T, AgentError>;

/// One decoded response frame. Query rows and their column order travel
/// separately on the wire, so the envelope is kept intact.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentFrame {
    pub data: Value,
    pub fields: Vec<String>,
    pub messages: Vec<String>,
    pub rows_affected: i64,
    pub truncated: bool,
    pub budget_exhausted: bool,
    pub outcome_unknown: bool,
}

/// One database driver running out of process. The wire format is the same
/// JSON-lines protocol the Go agents already speak: one request object per
/// line, exactly one response object per line, correlated by an integer id.
pub struct DriverAgent {
    binary: PathBuf,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl DriverAgent {
    pub fn spawn(binary: impl AsRef<Path>, args: &[String]) -> AgentResult<Self> {
        let binary = binary.as_ref().to_path_buf();
        let mut child = Command::new(&binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(AgentError::Spawn)?;
        let stdin = child.stdin.take().ok_or(AgentError::InvalidFrame)?;
        let stdout = child.stdout.take().ok_or(AgentError::InvalidFrame)?;
        Ok(Self {
            binary,
            child,
            stdin,
            stdout: BufReader::with_capacity(64 << 10, stdout),
            next_id: 1,
        })
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }

    /// Sends one request and waits for its response. The caller owns timeouts
    /// and cancellation; the agent serialises business requests internally.
    pub fn call(&mut self, method: &str, fields: Map<String, Value>) -> AgentResult<Value> {
        Ok(self.call_frame(method, fields)?.data)
    }

    /// Returns the full envelope: column order, messages and budget flags live
    /// beside the rows rather than inside them.
    pub fn call_frame(
        &mut self,
        method: &str,
        fields: Map<String, Value>,
    ) -> AgentResult<AgentFrame> {
        let id = self.next_id;
        self.next_id += 1;
        let mut request = fields;
        request.insert("id".into(), json!(id));
        request.insert("method".into(), json!(method));
        let line =
            serde_json::to_string(&Value::Object(request)).map_err(|_| AgentError::InvalidFrame)?;
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(AgentError::Io)?;

        let mut response = String::new();
        let read = self
            .stdout
            .read_line(&mut response)
            .map_err(AgentError::Io)?;
        if read == 0 {
            return Err(AgentError::Remote("driver agent closed the stream".into()));
        }
        if response.len() > MAX_LINE {
            return Err(AgentError::FrameTooLarge);
        }
        let parsed: Value =
            serde_json::from_str(&response).map_err(|_| AgentError::InvalidFrame)?;
        let object = parsed.as_object().ok_or(AgentError::InvalidFrame)?;
        if object.get("id").and_then(Value::as_i64) != Some(id) {
            return Err(AgentError::IdMismatch);
        }
        if object.get("success").and_then(Value::as_bool) != Some(true) {
            let message = object
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("driver agent rejected the request")
                .to_string();
            return Err(AgentError::Remote(message));
        }
        Ok(AgentFrame {
            data: object.get("data").cloned().unwrap_or(Value::Null),
            fields: string_list(object.get("fields")),
            messages: string_list(object.get("messages")),
            rows_affected: object
                .get("rowsAffected")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            truncated: object
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            budget_exhausted: object
                .get("budgetExhausted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            outcome_unknown: object
                .get("outcomeUnknown")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    pub fn shutdown(mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

impl Drop for DriverAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
