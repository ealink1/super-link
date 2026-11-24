use std::path::{Path, PathBuf};

use superlink_domain::{ConnectionConfig, Profile, ResultSet};
use superlink_drivers::{
    DriverError, NativeMysql, NativePostgres, NativeSqlite, TableEntry, TableStructure,
};

use crate::profiles::Credentials;

const DEFAULT_MAX_ROWS: u32 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("{0}")]
    Policy(String),
    #[error("执行已停止或已超时；写入结果请刷新数据库确认")]
    Cancelled,
    #[error(transparent)]
    Driver(#[from] DriverError),
    #[error("该数据源尚未接入 Rust 驱动层：{0}")]
    Unsupported(String),
    #[error("连接配置无效")]
    InvalidProfile,
    #[error("SQL 为空")]
    Empty,
}

/// Resolves an installed agent, connects, runs one statement and always tears
/// the process down. Credentials are injected only into the live config.
type DriverStamp = (u64, Option<std::time::SystemTime>, (u64, i64, i64));
pub struct QueryService {
    drivers_dir: PathBuf,
    directories: Vec<PathBuf>,
    verified_files: std::sync::Mutex<std::collections::HashMap<String, DriverStamp>>,
    installed: std::sync::RwLock<Vec<superlink_drivers::InstalledDriver>>,
}

