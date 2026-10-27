use std::path::{Path, PathBuf};

use superlink_domain::{ConnectionConfig, Profile, ResultSet};
use superlink_drivers::{DriverError, DriverSession};

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
        let binary = self
            .agent_path(&kind)
            .ok_or_else(|| QueryError::Unsupported(kind.clone()))?;
        let config = live_config(&profile.config, credentials);
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

    #[test]
    fn rejects_empty_sql_before_spawning_a_process() {
        let service = QueryService::new(drivers_dir());
        let profile = Profile::default();
        assert!(matches!(
            service.execute(&profile, &Credentials::default(), "   ", 10),
            Err(QueryError::Empty)
        ));
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
