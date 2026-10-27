use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ColumnDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub data_type: String,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub extra: String,
    #[serde(default)]
    pub comment: String,
    #[serde(rename = "primaryKey", default)]
    pub primary_key: bool,
    #[serde(default)]
    pub length: Option<i64>,
    #[serde(default)]
    pub precision: Option<i64>,
    #[serde(default)]
    pub scale: Option<i64>,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ForeignKeyDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(rename = "referencedTable", default)]
    pub referenced_table: String,
    #[serde(rename = "referencedColumns", default)]
    pub referenced_columns: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TriggerDefinition {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub timing: String,
    #[serde(default)]
    pub event: String,
}

/// Driver metadata for one object, independent of the UI toolkit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableInfo {
    #[serde(default)]
    pub columns: Vec<ColumnDefinition>,
    #[serde(default)]
    pub indexes: Vec<IndexDefinition>,
    #[serde(rename = "foreignKeys", default)]
    pub foreign_keys: Vec<ForeignKeyDefinition>,
    #[serde(default)]
    pub triggers: Vec<TriggerDefinition>,
    #[serde(default)]
    pub ddl: String,
    #[serde(default)]
    pub warnings: Vec<String>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    #[serde(default)]
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// A bounded server-side page of a canonical object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TableRequest {
    pub revision: i64,
    pub object: Object,
    pub page: u32,
    pub size: u32,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub sorts: Vec<Sort>,
    #[serde(default)]
    pub condition: String,
}

/// Total count and metadata travel with the bounded result.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TablePage {
    pub revision: i64,
    pub info: TableInfo,
    pub result: crate::query::ResultSet,
    pub total: i64,
    pub page: u32,
    pub size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_columns_are_not_writable() {
        let column = ColumnDefinition {
            extra: "DEFAULT_GENERATED on update CURRENT_TIMESTAMP".into(),
            ..Default::default()
        };
        assert!(column.writable());
        let computed = ColumnDefinition {
            extra: "STORED GENERATED".into(),
            ..Default::default()
        };
        assert!(!computed.writable());
        let identity = ColumnDefinition {
            extra: "IDENTITY ALWAYS AS (a+b)".into(),
            ..Default::default()
        };
        assert!(!identity.writable());
    }
}