impl QueryService {
    pub fn new(drivers_dir: impl Into<PathBuf>) -> Self {
        Self::with_directories(vec![drivers_dir.into()])
    }
    pub fn with_directories(directories: Vec<PathBuf>) -> Self {
        let service = Self {
            drivers_dir: directories.first().cloned().unwrap_or_default(),
            directories,
            installed: Default::default(),
            verified_files: Default::default(),
        };
        let _ = service.refresh_drivers();
        service
    }
    pub fn refresh_drivers(&self) -> Result<(), String> {
        let mut entries = Vec::new();
        for directory in &self.directories {
            let found =
                superlink_drivers::registry::inventory(directory).map_err(|e| e.to_string())?;
            for driver in found {
                if !entries
                    .iter()
                    .any(|d: &superlink_drivers::InstalledDriver| d.kind == driver.kind)
                {
                    entries.push(driver);
                }
            }
        }
        *self.installed.write().unwrap_or_else(|e| e.into_inner()) = entries;
        self.verified_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(())
    }
    pub fn installed_drivers(&self) -> Vec<superlink_drivers::InstalledDriver> {
        self.installed
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn drivers_dir(&self) -> &Path {
        &self.drivers_dir
    }
    pub fn agent_path(&self, kind: &str) -> Option<PathBuf> {
        let candidate = self
            .installed
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|d| d.kind == kind)
            .cloned()?;
        let metadata = std::fs::metadata(&candidate.path).ok()?;
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (metadata.ino(), metadata.ctime(), metadata.ctime_nsec())
        };
        #[cfg(not(unix))]
        let identity = (0, 0, 0);
        let stamp = (metadata.len(), metadata.modified().ok(), identity);
        if candidate.is_usable()
            && self
                .verified_files
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(kind)
                == Some(&stamp)
        {
            return Some(candidate.path);
        }
        let mut verified =
            superlink_drivers::registry::scan_one(candidate.path.parent()?, kind).ok()?;
        let driver = verified.pop().filter(|d| d.is_usable())?;
        let path = driver.path.clone();
        if let Some(record) = self
            .installed
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .iter_mut()
            .find(|d| d.kind == kind && d.path == driver.path)
        {
            *record = driver;
        }
        self.verified_files
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(kind.into(), stamp);
        Some(path)
    }

    pub fn supports(&self, kind: &str) -> bool {
        native_target(kind, NativeFlags::from_env()).is_some()
            || self
                .installed
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .any(|d| d.kind == kind && d.problem.is_none())
    }

    /// 结构浏览：原生 SQL 驱动优先，其余方言与隧道由 agent 承担。
    /// 层级对象树第一层：数据库列表。SQLite 返回空表示没有库层级。
    pub fn databases(
        &self,
        profile: &Profile,
        credentials: &Credentials,
    ) -> Result<Vec<String>, QueryError> {
        let kind = profile.sql_dialect().to_string();
        let mut config = live_config(&profile.config, credentials);
        if config.query_timeout == 0 {
            config.query_timeout = 30;
        }
        let cancel = crate::QueryCancel::new();
        let _operation = crate::execution::Operation::new(&cancel, config.query_timeout);
        match if profile.config.use_ssh
            || profile.config.use_proxy
            || profile.config.use_http_tunnel
            || !profile.config.dsn.is_empty()
            || !profile.config.uri.is_empty()
        {
            None
        } else {
            native_target(&kind, NativeFlags::from_env())
        } {
            Some(NativeKind::Sqlite) => {
                let driver = NativeSqlite::open(&config)?;
                driver.set_cancellation(&cancel);
                Ok(driver.databases()?)
            }
            Some(NativeKind::Mysql) => {
                let mut driver = NativeMysql::open(&config)?;
                driver.set_cancellation(&cancel, &config);
                Ok(driver.databases()?)
            }
            Some(NativeKind::Postgres) => {
                let mut driver = NativePostgres::open(&config)?;
                driver.set_cancellation(&cancel);
                Ok(driver.databases()?)
            }
            _ => self.agent_databases(profile, credentials),
        }
    }

    /// 指定库（SQLite 忽略）下的表与视图。
    pub fn tables(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
    ) -> Result<Vec<TableEntry>, QueryError> {
        let kind = profile.sql_dialect().to_string();
        let mut config = live_config(&profile.config, credentials);
        if !database.is_empty() {
            config.database = database.into();
        }
        if config.query_timeout == 0 {
            config.query_timeout = 30;
        }
        let cancel = crate::QueryCancel::new();
        let _operation = crate::execution::Operation::new(&cancel, config.query_timeout);
        match if profile.config.use_ssh
            || profile.config.use_proxy
            || profile.config.use_http_tunnel
            || !profile.config.dsn.is_empty()
            || !profile.config.uri.is_empty()
        {
            None
        } else {
            native_target(&kind, NativeFlags::from_env())
        } {
            Some(NativeKind::Sqlite) => {
                let driver = NativeSqlite::open(&config)?;
                driver.set_cancellation(&cancel);
                Ok(driver.tables()?)
            }
            Some(NativeKind::Mysql) => {
                let mut driver = NativeMysql::open(&config)?;
                driver.set_cancellation(&cancel, &config);
                Ok(driver.tables(database)?)
            }
            Some(NativeKind::Postgres) => {
                let mut driver = NativePostgres::open(&config)?;
                driver.set_cancellation(&cancel);
                Ok(driver.tables()?)
            }
            _ => self.agent_tables(profile, credentials, database),
        }
    }

    /// 单表列、索引与外键，保持数据库上下文及请求期限。
    pub fn structure(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        database: &str,
        table: &str,
    ) -> Result<TableStructure, QueryError> {
        let kind = profile.sql_dialect().to_string();
        let mut config = live_config(&profile.config, credentials);
        if !database.is_empty() {
            config.database = database.into();
        }
        if config.query_timeout == 0 {
            config.query_timeout = 30;
        }
        let cancel = crate::QueryCancel::new();
        let _operation = crate::execution::Operation::new(&cancel, config.query_timeout);
        match if profile.config.use_ssh
            || profile.config.use_proxy
            || profile.config.use_http_tunnel
            || !profile.config.dsn.is_empty()
            || !profile.config.uri.is_empty()
        {
            None
        } else {
            native_target(&kind, NativeFlags::from_env())
        } {
            Some(NativeKind::Sqlite) => {
                let driver = NativeSqlite::open(&config)?;
                driver.set_cancellation(&cancel);
                Ok(driver.structure(table)?)
            }
            Some(NativeKind::Mysql) => {
                let mut driver = NativeMysql::open(&config)?;
                driver.set_cancellation(&cancel, &config);
                Ok(driver.structure(database, table)?)
            }
            Some(NativeKind::Postgres) => {
                let mut driver = NativePostgres::open(&config)?;
                driver.set_cancellation(&cancel);
                Ok(driver.structure(table)?)
            }
            _ => self.agent_structure(profile, credentials, database, table),
        }
    }

    pub fn execute(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        sql: &str,
        max_rows: u32,
    ) -> Result<ResultSet, QueryError> {
        if sql.trim().is_empty() {
            return Err(QueryError::Empty);
        }
        let request = superlink_domain::Execution {
            text: sql.into(),
            max_rows: if max_rows == 0 {
                DEFAULT_MAX_ROWS
            } else {
                max_rows
            },
            write: true,
            revision: profile.revision,
            ..Default::default()
        };
        Ok(self
            .execute_request(
                profile,
                credentials,
                &request,
                &superlink_drivers::QueryCancel::new(),
            )?
            .pop()
            .unwrap_or_default())
    }

    /// 连接测试：执行一条最便宜的探测语句并返回往返耗时（毫秒）。
    /// 失败时返回驱动给出的原始错误，方便表单直接展示。
    pub fn test_connection(
        &self,
        profile: &Profile,
        credentials: &Credentials,
    ) -> Result<u64, QueryError> {
        self.test_connection_with_cancel(profile, credentials, &crate::QueryCancel::new())
    }
    pub fn test_connection_with_cancel(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        cancel: &crate::QueryCancel,
    ) -> Result<u64, QueryError> {
        let started = std::time::Instant::now();
        let mut profile = profile.clone();
        profile.config.query_timeout = profile.config.timeout.clamp(10, 60);
        let descriptor = superlink_domain::resolve(&profile.config.kind)
            .map_err(|_| QueryError::InvalidProfile)?;
        match descriptor.family {
            superlink_domain::Family::Sql | superlink_domain::Family::Search => {
                let text = if descriptor.family == superlink_domain::Family::Search {
                    "GET /_count"
                } else {
                    descriptor.default_query()
                };
                self.execute_request(
                    &profile,
                    credentials,
                    &superlink_domain::Execution {
                        revision: profile.revision,
                        text: text.into(),
                        max_rows: 1,
                        ..Default::default()
                    },
                    cancel,
                )?;
            }
            _ => {
                let _operation =
                    crate::execution::Operation::new(cancel, profile.config.query_timeout);
                let _connection = self.open_operation(&profile, credentials, cancel)?;
                if cancel.is_cancelled() {
                    return Err(QueryError::Cancelled);
                }
            }
        }
        Ok(started.elapsed().as_millis() as u64)
    }
}

