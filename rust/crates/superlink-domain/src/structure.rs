use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::table::{ColumnDefinition, TableInfo};

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StructureChange {
    #[serde(default)]
    pub kind: String,
    #[serde(rename = "originalName", default)]
    pub original_name: String,
    #[serde(default)]
    pub column: Option<ColumnDefinition>,
    #[serde(default)]
    pub index: Option<NewIndex>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StructureRequest {
    pub object: crate::table::Object,
    pub revision: i64,
    #[serde(rename = "beforeHash", default)]
    pub before_hash: String,
    #[serde(default)]
    pub changes: Vec<StructureChange>,
    #[serde(default)]
    pub confirmation: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureResult {
    #[serde(default)]
    pub applied: u32,
    #[serde(default)]
    pub atomic: bool,
    #[serde(default)]
    pub attempted: bool,
}

/// Ignores transient metadata warnings and binds the visible schema.
pub fn structure_hash(info: &TableInfo) -> String {
    let mut snapshot = info.clone();
    snapshot.warnings.clear();
    let raw = serde_json::to_vec(&snapshot).unwrap_or_default();
    let digest = Sha256::digest(&raw);
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_do_not_change_the_hash() {
        let mut info = TableInfo::default();
        info.ddl = "CREATE TABLE t(a int)".into();
        let first = structure_hash(&info);
        info.warnings.push("driver note".into());
        assert_eq!(first, structure_hash(&info));
    }

    #[test]
    fn column_changes_change_the_hash() {
        let mut info = TableInfo::default();
        let first = structure_hash(&info);
        info.columns.push(ColumnDefinition {
            name: "a".into(),
            ..Default::default()
        });
        assert_ne!(first, structure_hash(&info));
    }
}
