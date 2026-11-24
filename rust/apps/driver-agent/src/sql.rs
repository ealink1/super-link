use serde_json::Value;
use superlink_domain::{ConnectionConfig, ResultSet};
use superlink_drivers::{
    NativeClickHouse, NativeMysql, NativePostgres, NativeSqlite, QueryCancel, TableEntry,
    TableStructure,
};

pub enum Sql {
    Sqlite(NativeSqlite),
    Mysql(NativeMysql),
    Postgres(NativePostgres),
    ClickHouse(NativeClickHouse),
    Redis(crate::redis::Redis),
    Mongo(crate::mongo::Mongo),
    Rest(crate::http::Rest),
    Oracle(crate::oracle::Oracle),
    Dameng(crate::dameng::Dameng),
    SqlServer(crate::sqlserver::SqlServer),
    Kafka(crate::kafka::Kafka),
    Mqtt(crate::mqtt::Mqtt),
    Pulsar(crate::pulsar::PulsarDriver),
    RocketMq(crate::rocketmq::RocketMq),
    Iris(crate::iris::Iris),
    DuckDb(crate::duckdb::DuckDb),
    IotDb(crate::iotdb::IotDb),
    Gauss(crate::gauss::Gauss),
    Navicat(crate::navicat::Navicat),
}
pub fn protocol_kind(config: &ConnectionConfig) -> &str {
    match config.kind.as_str() {
        "mysql" | "goldendb" | "mariadb" | "diros" | "starrocks" | "sphinx" => "mysql",
        "oceanbase" if !config.oceanbase_protocol.eq_ignore_ascii_case("oracle") => "mysql",
        "postgres" | "kingbase" | "vastbase" => "postgres",
        "opengauss" | "gaussdb" | "highgo" => "gauss",
        "custom" => match config.driver.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "pgx" | "pq" => "postgres",
            "sqlite" | "sqlite3" => "sqlite",
            "mssql" | "sqlserver" => "sqlserver",
            "oracle" | "oci8" | "go-ora" | "godror" => "oracle",
            "dm" | "dameng" => "dameng",
            "mysql" => "mysql",
            "duckdb" => "duckdb",
            _ => config.driver.as_str(),
        },
        kind => kind,
    }
}
impl Sql {
    pub fn open(config: &ConnectionConfig, cancel: &QueryCancel) -> Result<Self, String> {
        if crate::navicat::Navicat::configured(config) {
            if protocol_kind(config) != "mysql" {
                return Err("脚本式 HTTP 隧道仅支持 MySQL 协议".into());
            }
            return Ok(Self::Navicat(crate::navicat::Navicat::open(
                config, cancel,
            )?));
        }
        let driver = match protocol_kind(config) {
            "sqlite" => {
                let d = NativeSqlite::open(config).map_err(|e| e.to_string())?;
                d.set_cancellation(cancel);
                Self::Sqlite(d)
            }
            "mysql" => {
                let d = NativeMysql::open(config).map_err(|e| e.to_string())?;
                d.set_cancellation(cancel, config);
                Self::Mysql(d)
            }
            "postgres" => {
                let d = NativePostgres::open(config).map_err(|e| e.to_string())?;
                d.set_cancellation(cancel);
                Self::Postgres(d)
            }
            "clickhouse" => {
                let mut d = NativeClickHouse::open(config).map_err(|e| e.to_string())?;
                d.set_cancellation(cancel);
                Self::ClickHouse(d)
            }
            "redis" => Self::Redis(crate::redis::Redis::open(config)?),
            "mongodb" => Self::Mongo(crate::mongo::Mongo::open(config)?),
            "qdrant" | "chroma" | "milvus" | "nacos" | "rabbitmq" | "tdengine" | "trino"
            | "elasticsearch" => Self::Rest(crate::http::Rest::open(config, cancel)?),
            "oracle" | "oceanbase" => Self::Oracle(crate::oracle::Oracle::open(config)?),
            "dameng" => Self::Dameng(crate::dameng::Dameng::open(config)?),
            "sqlserver" => Self::SqlServer(crate::sqlserver::SqlServer::open(config)?),
            "kafka" => Self::Kafka(crate::kafka::Kafka::open(config, cancel)?),
            "mqtt" => Self::Mqtt(crate::mqtt::Mqtt::open(config, cancel)?),
            "pulsar" => Self::Pulsar(crate::pulsar::PulsarDriver::open(config, cancel)?),
            "rocketmq" => Self::RocketMq(crate::rocketmq::RocketMq::open(config, cancel)?),
            "iris" | "cache" => Self::Iris(crate::iris::Iris::open(config, cancel)?),
            "duckdb" => {
                let d = crate::duckdb::DuckDb::open(config)?;
                d.set_cancellation(cancel);
                Self::DuckDb(d)
            }
            "iotdb" => Self::IotDb(crate::iotdb::IotDb::open(config)?),
            "gauss" => Self::Gauss(crate::gauss::Gauss::open(config, cancel)?),
            _ => return Err("该 Rust 驱动尚未迁移完成".into()),
        };
        Ok(driver)
    }
    pub fn transactional(&self) -> bool {
        matches!(
            self,
            Self::Sqlite(_)
                | Self::Mysql(_)
                | Self::Postgres(_)
                | Self::Oracle(_)
                | Self::Dameng(_)
                | Self::SqlServer(_)
                | Self::Iris(_)
                | Self::DuckDb(_)
                | Self::Gauss(_)
        )
    }
    pub fn query(&mut self, text: &str, args: &[Value], limit: u32) -> Result<ResultSet, String> {
        match self {
            Self::Redis(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::Mongo(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::Rest(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::Oracle(d) => return d.query(text, args, limit),
            Self::Dameng(d) => return d.query(text, args, limit),
            Self::SqlServer(d) => return d.query(text, args, limit),
            Self::Kafka(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::Mqtt(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::Pulsar(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::RocketMq(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::IotDb(d) => {
                no_arguments(args)?;
                return d.query(text, limit);
            }
            Self::Iris(d) => return d.query(text, args, limit),
            Self::DuckDb(d) => return d.query(text, args, limit),
            Self::Gauss(d) => return d.query(text, args, limit),
            Self::Navicat(d) => return d.query(text, args, limit),
            _ => {}
        }
        match self {
            Self::Sqlite(d) => d.query(text, args, limit),
            Self::Mysql(d) => d.query(text, args, limit),
            Self::Postgres(d) => d.query(text, args, limit),
            Self::ClickHouse(d) => d.query(text, args, limit),
            _ => unreachable!(),
        }
        .map_err(|e| e.to_string())
    }
    pub fn databases(&mut self) -> Result<Vec<String>, String> {
        match self {
            Self::Redis(d) => return d.databases(),
            Self::Navicat(_) => {
                return Ok(crate::rows::list(self.query(
                    "SHOW DATABASES",
                    &[],
                    10000,
                )?));
            }
            Self::Mongo(d) => return d.databases(),
            Self::Rest(d) => return d.databases(),
            Self::Kafka(_) | Self::Mqtt(_) | Self::Pulsar(_) | Self::RocketMq(_) => {
                return Ok(vec!["topics".into()]);
            }
            Self::Gauss(_) => {
                return Ok(crate::rows::list(self.query(
                    "SELECT datname FROM pg_database WHERE datallowconn",
                    &[],
                    10000,
                )?));
            }
            Self::DuckDb(_) => {
                return Ok(crate::rows::list(self.query(
                    "SELECT database_name FROM duckdb_databases()",
                    &[],
                    10000,
                )?));
            }
            Self::Iris(d) => return d.databases(),
            Self::IotDb(_) => {
                return Ok(crate::rows::list(self.query(
                    "SHOW DATABASES",
                    &[],
                    10000,
                )?));
            }
            Self::Oracle(_) | Self::Dameng(_) => {
                return Ok(crate::rows::list(self.query(
                    "SELECT username FROM all_users",
                    &[],
                    10000,
                )?));
            }
            Self::SqlServer(_) => {
                return Ok(crate::rows::list(self.query(
                    "SELECT name FROM sys.databases",
                    &[],
                    10000,
                )?));
            }
            Self::ClickHouse(_) => {
                return Ok(crate::rows::list(self.query(
                    "SHOW DATABASES",
                    &[],
                    10000,
                )?));
            }
            _ => {}
        }
        match self {
            Self::Sqlite(d) => d.databases(),
            Self::Mysql(d) => d.databases(),
            Self::Postgres(d) => d.databases(),
            _ => return Ok(vec![]),
        }
        .map_err(|e| e.to_string())
    }
    pub fn tables(&mut self, database: &str) -> Result<Vec<TableEntry>, String> {
        let names = match self {
            Self::Redis(d) => Some(d.tables(database)?),
            Self::Mongo(d) => Some(d.tables(database)?),
            Self::Rest(d) => Some(d.tables(database)?),
            Self::Kafka(d) => Some(d.tables()?),
            Self::Mqtt(d) => Some(d.tables()),
            Self::Pulsar(d) => Some(d.tables()?),
            Self::RocketMq(d) => Some(d.tables()?),
            Self::Iris(_) | Self::DuckDb(_) | Self::IotDb(_) | Self::Gauss(_) => {
                Some(self.generic_tables(database)?)
            }
            Self::Oracle(_) | Self::Dameng(_) | Self::SqlServer(_) | Self::ClickHouse(_) => {
                Some(self.generic_tables(database)?)
            }
            Self::Navicat(_) => Some(self.generic_tables(database)?),
            _ => None,
        };
        if let Some(names) = names {
            return Ok(names
                .into_iter()
                .map(|name| TableEntry {
                    name,
                    kind: "object".into(),
                })
                .collect());
        }
        match self {
            Self::Sqlite(d) => d.tables(),
            Self::Mysql(d) => d.tables(database),
            Self::Postgres(d) => d.tables(),
            _ => return Err("目录实现尚未迁移".into()),
        }
        .map_err(|e| e.to_string())
    }
    pub fn structure(&mut self, database: &str, table: &str) -> Result<TableStructure, String> {
        if matches!(
            self,
            Self::Oracle(_)
                | Self::Dameng(_)
                | Self::SqlServer(_)
                | Self::ClickHouse(_)
                | Self::Iris(_)
                | Self::DuckDb(_)
                | Self::IotDb(_)
                | Self::Gauss(_)
                | Self::Navicat(_)
        ) {
            return self.generic_structure(database, table);
        }
        match self {
            Self::Sqlite(d) => d.structure(table),
            Self::Mysql(d) => d.structure(database, table),
            Self::Postgres(d) => d.structure(table),
            _ => return self.object_structure(database, table),
        }
        .map_err(|e| e.to_string())
    }
}
fn no_arguments(args: &[Value]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("此协议使用命令 JSON，不支持 SQL 参数绑定".into())
    }
}
