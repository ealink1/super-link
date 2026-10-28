use std::path::{Path, PathBuf};

use superlink_domain::{ConnectionConfig, Profile, ResultSet};
use superlink_drivers::{
    DriverError, DriverSession, NativeClickHouse, NativeElasticsearch, NativeMysql, NativePostgres,
    NativeSqlite, TableEntry, TableStructure,
};

use crate::profiles::Credentials;

const DEFAULT_MAX_ROWS: u32 = 1_000;

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
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
pub struct QueryService {
    drivers_dir: PathBuf,
}

impl QueryService {
    pub fn new(drivers_dir: impl Into<PathBuf>) -> Self {
        Self {
            drivers_dir: drivers_dir.into(),
        }
    }

    pub fn drivers_dir(&self) -> &Path {
        &self.drivers_dir
    }

    pub fn agent_path(&self, kind: &str) -> Option<PathBuf> {
        let suffix = if cfg!(target_os = "windows") {
            ".exe"
        } else {
            ""
        };
        let path = self
            .drivers_dir
            .join(format!("{kind}-driver-agent{suffix}"));
        path.is_file().then_some(path)
    }

    pub fn supports(&self, kind: &str) -> bool {
        self.agent_path(kind).is_some()
    }

    /// 结构浏览：目前只有原生 SQLite 提供；其余方言仍由 agent 承担。
    pub fn tables(&self, profile: &Profile) -> Result<Vec<TableEntry>, QueryError> {
        let kind = profile.sql_dialect().to_string();
        match native_target(&kind, NativeFlags::from_env()) {
            Some(NativeKind::Sqlite) => Ok(NativeSqlite::open(&profile.config)?.tables()?),
            Some(NativeKind::Mysql) => Ok(NativeMysql::open(&profile.config)?.tables()?),
            Some(NativeKind::Postgres) => Ok(NativePostgres::open(&profile.config)?.tables()?),
            _ => Err(QueryError::Unsupported(kind)),
        }
    }

    /// 单表的列定义；与 tables 一样目前只支持原生 SQLite。
    pub fn structure(&self, profile: &Profile, table: &str) -> Result<TableStructure, QueryError> {
        let kind = profile.sql_dialect().to_string();
        match native_target(&kind, NativeFlags::from_env()) {
            Some(NativeKind::Sqlite) => Ok(NativeSqlite::open(&profile.config)?.structure(table)?),
            Some(NativeKind::Mysql) => Ok(NativeMysql::open(&profile.config)?.structure(table)?),
            Some(NativeKind::Postgres) => {
                Ok(NativePostgres::open(&profile.config)?.structure(table)?)
            }
            _ => Err(QueryError::Unsupported(kind)),
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
        let kind = profile.sql_dialect().to_string();
        let config = live_config(&profile.config, credentials);
        // 已移植的方言走进程内驱动，其余仍需 agent 二进制。
        match native_target(&kind, NativeFlags::from_env()) {
            Some(NativeKind::Sqlite) => {
                let driver = NativeSqlite::open(&config)?;
                return Ok(driver.query(sql, &[], max_rows)?);
            }
            Some(NativeKind::Postgres) => {
                let mut driver = NativePostgres::open(&config)?;
                return Ok(driver.query(sql, &[], max_rows)?);
            }
            Some(NativeKind::Mysql) => {
                let mut driver = NativeMysql::open(&config)?;
                return Ok(driver.query(sql, &[], max_rows)?);
            }
            Some(NativeKind::ClickHouse) => {
                let mut driver = NativeClickHouse::open(&config)?;
                return Ok(driver.query(sql, &[], max_rows)?);
            }
            Some(NativeKind::Elasticsearch) => {
                let mut driver = NativeElasticsearch::open(&config)?;
                return Ok(driver.query(sql, &[], max_rows)?);
            }
            None => {}
        }
        let binary = self
            .agent_path(&kind)
            .ok_or_else(|| QueryError::Unsupported(kind.clone()))?;
        let budget = if max_rows == 0 {
            DEFAULT_MAX_ROWS
        } else {
            max_rows
        };

        let mut session = DriverSession::open(&binary, &kind)?;
        session.connect(&config)?;
        let result = session.query(sql, &[], budget)?;
        let _ = session.close();
        Ok(result)
    }
}

/// 原生驱动默认关闭，需显式开启以保留 agent 行为作为回退。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeFlags {
    pub sqlite: bool,
    pub postgres: bool,
    pub mysql: bool,
    pub clickhouse: bool,
    pub elasticsearch: bool,
}

impl NativeFlags {
    fn from_env() -> Self {
        let on = |key: &str| std::env::var_os(key).is_some();
        Self {
            sqlite: on("SUPERLINK_NATIVE_SQLITE"),
            postgres: on("SUPERLINK_NATIVE_PG"),
            mysql: on("SUPERLINK_NATIVE_MYSQL"),
            clickhouse: on("SUPERLINK_NATIVE_CLICKHOUSE"),
            elasticsearch: on("SUPERLINK_NATIVE_ES"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeKind {
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
pub(crate) fn native_target(kind: &str, flags: NativeFlags) -> Option<NativeKind> {
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
    let mut live = config.clone();
    live.password = credentials.password.clone();
    live
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use superlink_domain::ConnectionConfig;

    fn drivers_dir() -> PathBuf {
        PathBuf::from(format!("{}/../../bin/drivers", env!("CARGO_MANIFEST_DIR")))
    }

    #[test]
    fn reports_missing_agents_without_panicking() {
        let service = QueryService::new(std::env::temp_dir().join("no-such-drivers"));
        assert!(!service.supports("sqlite"));
        let profile = Profile {
            config: ConnectionConfig {
                kind: "sqlite".into(),
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

    /// 同一数据库分别走原生驱动与 Go agent，结果必须逐值一致。
    #[test]
    fn native_driver_matches_the_agent() {
        let service = QueryService::new(drivers_dir());
        if !service.supports("sqlite") {
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
        if !service.supports("sqlite") {
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
