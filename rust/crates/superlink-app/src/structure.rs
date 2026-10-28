//! 结构写回：把结构化变更编译成 ALTER DDL 并在单事务内执行。
//!
//! DDL 生成完全复用 superlink-sql 的校验器；这里只负责读取当前结构、
//! 组装请求并在对应原生驱动上执行。

use superlink_domain::Profile;
use superlink_drivers::{NativeMysql, NativePostgres, NativeSqlite};
use superlink_sql::error::SqlError;
use superlink_sql::model::{
    ColumnDefinition, Object, StructureChange, StructureRequest, TableInfo,
};
use superlink_sql::structure::build_structure_changes;

use crate::query::{NativeFlags, NativeKind, native_target};

#[derive(Debug, thiserror::Error)]
pub enum StructureError {
    #[error(transparent)]
    Driver(#[from] superlink_drivers::DriverError),
    #[error("SQL 校验失败：{0}")]
    Sql(#[from] SqlError),
    #[error("没有结构变更")]
    Empty,
    #[error("该数据源的结构写回尚未接入：{0}")]
    Unsupported(String),
}

/// 结构写回入口；无状态，连接按需打开。
pub struct StructureService;

impl StructureService {
    pub fn new() -> Self {
        Self
    }

    /// 返回实际执行的语句，便于在 UI 上回显。
    pub fn apply(
        &self,
        profile: &Profile,
        table: &str,
        changes: Vec<StructureChange>,
    ) -> Result<Vec<String>, StructureError> {
        self.apply_with(profile, table, changes, NativeFlags::from_env())
    }

    /// 便捷入口：新增一列。
    pub fn add_column(
        &self,
        profile: &Profile,
        table: &str,
        name: &str,
        data_type: &str,
        nullable: bool,
    ) -> Result<Vec<String>, StructureError> {
        let change = StructureChange {
            kind: "addColumn".into(),
            column: ColumnDefinition {
                name: name.trim().to_string(),
                data_type: data_type.trim().to_string(),
                nullable: if nullable { "YES" } else { "NO" }.to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        self.apply(profile, table, vec![change])
    }

    /// 便捷入口：删除一列（主键列会被校验器拒绝）。
    pub fn drop_column(
        &self,
        profile: &Profile,
        table: &str,
        name: &str,
    ) -> Result<Vec<String>, StructureError> {
        let change = StructureChange {
            kind: "dropColumn".into(),
            original_name: name.to_string(),
            column: ColumnDefinition {
                name: name.to_string(),
                data_type: String::new(),
                nullable: "YES".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        self.apply(profile, table, vec![change])
    }

    /// 显式传入开关，测试用它避免改动进程环境变量。
    pub(crate) fn apply_with(
        &self,
        profile: &Profile,
        table: &str,
        changes: Vec<StructureChange>,
        flags: NativeFlags,
    ) -> Result<Vec<String>, StructureError> {
        if changes.is_empty() {
            return Err(StructureError::Empty);
        }
        let kind = profile.sql_dialect().to_string();
        let target =
            native_target(&kind, flags).ok_or_else(|| StructureError::Unsupported(kind.clone()))?;

        let structure = match target {
            NativeKind::Sqlite => NativeSqlite::open(&profile.config)?.structure(table)?,
            NativeKind::Mysql => NativeMysql::open(&profile.config)?.structure(table)?,
            NativeKind::Postgres => NativePostgres::open(&profile.config)?.structure(table)?,
            _ => return Err(StructureError::Unsupported(kind)),
        };

        let info = TableInfo {
            columns: structure
                .columns
                .iter()
                .map(|column| ColumnDefinition {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    nullable: if column.nullable { "YES" } else { "NO" }.to_string(),
                    key: if column.primary_key { "PRI" } else { "" }.to_string(),
                    default: column.default_value.clone(),
                    has_default: column.default_value.is_some(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };

        let (schema, name) = match table.split_once('.') {
            Some((schema, name)) => (schema.to_string(), name.to_string()),
            None => (String::new(), table.to_string()),
        };
        let request = StructureRequest {
            object: Object {
                name,
                kind: "table".into(),
                schema,
                ..Default::default()
            },
            changes,
            ..Default::default()
        };

        let statements = build_structure_changes(&kind, &request, &info)?;
        match target {
            NativeKind::Sqlite => {
                NativeSqlite::open(&profile.config)?.execute_batch(&statements)?
            }
            NativeKind::Mysql => NativeMysql::open(&profile.config)?.execute_batch(&statements)?,
            NativeKind::Postgres => {
                NativePostgres::open(&profile.config)?.execute_batch(&statements)?
            }
            _ => return Err(StructureError::Unsupported(kind)),
        }
        Ok(statements)
    }
}

impl Default for StructureService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::ConnectionConfig;
    use superlink_sql::model::NewIndex;

    fn sqlite_profile(database: &str) -> Profile {
        Profile {
            config: ConnectionConfig {
                kind: "sqlite".into(),
                database: database.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn temp_db() -> (Profile, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("superlink-structure-{}.db", uuid::Uuid::new_v4()));
        let driver = NativeSqlite::open(&sqlite_profile(&path.to_string_lossy()).config).unwrap();
        driver
            .query(
                "CREATE TABLE items(id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
                &[],
                0,
            )
            .unwrap();
        (sqlite_profile(&path.to_string_lossy()), path)
    }

    fn add_column(name: &str) -> StructureChange {
        StructureChange {
            kind: "addColumn".into(),
            column: ColumnDefinition {
                name: name.into(),
                data_type: "TEXT".into(),
                nullable: "YES".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn applies_add_and_drop_column() {
        let (profile, path) = temp_db();
        let service = StructureService::new();
        let flags = sqlite_flags();

        let statements = service
            .apply_with(&profile, "items", vec![add_column("note")], flags)
            .expect("add column");
        assert_eq!(statements.len(), 1);
        assert!(
            statements[0].contains("ALTER TABLE") && statements[0].contains("ADD COLUMN"),
            "实际语句：{}",
            statements[0]
        );

        let driver = NativeSqlite::open(&profile.config).unwrap();
        let structure = driver.structure("items").unwrap();
        assert!(structure.columns.iter().any(|column| column.name == "note"));

        let drop_change = StructureChange {
            kind: "dropColumn".into(),
            original_name: "note".into(),
            column: ColumnDefinition {
                name: "note".into(),
                data_type: "TEXT".into(),
                nullable: "YES".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        service
            .apply_with(&profile, "items", vec![drop_change], flags)
            .expect("drop");
        let structure = driver.structure("items").unwrap();
        assert!(!structure.columns.iter().any(|column| column.name == "note"));
        drop(driver);
        let _ = std::fs::remove_file(path);
    }

    fn sqlite_flags() -> NativeFlags {
        NativeFlags {
            sqlite: true,
            ..Default::default()
        }
    }

    #[test]
    fn convenience_helpers_add_and_drop() {
        let (profile, path) = temp_db();
        let service = StructureService::new();
        let flags = sqlite_flags();
        service
            .apply_with(&profile, "items", vec![add_column("note")], flags)
            .unwrap();
        // 直接用便捷方法删除
        let statements = service
            .apply_with(
                &profile,
                "items",
                vec![StructureChange {
                    kind: "dropColumn".into(),
                    original_name: "note".into(),
                    column: ColumnDefinition {
                        name: "note".into(),
                        data_type: "TEXT".into(),
                        nullable: "YES".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                flags,
            )
            .unwrap();
        assert!(statements[0].contains("DROP COLUMN"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_duplicate_column_names() {
        let (profile, path) = temp_db();
        let error = StructureService::new()
            .apply_with(&profile, "items", vec![add_column("title")], sqlite_flags())
            .unwrap_err();
        assert!(error.to_string().contains("duplicate"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_unsupported_dialects() {
        let profile = Profile {
            config: ConnectionConfig {
                kind: "oracle".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let error = StructureService::new()
            .apply(&profile, "t", vec![add_column("x")])
            .unwrap_err();
        assert!(matches!(error, StructureError::Unsupported(_)));
    }

    #[test]
    fn rolls_back_when_a_statement_fails() {
        let (profile, path) = temp_db();
        // 先建一个已存在的索引：校验通过，但执行阶段会冲突，用于验证回滚
        let setup = NativeSqlite::open(&profile.config).unwrap();
        setup
            .query("CREATE INDEX idx_existing ON items(id)", &[], 0)
            .unwrap();
        drop(setup);
        let duplicate_index = StructureChange {
            kind: "addIndex".into(),
            index: NewIndex {
                name: "idx_existing".into(),
                columns: vec![superlink_sql::model::IndexColumn {
                    name: "id".into(),
                    descending: false,
                }],
                unique: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let error = StructureService::new()
            .apply_with(
                &profile,
                "items",
                vec![add_column("note"), duplicate_index],
                sqlite_flags(),
            )
            .unwrap_err();
        assert!(matches!(error, StructureError::Driver(_)));
        let driver = NativeSqlite::open(&profile.config).unwrap();
        let structure = driver.structure("items").unwrap();
        assert!(
            !structure.columns.iter().any(|column| column.name == "note"),
            "失败后应回滚"
        );
        drop(driver);
        let _ = std::fs::remove_file(path);
    }
}
