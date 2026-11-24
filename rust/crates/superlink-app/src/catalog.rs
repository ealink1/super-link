//! Metadata uses the data source's Rust agent identity, independently of SQL dialect.
use crate::{Credentials, QueryCancel, QueryError, QueryService};
use serde_json::Value;
use std::collections::BTreeMap;
use superlink_domain::Profile;
use superlink_drivers::{
    ColumnEntry, DriverSession, ForeignKeyEntry, IndexEntry, TableEntry, TableStructure,
};

impl QueryService {
    fn with_catalog_agent<T>(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        operation: impl FnOnce(&mut DriverSession) -> Result<T, QueryError>,
    ) -> Result<T, QueryError> {
        let kind = profile.config.kind.as_str();
        let binary = self
            .agent_path(kind)
            .ok_or_else(|| QueryError::Unsupported(kind.into()))?;
        let mut config = credentials.apply(&profile.config);
        crate::scope::apply(&mut config, database).map_err(QueryError::Policy)?;
        let cancel = QueryCancel::new();
        let _operation = crate::execution::Operation::new(&cancel, config.query_timeout.max(30));
        let mut session = DriverSession::open_with_cancel(binary, kind, &cancel)?;
        session.set_cancellation(&cancel);
        session.connect(&config)?;
        operation(&mut session)
    }
    pub(crate) fn agent_databases(
        &self,
        profile: &Profile,
        credentials: &Credentials,
    ) -> Result<Vec<String>, QueryError> {
        self.with_catalog_agent(profile, credentials, "", |session| {
            let value = session.catalog("getDatabases", "", "")?;
            serde_json::from_value(value)
                .map_err(|_| QueryError::Policy("数据库目录响应无效".into()))
        })
    }
    pub(crate) fn agent_tables(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
    ) -> Result<Vec<TableEntry>, QueryError> {
        self.with_catalog_agent(profile, credentials, database, |session| {
            let scope = if profile.config.kind == "redis" && database.parse::<u32>().is_ok() {
                "*"
            } else {
                database
            };
            let value = session.catalog("getTables", scope, "")?;
            let kind = superlink_domain::resolve(&profile.config.kind)
                .map(|d| {
                    if d.family == superlink_domain::Family::Sql {
                        "table"
                    } else {
                        d.family.as_str()
                    }
                })
                .unwrap_or("table");
            let names: Vec<String> = serde_json::from_value(value)
                .map_err(|_| QueryError::Policy("表目录响应无效".into()))?;
            Ok(names
                .into_iter()
                .take(10000)
                .map(|name| TableEntry {
                    name,
                    kind: kind.into(),
                })
                .collect())
        })
    }
    pub(crate) fn agent_structure(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, QueryError> {
        self.with_catalog_agent(profile, credentials, database, |session| {
            let columns = session.catalog("getColumns", database, table)?;
            let indexes = session.catalog("getIndexes", database, table)?;
            let foreign = session.catalog("getForeignKeys", database, table)?;
            let columns = columns
                .as_array()
                .ok_or_else(|| QueryError::Policy("列目录响应无效".into()))?
                .iter()
                .take(4096)
                .map(|c| ColumnEntry {
                    name: text(c, "name"),
                    data_type: text(c, "type"),
                    nullable: text(c, "nullable") == "YES",
                    primary_key: text(c, "key") == "PRI",
                    extra: text(c, "extra"),
                    default_value: c.get("default").and_then(Value::as_str).map(str::to_owned),
                    comment: c
                        .get("comment")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                })
                .collect();
            let mut grouped: BTreeMap<String, IndexEntry> = BTreeMap::new();
            if let Some(indexes) = indexes.as_array() {
                let mut entries = indexes.iter().collect::<Vec<_>>();
                entries.sort_by_key(|i| i.get("seqInIndex").and_then(Value::as_i64).unwrap_or(0));
                for item in entries {
                    let name = text(item, "name");
                    let index = grouped.entry(name.clone()).or_insert_with(|| IndexEntry {
                        name,
                        columns: Vec::new(),
                        unique: item.get("nonUnique").and_then(Value::as_i64) == Some(0),
                    });
                    index.columns.push(text(item, "columnName"));
                }
            }
            let mut keys: BTreeMap<String, ForeignKeyEntry> = BTreeMap::new();
            if let Some(foreign) = foreign.as_array() {
                for item in foreign {
                    let name = text(item, "constraintName");
                    let key = keys.entry(name.clone()).or_insert_with(|| ForeignKeyEntry {
                        name,
                        columns: Vec::new(),
                        referenced_table: text(item, "refTableName"),
                        referenced_columns: Vec::new(),
                    });
                    key.columns.push(text(item, "columnName"));
                    key.referenced_columns.push(text(item, "refColumnName"));
                }
            }
            Ok(TableStructure {
                columns,
                indexes: grouped.into_values().collect(),
                foreign_keys: keys.into_values().collect(),
            })
        })
    }
    pub fn table_ddl(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        table: &str,
    ) -> Result<String, QueryError> {
        if profile.sql_dialect() == "sqlite" && !profile.config.use_ssh {
            let driver = superlink_drivers::NativeSqlite::open(&profile.config)?;
            let result = driver.query(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                &[Value::String(table.into())],
                1,
            )?;
            return Ok(result
                .rows
                .first()
                .and_then(|r| r.first())
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into());
        }
        if matches!(
            crate::query::native_target(
                profile.sql_dialect(),
                crate::query::NativeFlags::from_env()
            ),
            Some(crate::query::NativeKind::Mysql)
        ) && !profile.config.use_ssh
            && !profile.config.use_proxy
        {
            let mut current = profile.clone();
            if !database.is_empty() {
                current.config.database = database.into();
            }
            let quoted = superlink_sql::quote_identifier(profile.sql_dialect(), table)
                .map_err(|e| QueryError::Policy(e.to_string()))?;
            let result = self.execute(
                &current,
                credentials,
                &format!("SHOW CREATE TABLE {quoted}"),
                1,
            )?;
            return Ok(result
                .rows
                .first()
                .and_then(|r| r.get(1))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into());
        }
        self.with_catalog_agent(profile, credentials, database, |session| {
            let value = session.catalog("getCreateStatement", database, table)?;
            Ok(value.as_str().unwrap_or_default().into())
        })
    }
}
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
