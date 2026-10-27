use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Column {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// One ordered result set. Rows are already bounded by the caller.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultSet {
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub rows: Vec<Vec<Value>>,
    #[serde(default)]
    pub messages: Vec<String>,
    #[serde(rename = "rowsAffected", default)]
    pub rows_affected: i64,
    #[serde(default)]
    pub truncated: bool,
    #[serde(rename = "durationNs", default)]
    pub duration_ns: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub value: Value,
}

/// A fully prepared statement. Values never get interpolated into SQL text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Execution {
    pub revision: i64,
    #[serde(default)]
    pub parameters: Map<String, Value>,
    #[serde(default)]
    pub args: Vec<Value>,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub scope: String,
    #[serde(rename = "maxRows", default)]
    pub max_rows: u32,
    #[serde(default)]
    pub write: bool,
    #[serde(default)]
    pub confirmation: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "profileId", default)]
    pub profile_id: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    #[serde(default)]
    pub id: i64,
    #[serde(rename = "profileId", default)]
    pub profile_id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub rows: i64,
    #[serde(rename = "durationNs", default)]
    pub duration_ns: u64,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// An explicitly named SQL document, separate from autosaved drafts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedQuery {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "profileId", default)]
    pub profile_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub revision: i64,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// SQL structure separated from values sent through the driver binder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    #[serde(default)]
    pub sql: String,
    #[serde(default)]
    pub args: Vec<Value>,
    #[serde(rename = "requireOne", default)]
    pub require_one: bool,
}

/// A complete original row travels with each change for optimistic checks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowChange {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub original: Map<String, Value>,
    #[serde(default)]
    pub values: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableChanges {
    #[serde(default)]
    pub import: bool,
    pub object: crate::table::Object,
    pub revision: i64,
    #[serde(default)]
    pub rows: Vec<RowChange>,
    #[serde(default)]
    pub confirmation: String,
}