/// 原生驱动默认关闭，需显式开启以保留 agent 行为作为回退。
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeFlags {
    pub sqlite: bool,
    pub postgres: bool,
    pub mysql: bool,
    pub clickhouse: bool,
    pub elasticsearch: bool,
}

impl NativeFlags {
    /// 原生驱动已是默认路径；设置 SUPERLINK_AGENTS_ONLY 可整体退回 driver agent。
    pub fn from_env() -> Self {
        if std::env::var_os("SUPERLINK_AGENTS_ONLY").is_some() {
            return Self::default();
        }
        Self {
            sqlite: true,
            postgres: true,
            mysql: true,
            clickhouse: true,
            elasticsearch: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeKind {
    Sqlite,
    Postgres,
    Mysql,
    ClickHouse,
    Elasticsearch,
}

/// 方言 → 原生驱动的路由。
///
/// PostgreSQL 协议族只纳入认证兼容的方言：Kingbase/Vastbase 使用 md5/scram。
/// GaussDB/openGauss（sha256 挑战）与 HighGo（SM3）需要专用认证，
/// tokio-postgres 无法完成握手，继续走 agent。
pub fn native_target(kind: &str, flags: NativeFlags) -> Option<NativeKind> {
    let key = kind.trim().to_ascii_lowercase();
    let key = match key.as_str() {
        "postgresql" | "pg" => "postgres",
        other => other,
    };
    match key {
        "sqlite" if flags.sqlite => Some(NativeKind::Sqlite),
        "postgres" | "kingbase" | "vastbase" if flags.postgres => Some(NativeKind::Postgres),
        // Sphinx/StarRocks 同协议但行为差异未验证，暂不纳入。
        "mysql" | "mariadb" | "oceanbase" | "tidb" if flags.mysql => Some(NativeKind::Mysql),
        "clickhouse" if flags.clickhouse => Some(NativeKind::ClickHouse),
        "elasticsearch" if flags.elasticsearch => Some(NativeKind::Elasticsearch),
        _ => None,
    }
}

/// The vault keeps the password out of metadata; the driver still needs it.
fn live_config(config: &ConnectionConfig, credentials: &Credentials) -> ConnectionConfig {
    credentials.apply(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use superlink_domain::ConnectionConfig;
    use superlink_drivers::DriverSession;

    fn drivers_dir() -> PathBuf {
        PathBuf::from(format!(
            "{}/../../../bin/drivers",
            env!("CARGO_MANIFEST_DIR")
        ))
    }

    #[test]
    fn reports_missing_agents_without_panicking() {
        let service = QueryService::new(std::env::temp_dir().join("no-such-drivers"));
        // Oracle 没有原生驱动，也缺少 agent，可用来验证优雅报错。
        assert!(!service.supports("oracle"));
        let profile = Profile {
            config: ConnectionConfig {
                kind: "oracle".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let error = service
            .execute(&profile, &Credentials::default(), "SELECT 1", 10)
            .unwrap_err();
        assert!(matches!(error, QueryError::Unsupported(_)));
    }

    fn all_on() -> NativeFlags {
        NativeFlags {
            sqlite: true,
            postgres: true,
            mysql: true,
            clickhouse: true,
            elasticsearch: true,
        }
    }

    #[test]
    fn routes_migrated_dialects_to_native_drivers() {
        assert_eq!(native_target("sqlite", all_on()), Some(NativeKind::Sqlite));
        // 目录键是 postgres，别名 postgresql 也必须命中
        assert_eq!(
            native_target("postgres", all_on()),
            Some(NativeKind::Postgres)
        );
        assert_eq!(
            native_target("PostgreSQL", all_on()),
            Some(NativeKind::Postgres)
        );
        assert_eq!(
            native_target("kingbase", all_on()),
            Some(NativeKind::Postgres)
        );
        assert_eq!(
            native_target("vastbase", all_on()),
            Some(NativeKind::Postgres)
        );
        assert_eq!(native_target("mysql", all_on()), Some(NativeKind::Mysql));
        assert_eq!(native_target("mariadb", all_on()), Some(NativeKind::Mysql));
        assert_eq!(
            native_target("oceanbase", all_on()),
            Some(NativeKind::Mysql)
        );
        assert_eq!(native_target("tidb", all_on()), Some(NativeKind::Mysql));
        assert_eq!(
            native_target("elasticsearch", all_on()),
            Some(NativeKind::Elasticsearch)
        );
        assert_eq!(
            native_target("clickhouse", all_on()),
            Some(NativeKind::ClickHouse)
        );
    }

    #[test]
    fn keeps_incompatible_authentication_on_the_agent() {
        // sha256 / SM3 认证：tokio-postgres 无法握手
        assert_eq!(native_target("gaussdb", all_on()), None);
        assert_eq!(native_target("opengauss", all_on()), None);
        assert_eq!(native_target("highgo", all_on()), None);
        // 未验证的 MySQL 兼容服务
        assert_eq!(native_target("sphinx", all_on()), None);
        assert_eq!(native_target("starrocks", all_on()), None);
    }

    #[test]
    fn flags_gate_each_driver() {
        let none = NativeFlags::default();
        assert_eq!(native_target("sqlite", none), None);
        assert_eq!(native_target("postgres", none), None);
        assert_eq!(native_target("mysql", none), None);
        let only_pg = NativeFlags {
            postgres: true,
            ..Default::default()
        };
        assert_eq!(
            native_target("kingbase", only_pg),
            Some(NativeKind::Postgres)
        );
        assert_eq!(native_target("mysql", only_pg), None);
    }

    #[test]
    fn rejects_empty_sql_before_spawning_a_process() {
        let service = QueryService::new(drivers_dir());
        let profile = Profile::default();
        assert!(matches!(
            service.execute(&profile, &Credentials::default(), "   ", 10),
            Err(QueryError::Empty)
        ));
    }

    /// 同一数据库分别走原生驱动与 Rust agent，结果必须逐值一致。
    #[test]
    fn native_driver_matches_the_agent() {
        let service = QueryService::new(drivers_dir());
        if service.agent_path("sqlite").is_none() {
            eprintln!("skipping: sqlite agent not present");
            return;
        }
        let database = std::env::temp_dir().join(format!(
            "superlink-parity-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&database);
        let config = ConnectionConfig {
            kind: "sqlite".into(),
            database: database.to_string_lossy().to_string(),
            ..Default::default()
        };
        let profile = Profile {
            config: config.clone(),
            ..Default::default()
        };
        let credentials = Credentials::default();
        service
            .execute(
                &profile,
                &credentials,
                "CREATE TABLE p(id INTEGER, name TEXT, score REAL, note TEXT)",
                10,
            )
            .unwrap();
        service
            .execute(
                &profile,
                &credentials,
                "INSERT INTO p VALUES (1,'a',1.5,NULL),(2,'b',2.5,'x')",
                10,
            )
            .unwrap();

        // agent 路径
        let mut session =
            DriverSession::open(service.agent_path("sqlite").expect("agent"), "sqlite").unwrap();
        session.connect(&config).unwrap();
        let via_agent = session
            .query("SELECT id, name, score, note FROM p ORDER BY id", &[], 10)
            .unwrap();
        let _ = session.close();

        // 原生路径
        let native = NativeSqlite::open(&config).unwrap();
        let via_native = native
            .query("SELECT id, name, score, note FROM p ORDER BY id", &[], 10)
            .unwrap();

        assert_eq!(via_agent.columns, via_native.columns);
        assert_eq!(via_agent.rows, via_native.rows);
        assert_eq!(via_agent.truncated, via_native.truncated);
        let _ = std::fs::remove_file(&database);
    }

    #[test]
    fn executes_against_the_real_sqlite_agent() {
        let service = QueryService::new(drivers_dir());
        if service.agent_path("sqlite").is_none() {
            eprintln!("skipping: sqlite agent not present");
            return;
        }
        let database =
            std::env::temp_dir().join(format!("superlink-query-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&database);
        let profile = Profile {
            config: ConnectionConfig {
                kind: "sqlite".into(),
                database: database.to_string_lossy().to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let credentials = Credentials::default();
        service
            .execute(&profile, &credentials, "CREATE TABLE t(a INTEGER)", 10)
            .unwrap();
        service
            .execute(&profile, &credentials, "INSERT INTO t VALUES (7)", 10)
            .unwrap();
        let result = service
            .execute(&profile, &credentials, "SELECT a FROM t", 10)
            .unwrap();
        assert_eq!(result.rows, vec![vec![json!(7)]]);
        let _ = std::fs::remove_file(&database);
    }

    #[test]
    fn injects_the_password_only_into_the_live_config() {
        let config = ConnectionConfig {
            password: String::new(),
            ..Default::default()
        };
        let credentials = Credentials {
            password: "secret".into(),
            ..Default::default()
        };
        let live = live_config(&config, &credentials);
        assert_eq!(live.password, "secret");
        assert!(config.password.is_empty());
    }
}
