//! 结构面板共用的数据类型。

/// 表/视图条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableEntry {
    pub name: String,
    pub kind: String,
}

/// information_schema 提供的表统计；不支持的方言保持默认值。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TableStats {
    /// 引擎估算行数（MySQL 的 TABLE_ROWS 是采样值）。
    pub rows: Option<u64>,
    /// 数据长度，单位字节。
    pub data_length: Option<u64>,
    pub engine: Option<String>,
    /// 建表时间，保留数据库返回的文本形式。
    pub created: Option<String>,
    pub updated: Option<String>,
    pub collation: Option<String>,
    pub comment: Option<String>,
}

/// 列定义。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ColumnEntry {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub default_value: Option<String>,
    pub extra: String,
    /// PostgreSQL 的列注释（COMMENT ON COLUMN），其它方言留空。
    pub comment: Option<String>,
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
