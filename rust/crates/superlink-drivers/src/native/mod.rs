//! 进程内原生驱动：逐步替换 Go agent 的四个方法子集。

pub mod postgres;
pub mod sqlite;

pub use postgres::NativePostgres;
pub use sqlite::NativeSqlite;
