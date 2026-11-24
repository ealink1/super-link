//! Complete query operation: validate first, bind values, reuse one connection,
//! enforce deadlines and release cancellation registrations before returning.
use crate::execution_policy::{authorize, split_sql};
use crate::query::{NativeFlags, NativeKind, native_target};
use crate::{Credentials, QueryError, QueryService};
use serde_json::Value;
use std::sync::mpsc;
use std::time::Duration;
use superlink_domain::{Execution, Family, Profile, ResultSet, resolve};
use superlink_drivers::{
    DriverSession, NativeClickHouse, NativeElasticsearch, NativeMysql, NativePostgres,
    NativeSqlite, QueryCancel,
};

pub(crate) enum Connection {
    Sqlite(NativeSqlite),
    Postgres(NativePostgres),
    Mysql(NativeMysql),
    ClickHouse(NativeClickHouse),
    Elasticsearch(NativeElasticsearch),
    Agent(DriverSession),
}

impl Connection {
    pub(crate) fn transactional(&self) -> bool {
        match self {
            Self::Sqlite(_) | Self::Postgres(_) | Self::Mysql(_) => true,
            Self::Agent(agent) => agent.has_sql_session(),
            _ => false,
        }
    }
    pub(crate) fn query(
        &mut self,
        sql: &str,
        args: &[Value],
        limit: u32,
    ) -> Result<ResultSet, QueryError> {
        Ok(match self {
            Self::Sqlite(d) => d.query(sql, args, limit)?,
            Self::Postgres(d) => d.query(sql, args, limit)?,
            Self::Mysql(d) => d.query(sql, args, limit)?,
            Self::ClickHouse(d) => d.query(sql, args, limit)?,
            Self::Elasticsearch(d) => d.query(sql, args, limit)?,
            Self::Agent(d) => {
                let family = superlink_domain::resolve(d.kind())
                    .map(|d| d.family)
                    .unwrap_or(superlink_domain::Family::Sql);
                if family == Family::Search {
                    return Ok(d.elasticsearch_console(sql)?);
                }
                let first = superlink_sql::tokenize(sql)
                    .into_iter()
                    .find(|t| {
                        !matches!(t.kind, superlink_sql::TokenKind::Comment)
                            && !t.text.trim().is_empty()
                    })
                    .map(|t| t.text.to_ascii_uppercase())
                    .unwrap_or_default();
                let write = if family == superlink_domain::Family::Sql {
                    !matches!(
                        first.as_str(),
                        "SELECT"
                            | "WITH"
                            | "SHOW"
                            | "DESCRIBE"
                            | "DESC"
                            | "EXPLAIN"
                            | "PRAGMA"
                            | "VALUES"
                    ) && !sql.to_ascii_uppercase().contains("RETURNING")
                } else {
                    matches!(family, superlink_domain::Family::Message)
                        && serde_json::from_str::<Value>(sql).is_ok_and(|v| {
                            v.get("publish").is_some()
                                || v.get("op").and_then(Value::as_str).is_some_and(|op| {
                                    matches!(op, "publish" | "send" | "delete" | "ack")
                                })
                        })
                };
                if write {
                    d.exec(sql, args)?
                } else {
                    d.query(sql, args, limit)?
                }
            }
        })
    }
}

pub(crate) struct Operation {
    cancel: QueryCancel,
    stop: Option<mpsc::Sender<()>>,
    timer: Option<std::thread::JoinHandle<()>>,
}

impl Operation {
    pub(crate) fn new(cancel: &QueryCancel, seconds: u32) -> Self {
        cancel.begin();
        let mut operation = Self {
            cancel: cancel.clone(),
            stop: None,
            timer: None,
        };
        if seconds > 0 {
            let (sender, receiver) = mpsc::channel();
            let cancel = cancel.clone();
            operation.stop = Some(sender);
            operation.timer = Some(std::thread::spawn(move || {
                if matches!(
                    receiver.recv_timeout(Duration::from_secs(seconds.into())),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    cancel.cancel();
                }
            }));
        }
        operation
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if let Some(sender) = self.stop.take() {
            let _ = sender.send(());
        }
        if let Some(timer) = self.timer.take() {
            let _ = timer.join();
        }
        self.cancel.clear();
        self.cancel.finish();
    }
}

