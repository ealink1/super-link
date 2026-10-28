//! 进程内原生驱动：逐步替换 Go agent 的四个方法子集。

pub mod clickhouse;
pub mod mysql;
pub mod postgres;
pub mod sqlite;

pub use clickhouse::NativeClickHouse;
pub use mysql::NativeMysql;
pub use postgres::NativePostgres;
pub use sqlite::NativeSqlite;
