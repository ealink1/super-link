//! Input and output types shared with the UI layer.
//!
//! The field domains follow the Go structs that feed the builders so the ports
//! stay behaviourally identical; see the crate documentation.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::base64;

/// Canonical address of one browsable object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Object {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub schema: String,
}

/// One column as reported by driver metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub data_type: String,
    /// `YES` or `NO`; anything else is rejected by the builders.
    #[serde(default)]
    pub nullable: String,
    /// `PRI`, `UNI`, `MUL` or empty.
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(rename = "hasDefault", default)]
    pub has_default: bool,
    #[serde(default)]
    pub extra: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub charset: String,
    #[serde(default)]
    pub collation: String,
}

impl ColumnDefinition {
    /// Distinguishes computed fields from server-generated defaults.
    pub fn writable(&self) -> bool {
        let extra = self.extra.to_lowercase().replace("default_generated", "");
        !extra.contains("generated")
            && !extra.contains("computed")
            && !extra.contains("identity always")
    }
}

/// One metadata row of an index; multi-column indexes repeat the name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "columnName", default)]
    pub column_name: String,
    #[serde(rename = "nonUnique", default)]
    pub non_unique: i32,
    #[serde(rename = "seqInIndex", default)]
    pub seq_in_index: i32,
    #[serde(rename = "indexType", default)]
    pub index_type: String,
    #[serde(rename = "subPart", default)]
    pub sub_part: i32,
}

/// Driver metadata for one object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableInfo {
    #[serde(default)]
    pub columns: Vec<ColumnDefinition>,
    #[serde(default)]
    pub indexes: Vec<IndexDefinition>,
}

/// One typed condition in the table browser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub column: String,
    #[serde(default)]
    pub operator: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub join: String,
}

/// One validated column ordering.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    #[serde(default)]
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// A bounded server-side page of a canonical object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRequest {
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub object: Object,
    #[serde(default)]
    pub page: i64,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub sorts: Vec<Sort>,
    #[serde(default)]
    pub condition: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexColumn {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub descending: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIndex {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub columns: Vec<IndexColumn>,
    #[serde(default)]
    pub unique: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureChange {
    #[serde(default)]
    pub kind: String,
    #[serde(rename = "originalName", default)]
    pub original_name: String,
    #[serde(default)]
    pub column: ColumnDefinition,
    #[serde(default)]
    pub index: NewIndex,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureRequest {
    #[serde(default)]
    pub object: Object,
    #[serde(default)]
    pub revision: i64,
    #[serde(rename = "beforeHash", default)]
    pub before_hash: String,
    #[serde(default)]
    pub changes: Vec<StructureChange>,
    #[serde(default)]
    pub confirmation: String,
}

/// A complete original row travels with each change for optimistic checks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowChange {
    /// `insert`, `update` or `delete`.
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
    #[serde(default)]
    pub object: Object,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub rows: Vec<RowChange>,
    #[serde(default)]
    pub confirmation: String,
}

/// One bound parameter value.
///
/// A JSON value cannot express driver bytes or decimal text that must not be
/// re-parsed as a float, so bound arguments keep Go's binder distinctions.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
}

impl ArgValue {
    /// Binds an edited JSON value without changing its meaning.
    pub fn from_json(value: &Value) -> Self {
        match value {
            Value::Null => ArgValue::Null,
            Value::Bool(flag) => ArgValue::Bool(*flag),
            Value::Number(number) => {
                if let Some(integer) = number.as_i64() {
                    ArgValue::Integer(integer)
                } else if let Some(unsigned) = number.as_u64() {
                    ArgValue::Text(unsigned.to_string())
                } else if let Some(float) = number.as_f64() {
                    ArgValue::Float(float)
                } else {
                    ArgValue::Text(number.to_string())
                }
            }
            Value::String(text) => ArgValue::Text(text.clone()),
            other => ArgValue::Text(other.to_string()),
        }
    }

    /// Renders the value the way Go's `encoding/json` did in previews, where a
    /// byte slice becomes a Base64 string.
    pub fn to_json(&self) -> Value {
        match self {
            ArgValue::Null => Value::Null,
            ArgValue::Bool(flag) => Value::Bool(*flag),
            ArgValue::Integer(integer) => Value::from(*integer),
            ArgValue::Float(float) => Value::from(*float),
            ArgValue::Text(text) => Value::String(text.clone()),
            ArgValue::Bytes(bytes) => Value::String(base64::encode(bytes)),
        }
    }
}

impl From<String> for ArgValue {
    fn from(value: String) -> Self {
        ArgValue::Text(value)
    }
}

impl From<i64> for ArgValue {
    fn from(value: i64) -> Self {
        ArgValue::Integer(value)
    }
}

/// SQL structure separated from the values sent through the driver binder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Statement {
    pub sql: String,
    pub args: Vec<ArgValue>,
    pub require_one: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_columns_are_not_writable() {
        let mut column = ColumnDefinition {
            extra: "DEFAULT_GENERATED on update CURRENT_TIMESTAMP".into(),
            ..Default::default()
        };
        assert!(column.writable());
        column.extra = "STORED GENERATED".into();
        assert!(!column.writable());
        column.extra = "IDENTITY ALWAYS AS (a+b)".into();
        assert!(!column.writable());
        column.extra = "COMPUTED".into();
        assert!(!column.writable());
    }

    #[test]
    fn arguments_keep_binder_types() {
        assert_eq!(
            ArgValue::from_json(&serde_json::json!("text")),
            ArgValue::Text("text".into())
        );
        assert_eq!(
            ArgValue::from_json(&serde_json::json!(7)),
            ArgValue::Integer(7)
        );
        assert_eq!(
            ArgValue::from_json(&serde_json::json!(1.5)),
            ArgValue::Float(1.5)
        );
        assert_eq!(
            ArgValue::from_json(&serde_json::json!({"a": 1})),
            ArgValue::Text("{\"a\":1}".into())
        );
        assert_eq!(
            ArgValue::Bytes(b"fo".to_vec()).to_json(),
            serde_json::json!("Zm8=")
        );
    }
}