impl QueryService {
    pub fn execute_request(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        request: &Execution,
        cancel: &QueryCancel,
    ) -> Result<Vec<ResultSet>, QueryError> {
        if request.revision != 0 && request.revision != profile.revision {
            return Err(QueryError::Policy("连接配置已修改，请重新执行".into()));
        }
        let mut current = profile.clone();
        crate::scope::apply(&mut current.config, &request.scope).map_err(QueryError::Policy)?;
        let dialect = current.sql_dialect();
        let sql_family = resolve(&current.config.kind)
            .map(|d| d.family == Family::Sql)
            .unwrap_or(true);
        let statements = if sql_family {
            split_sql(&request.text, dialect).map_err(QueryError::Policy)?
        } else if request.text.trim().is_empty() {
            return Err(QueryError::Empty);
        } else {
            vec![request.text.clone()]
        };
        for sql in &statements {
            authorize(&current, sql, request.write, &request.action).map_err(QueryError::Policy)?;
        }
        if !request.args.is_empty() && statements.len() != 1 {
            return Err(QueryError::Policy("绑定参数时请选中一条语句执行".into()));
        }
        let prepared: Vec<_> = statements
            .iter()
            .map(|sql| bind_named(sql, dialect, &request.parameters))
            .collect::<Result<_, _>>()
            .map_err(QueryError::Policy)?;
        let _operation = Operation::new(cancel, current.config.query_timeout);
        check_cancel(cancel)?;
        let mut connection = if current.config.kind == "elasticsearch"
            && !NativeElasticsearch::supports_request(&request.text)
        {
            Connection::Agent(self.open_agent_operation(&current, credentials, cancel)?)
        } else {
            self.open_operation(&current, credentials, cancel)?
        };
        check_cancel(cancel)?;
        if !request.schema.is_empty()
            && matches!(
                native_target(dialect, NativeFlags::from_env()),
                Some(NativeKind::Postgres)
            )
        {
            let quoted = superlink_sql::quote_identifier(dialect, &request.schema)
                .map_err(|e| QueryError::Policy(e.to_string()))?;
            connection.query(&format!("SET search_path TO {quoted}"), &[], 1)?;
        }
        let limit = request.max_rows.clamp(1, 100_000);
        let mut results = Vec::new();
        let mut remaining = limit;
        let mut memory = superlink_drivers::budget::ResultBudget::default();
        for (sql, named) in prepared {
            check_cancel(cancel)?;
            let args = if request.args.is_empty() {
                &named
            } else {
                &request.args
            };
            let result = connection.query(&sql, args, remaining.max(1));
            check_cancel(cancel)?;
            let mut result = result?;
            if remaining == 0 {
                result.rows.clear();
                result.truncated = true;
            }
            let allowed = result
                .rows
                .iter()
                .take_while(|row| memory.accept(row))
                .count();
            if allowed < result.rows.len() {
                result.rows.truncate(allowed);
                result.truncated = true;
            }
            remaining = remaining.saturating_sub(result.rows.len() as u32);
            results.push(result);
        }
        Ok(results)
    }

    pub(crate) fn open_agent_operation(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        cancel: &QueryCancel,
    ) -> Result<DriverSession, QueryError> {
        let kind = profile.config.kind.as_str();
        let binary = self
            .agent_path(kind)
            .ok_or_else(|| QueryError::Unsupported(kind.into()))?;
        let mut session = DriverSession::open_with_cancel(binary, kind, cancel)?;
        session.set_cancellation(cancel);
        session.connect(&credentials.apply(&profile.config))?;
        if superlink_domain::resolve(&profile.config.kind).is_ok_and(|d| d.family == Family::Sql) {
            session.open_sql_session()?;
        }
        Ok(session)
    }
    pub(crate) fn open_operation(
        &self,
        profile: &Profile,
        credentials: &Credentials,
        cancel: &QueryCancel,
    ) -> Result<Connection, QueryError> {
        let config = credentials.apply(&profile.config);
        let kind = profile.sql_dialect();
        let target = if config.use_ssh
            || config.use_proxy
            || config.use_http_tunnel
            || !config.dsn.is_empty()
            || !config.uri.is_empty()
        {
            None
        } else {
            native_target(kind, NativeFlags::from_env())
        };
        Ok(match target {
            Some(NativeKind::Sqlite) => {
                let driver = NativeSqlite::open(&config)?;
                driver.set_cancellation(cancel);
                Connection::Sqlite(driver)
            }
            Some(NativeKind::Postgres) => {
                let driver = NativePostgres::open(&config)?;
                driver.set_cancellation(cancel);
                Connection::Postgres(driver)
            }
            Some(NativeKind::Mysql) => {
                let driver = NativeMysql::open(&config)?;
                driver.set_cancellation(cancel, &config);
                Connection::Mysql(driver)
            }
            Some(NativeKind::ClickHouse) => {
                let mut driver = NativeClickHouse::open(&config)?;
                driver.set_cancellation(cancel);
                Connection::ClickHouse(driver)
            }
            Some(NativeKind::Elasticsearch) => {
                let mut driver = NativeElasticsearch::open(&config)?;
                driver.set_cancellation(cancel);
                Connection::Elasticsearch(driver)
            }
            None => Connection::Agent(self.open_agent_operation(profile, credentials, cancel)?),
        })
    }
}

