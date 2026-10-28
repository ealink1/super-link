//! 结构面板共用的数据类型。

/// 表/视图条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableEntry {
    pub name: String,
    pub kind: String,
}

/// 列定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnEntry {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub default_value: Option<String>,
}

/// 索引定义；列顺序即索引列顺序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

/// 外键定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeyEntry {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
}

/// 一次结构查询的完整结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableStructure {
    pub columns: Vec<ColumnEntry>,
    pub indexes: Vec<IndexEntry>,
    pub foreign_keys: Vec<ForeignKeyEntry>,
}
