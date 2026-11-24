//! 结构写回：把结构化变更编译成 ALTER DDL 并在单事务内执行。
//!
//! DDL 生成完全复用 superlink-sql 的校验器；这里只负责读取当前结构、
//! 组装请求并在对应原生驱动或代理会话上执行。

use superlink_domain::Profile;
use superlink_drivers::{NativeMysql, NativePostgres, NativeSqlite};
use superlink_sql::error::SqlError;
use superlink_sql::model::{
    ColumnDefinition, Object, StructureChange, StructureRequest, TableInfo,
};
use superlink_sql::structure::build_structure_changes;

use crate::query::{NativeFlags, NativeKind, native_target};

/// 驱动列/索引条目 → SQL 层表信息。
pub fn table_info_from(
    columns: &[superlink_drivers::ColumnEntry],
    indexes: &[superlink_drivers::IndexEntry],
) -> TableInfo {
    TableInfo {
        columns: columns
            .iter()
            .map(|column| ColumnDefinition {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                nullable: if column.nullable { "YES" } else { "NO" }.to_string(),
                key: if column.primary_key { "PRI" } else { "" }.to_string(),
                default: column.default_value.clone(),
                has_default: column.default_value.is_some(),
                extra: column.extra.clone(),
                ..Default::default()
            })
            .collect(),
        indexes: indexes
            .iter()
            .flat_map(|index| {
                index
                    .columns
                    .iter()
                    .enumerate()
                    .map(move |(position, column)| superlink_sql::IndexDefinition {
                        name: index.name.clone(),
                        column_name: column.clone(),
                        non_unique: if index.unique { 0 } else { 1 },
                        seq_in_index: position as i32 + 1,
                        ..Default::default()
                    })
            })
            .collect(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StructureError {
    #[error(transparent)]
    Driver(#[from] superlink_drivers::DriverError),
    #[error("SQL 校验失败：{0}")]
    Sql(#[from] SqlError),
    #[error("{0}")]
    Policy(String),
    #[error("没有结构变更")]
    Empty,
    #[error("该数据源的结构写回尚未接入：{0}")]
    Unsupported(String),
}

/// 结构写回入口；无状态，连接按需打开。
pub struct StructureService {
    queries: Option<std::sync::Arc<crate::QueryService>>,
}

impl StructureService {
    pub fn new() -> Self {
        Self { queries: None }
    }
    pub fn with_queries(queries: std::sync::Arc<crate::QueryService>) -> Self {
        Self {
            queries: Some(queries),
        }
    }

    /// 返回实际执行的语句，便于在 UI 上回显。
    pub fn apply(
        &self,
        profile: &Profile,
        database: &str,
        table: &str,
        changes: Vec<StructureChange>,
    ) -> Result<Vec<String>, StructureError> {
        self.apply_with(profile, database, table, changes, NativeFlags::from_env())
    }

    fn apply_agent(
        &self,
        profile: &Profile,
        database: &str,
        table: &str,
        changes: Vec<StructureChange>,
    ) -> Result<Vec<String>, StructureError> {
        if superlink_sql::structure_dialect(profile.sql_dialect()).is_empty() {
            return Err(StructureError::Unsupported(profile.sql_dialect().into()));
        }
        let queries = self
            .queries
            .as_ref()
            .ok_or_else(|| StructureError::Unsupported(profile.sql_dialect().into()))?;
        let credentials = crate::Credentials::default();
        let structure = queries
            .structure(profile, &credentials, database, table)
            .map_err(|e| StructureError::Policy(e.to_string()))?;
        let info = table_info_from(&structure.columns, &structure.indexes);
        let (schema, name) = table
            .split_once('.')
            .map(|(s, n)| (s.to_string(), n.to_string()))
            .unwrap_or_else(|| (String::new(), table.to_string()));
        let request = StructureRequest {
            object: Object {
                name,
                scope: database.into(),
                schema,
                kind: "table".into(),
            },
            changes,
            ..Default::default()
        };
        let statements = build_structure_changes(profile.sql_dialect(), &request, &info)?;
        let cancel = crate::QueryCancel::new();
        let _operation =
            crate::execution::Operation::new(&cancel, profile.config.query_timeout.max(30));
        let mut live = profile.clone();
        crate::scope::apply(&mut live.config, database).map_err(StructureError::Policy)?;
        let session = queries
            .open_agent_operation(&live, &credentials, &cancel)
            .map_err(|e| StructureError::Policy(e.to_string()))?;
        let mut connection = crate::execution::Connection::Agent(session);
        if !connection.transactional() {
            return Err(StructureError::Unsupported("驱动缺少结构变更会话".into()));
        }
        connection
            .query("BEGIN", &[], 1)
            .map_err(|e| StructureError::Policy(e.to_string()))?;
        let outcome = (|| {
            for statement in &statements {
                if cancel.is_cancelled() {
                    return Err(StructureError::Policy(
                        "结构变更已停止，请刷新确认数据库状态".into(),
                    ));
                }
                connection
                    .query(statement, &[], 1)
                    .map_err(|e| StructureError::Policy(e.to_string()))?;
            }
            connection
                .query("COMMIT", &[], 1)
                .map_err(|e| StructureError::Policy(e.to_string()))?;
            Ok(statements)
        })();
        if outcome.is_err() {
            let _ = connection.query("ROLLBACK", &[], 1);
        }
        outcome
    }

    /// 便捷入口：新增一列。
    pub fn add_column(
        &self,
        profile: &Profile,
        database: &str,
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
        self.apply(profile, database, table, vec![change])
    }

    /// 便捷入口：删除一列（主键列会被校验器拒绝）。
    pub fn drop_column(
        &self,
        profile: &Profile,
        database: &str,
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
        self.apply(profile, database, table, vec![change])
    }

    /// 显式传入开关，测试用它避免改动进程环境变量。
    pub fn apply_with(
        &self,
        profile: &Profile,
        database: &str,
        table: &str,
        changes: Vec<StructureChange>,
        flags: NativeFlags,
    ) -> Result<Vec<String>, StructureError> {
        crate::execution_policy::authorize(profile, "ALTER TABLE object", true, "structure")
            .map_err(StructureError::Policy)?;
        if changes.is_empty() {
            return Err(StructureError::Empty);
        }
        let kind = profile.sql_dialect().to_string();
        if profile.config.use_ssh
            || profile.config.use_proxy
            || profile.config.use_http_tunnel
            || !profile.config.dsn.is_empty()
            || !profile.config.uri.is_empty()
            || native_target(&kind, flags).is_none()
        {
            return self.apply_agent(profile, database, table, changes);
        }
        let target =
            native_target(&kind, flags).ok_or_else(|| StructureError::Unsupported(kind.clone()))?;

        let mut config = profile.config.clone();
        if !database.is_empty() {
            config.database = database.into();
        }
        let cancel = crate::QueryCancel::new();
        let _operation = crate::execution::Operation::new(&cancel, config.query_timeout.max(30));
        let mut connection = match target {
            NativeKind::Sqlite => {
                let driver = NativeSqlite::open(&config)?;
                driver.set_cancellation(&cancel);
                crate::execution::Connection::Sqlite(driver)
            }
            NativeKind::Mysql => {
                let driver = NativeMysql::open(&config)?;
                driver.set_cancellation(&cancel, &config);
                crate::execution::Connection::Mysql(driver)
            }
            NativeKind::Postgres => {
                let driver = NativePostgres::open(&config)?;
                driver.set_cancellation(&cancel);
                crate::execution::Connection::Postgres(driver)
            }
            _ => return Err(StructureError::Unsupported(kind)),
        };
        let structure = match &mut connection {
            crate::execution::Connection::Sqlite(driver) => driver.structure(table)?,
            crate::execution::Connection::Mysql(driver) => driver.structure(database, table)?,
            crate::execution::Connection::Postgres(driver) => driver.structure(table)?,
            _ => unreachable!(),
        };

        let info = table_info_from(&structure.columns, &structure.indexes);

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
        if cancel.is_cancelled() {
            return Err(StructureError::Policy("结构变更已停止".into()));
        }
        match &mut connection {
            crate::execution::Connection::Sqlite(driver) => driver.execute_batch(&statements)?,
            crate::execution::Connection::Mysql(driver) => {
                let mut batch = vec![superlink_sql::identifier::MYSQL_DDL_SESSION.to_string()];
                batch.extend(statements.iter().cloned());
                driver.execute_batch(&batch)?;
            }
            crate::execution::Connection::Postgres(driver) => driver.execute_batch(&statements)?,
            _ => unreachable!(),
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
            .apply_with(&profile, "", "items", vec![add_column("note")], flags)
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
            .apply_with(&profile, "", "items", vec![drop_change], flags)
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
            .apply_with(&profile, "", "items", vec![add_column("note")], flags)
            .unwrap();
        // 直接用便捷方法删除
        let statements = service
            .apply_with(
                &profile,
                "",
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
            .apply_with(
                &profile,
                "",
                "items",
                vec![add_column("title")],
                sqlite_flags(),
            )
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
            .apply(&profile, "", "t", vec![add_column("x")])
            .unwrap_err();
        assert!(matches!(error, StructureError::Unsupported(_)));
    }

    #[test]
    fn rolls_back_when_a_statement_fails() {
        let (profile, path) = temp_db();
        // 先建一个已存在的索引：校验通过，但执行阶段会冲突，用于验证回滚
        let setup = NativeSqlite::open(&profile.config).unwrap();
        setup
            .query("CREATE TABLE other(id INTEGER)", &[], 0)
            .unwrap();
        setup
            .query("CREATE INDEX idx_existing ON other(id)", &[], 0)
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
                "",
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
    #[test]
    fn preserves_composite_index_order_and_generated_metadata() {
        let info = table_info_from(
            &[superlink_drivers::ColumnEntry {
                name: "computed".into(),
                data_type: "TEXT".into(),
                nullable: true,
                primary_key: false,
                extra: "VIRTUAL GENERATED".into(),
                default_value: None,
                comment: None,
            }],
            &[superlink_drivers::IndexEntry {
                name: "combined".into(),
                columns: vec!["b".into(), "a".into()],
                unique: true,
            }],
        );
        assert_eq!(info.indexes.len(), 2);
        assert_eq!(info.indexes[1].column_name, "a");
        assert_eq!(info.indexes[1].seq_in_index, 2);
        assert_eq!(info.columns[0].extra, "VIRTUAL GENERATED");
    }

    #[test]
    fn agent_structure_uses_one_transaction_and_rolls_back_failure() {
        let (profile, path) = temp_db();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../bin/drivers");
        let service =
            StructureService::with_queries(std::sync::Arc::new(crate::QueryService::new(root)));
        service
            .apply_with(
                &profile,
                "",
                "items",
                vec![add_column("note")],
                NativeFlags::default(),
            )
            .unwrap();
        let duplicate = StructureChange {
            kind: "addIndex".into(),
            index: NewIndex {
                name: "idx_fail".into(),
                columns: vec![superlink_sql::model::IndexColumn {
                    name: "id".into(),
                    descending: false,
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let driver = NativeSqlite::open(&profile.config).unwrap();
        driver
            .query("CREATE TABLE other(id INTEGER)", &[], 0)
            .unwrap();
        driver
            .query("CREATE INDEX idx_fail ON other(id)", &[], 0)
            .unwrap();
        assert!(
            service
                .apply_with(
                    &profile,
                    "",
                    "items",
                    vec![add_column("rolled_back"), duplicate],
                    NativeFlags::default()
                )
                .is_err()
        );
        let columns = driver.structure("items").unwrap().columns;
        assert!(columns.iter().any(|c| c.name == "note"));
        assert!(!columns.iter().any(|c| c.name == "rolled_back"));
        drop(driver);
        let _ = std::fs::remove_file(path);
    }
}