fn check_cancel(cancel: &QueryCancel) -> Result<(), QueryError> {
    if cancel.is_cancelled() {
        Err(QueryError::Cancelled)
    } else {
        Ok(())
    }
}

/// Named placeholders are rewritten outside strings/comments/quoted names.
pub fn bind_named(
    sql: &str,
    dialect: &str,
    parameters: &serde_json::Map<String, Value>,
) -> Result<(String, Vec<Value>), String> {
    let tokens = superlink_sql::tokenize(sql);
    let mut out = String::new();
    let mut args = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if token.text == ":" && tokens.get(index + 1).is_some_and(|t| t.text == ":") {
            out.push_str("::");
            index += 2;
            continue;
        }
        if token.text == ":"
            && let Some(next) = tokens.get(index + 1)
        {
            let name = &next.text;
            if name
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                let value = parameters
                    .get(name)
                    .ok_or_else(|| format!("请填写参数：{name}"))?;
                args.push(
                    if value.get("type").and_then(Value::as_str) == Some("binary") {
                        value.clone()
                    } else {
                        value.get("value").unwrap_or(value).clone()
                    },
                );
                out.push_str(&superlink_sql::placeholder(dialect, args.len()));
                index += 2;
                continue;
            }
        }
        out.push_str(&token.text);
        index += 1;
    }
    Ok((out, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn dollar_quoted_literals_do_not_bind_or_trigger_writes() {
        let sql = "SELECT $body$:missing DELETE FROM t$body$, :id";
        let parameters = serde_json::json!({"id":7});
        let (bound, args) = bind_named(sql, "postgres", parameters.as_object().unwrap()).unwrap();
        assert_eq!(bound, "SELECT $body$:missing DELETE FROM t$body$, $1");
        assert_eq!(args, vec![serde_json::json!(7)]);
        assert!(!crate::execution_policy::is_write(sql));
    }
    #[test]
    fn cancels_sqlite_before_or_during_a_long_statement() {
        let service = QueryService::new("/nonexistent");
        let profile = Profile {
            config: superlink_domain::ConnectionConfig {
                kind: "sqlite".into(),
                database: ":memory:".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let request = Execution { text: "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x) FROM n".into(), max_rows: 1, ..Default::default() };
        let cancel = QueryCancel::new();
        let other = cancel.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            other.cancel();
        });
        let started = std::time::Instant::now();
        assert!(matches!(
            service.execute_request(&profile, &Credentials::default(), &request, &cancel),
            Err(QueryError::Cancelled)
        ));
        thread.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn binds_names_without_touching_literals_or_casts() {
        let params = json!({"id": 9007199254740993i64});
        let (sql, args) = bind_named(
            "SELECT :id::bigint, ':id' -- :other",
            "postgres",
            params.as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(sql, "SELECT $1::bigint, ':id' -- :other");
        assert_eq!(args, vec![json!(9007199254740993i64)]);
    }
    #[test]
    fn sqlite_operation_keeps_a_transaction_and_binds_parameters() {
        let service = QueryService::new("/missing");
        let profile = Profile {
            config: superlink_domain::ConnectionConfig {
                kind: "sqlite".into(),
                database: ":memory:".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let request = Execution {
            text:
                "BEGIN; CREATE TABLE t(id); INSERT INTO t VALUES (:id); SELECT id FROM t; ROLLBACK;"
                    .into(),
            parameters: json!({"id":42}).as_object().unwrap().clone(),
            max_rows: 100,
            write: true,
            ..Default::default()
        };
        let results = service
            .execute_request(
                &profile,
                &Credentials::default(),
                &request,
                &QueryCancel::new(),
            )
            .unwrap();
        assert_eq!(results[3].rows, vec![vec![json!(42)]]);
    }
    #[test]
    fn readonly_is_enforced_before_a_database_file_is_created() {
        let path =
            std::env::temp_dir().join(format!("superlink-denied-{}.db", uuid::Uuid::new_v4()));
        let profile = Profile {
            read_only: true,
            config: superlink_domain::ConnectionConfig {
                kind: "sqlite".into(),
                database: path.to_string_lossy().into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let request = Execution {
            text: "CREATE TABLE t(id)".into(),
            write: true,
            ..Default::default()
        };
        assert!(
            QueryService::new("/missing")
                .execute_request(
                    &profile,
                    &Credentials::default(),
                    &request,
                    &QueryCancel::new()
                )
                .is_err()
        );
        assert!(!path.exists());
    }
}
