use crate::error::{DomainError, DomainResult};

/// Data source family drives the default query and the editor mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Sql,
    Document,
    Search,
    Vector,
    Cache,
    Message,
    Configuration,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Sql => "sql",
            Family::Document => "document",
            Family::Search => "search",
            Family::Vector => "vector",
            Family::Cache => "cache",
            Family::Message => "message",
            Family::Configuration => "configuration",
        }
    }
}

/// One selectable data source. Mirrors the Go descriptor catalog.
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    pub key: &'static str,
    pub name: &'static str,
    pub family: Family,
    pub port: Option<u16>,
    pub agent: bool,
    pub example: Option<&'static str>,
}

impl Descriptor {
    pub fn default_query(self) -> &'static str {
        match (self.example, self.family) {
            (Some(example), _) => example,
            (None, Family::Sql) => "SELECT 1;",
            _ => "",
        }
    }
}

const fn sql(key: &'static str, name: &'static str, port: u16, agent: bool) -> Descriptor {
    Descriptor {
        key,
        name,
        family: Family::Sql,
        port: Some(port),
        agent,
        example: None,
    }
}

const fn plain(key: &'static str, name: &'static str, family: Family, port: u16) -> Descriptor {
    Descriptor {
        key,
        name,
        family,
        port: Some(port),
        agent: false,
        example: None,
    }
}

/// Ordered exactly like the Go catalog so the picker stays stable.
pub const CATALOG: &[Descriptor] = &[
    sql("mysql", "MySQL", 3306, false),
    sql("goldendb", "GoldenDB", 1523, false),
    sql("postgres", "PostgreSQL", 5432, false),
    Descriptor {
        key: "oracle",
        name: "Oracle",
        family: Family::Sql,
        port: Some(1521),
        agent: false,
        example: Some("SELECT 1 FROM dual;"),
    },
    sql("mariadb", "MariaDB", 3306, true),
    sql("oceanbase", "OceanBase", 2881, true),
    sql("diros", "Apache Doris", 9030, true),
    sql("starrocks", "StarRocks", 9030, true),
    Descriptor {
        key: "sphinx",
        name: "Sphinx",
        family: Family::Sql,
        port: Some(9306),
        agent: true,
        example: Some("SHOW TABLES"),
    },
    sql("sqlserver", "SQL Server", 1433, true),
    Descriptor {
        key: "sqlite",
        name: "SQLite",
        family: Family::Sql,
        port: None,
        agent: true,
        example: None,
    },
    sql("duckdb", "DuckDB", 0, true),
    sql("dameng", "达梦 Dameng", 5236, true),
    sql("kingbase", "金仓 Kingbase", 54321, true),
    sql("highgo", "瀚高 HighGo", 5866, true),
    sql("vastbase", "海量 Vastbase", 5432, true),
    sql("opengauss", "openGauss", 5432, true),
    sql("gaussdb", "GaussDB", 5432, true),
    sql("iris", "InterSystems IRIS", 1972, true),
    sql("cache", "InterSystems Caché", 1972, true),
    Descriptor {
        key: "mongodb",
        name: "MongoDB",
        family: Family::Document,
        port: Some(27017),
        agent: true,
        example: Some(r#"{"find":"collection_name","filter":{},"limit":100}"#),
    },
    sql("tdengine", "TDengine", 6041, true),
    Descriptor {
        key: "iotdb",
        name: "Apache IoTDB",
        family: Family::Sql,
        port: Some(6667),
        agent: true,
        example: Some("SHOW DATABASES"),
    },
    sql("clickhouse", "ClickHouse", 8123, true),
    Descriptor {
        key: "elasticsearch",
        name: "Elasticsearch",
        family: Family::Search,
        port: Some(9200),
        agent: true,
        example: Some("GET /_cat/indices?format=json"),
    },
    sql("trino", "Trino", 8080, true),
    Descriptor {
        key: "redis",
        name: "Redis",
        family: Family::Cache,
        port: Some(6379),
        agent: false,
        example: Some("PING"),
    },
    plain("chroma", "Chroma", Family::Vector, 8000),
    plain("qdrant", "Qdrant", Family::Vector, 6333),
    plain("milvus", "Milvus", Family::Vector, 19530),
    Descriptor {
        key: "rocketmq",
        name: "RocketMQ",
        family: Family::Message,
        port: Some(9876),
        agent: false,
        example: Some("SHOW TOPICS"),
    },
    Descriptor {
        key: "mqtt",
        name: "MQTT",
        family: Family::Message,
        port: Some(1883),
        agent: false,
        example: Some("SHOW TOPICS"),
    },
    Descriptor {
        key: "kafka",
        name: "Kafka",
        family: Family::Message,
        port: Some(9092),
        agent: false,
        example: Some("SHOW TOPICS"),
    },
    Descriptor {
        key: "rabbitmq",
        name: "RabbitMQ",
        family: Family::Message,
        port: Some(15672),
        agent: false,
        example: Some("SHOW QUEUES"),
    },
    Descriptor {
        key: "pulsar",
        name: "Pulsar",
        family: Family::Message,
        port: Some(6650),
        agent: false,
        example: Some("SHOW TOPICS"),
    },
    Descriptor {
        key: "nacos",
        name: "Nacos",
        family: Family::Configuration,
        port: Some(8848),
        agent: false,
        example: Some(r#"{"op":"configs","namespace":"","group":"DEFAULT_GROUP"}"#),
    },
    Descriptor {
        key: "custom",
        name: "Custom Driver / DSN",
        family: Family::Sql,
        port: None,
        agent: false,
        example: None,
    },
];

pub fn catalog() -> &'static [Descriptor] {
    CATALOG
}

/// Resolves user input, accepting the same aliases as the Go implementation.
pub fn resolve(input: &str) -> DomainResult<&'static Descriptor> {
    let key = input.trim().to_ascii_lowercase();
    let key = match key.as_str() {
        "doris" => "diros",
        "postgresql" | "pg" => "postgres",
        "sqlite3" => "sqlite",
        "mssql" => "sqlserver",
        other => other,
    };
    CATALOG
        .iter()
        .find(|item| item.key == key)
        .ok_or_else(|| DomainError::UnknownDataSource(key.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_match_go_behavior() {
        assert_eq!(resolve("PostgreSQL").unwrap().key, "postgres");
        assert_eq!(resolve(" doris ").unwrap().key, "diros");
        assert_eq!(resolve("MSSQL").unwrap().key, "sqlserver");
        assert!(resolve("jvm").is_err());
    }

    #[test]
    fn every_sql_source_has_a_default_query() {
        assert_eq!(resolve("mysql").unwrap().default_query(), "SELECT 1;");
        assert_eq!(resolve("redis").unwrap().default_query(), "PING");
        assert_eq!(resolve("chroma").unwrap().default_query(), "");
    }
}
