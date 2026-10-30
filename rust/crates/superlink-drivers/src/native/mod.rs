//! 进程内原生驱动：逐步替换 Go agent 的四个方法子集。

pub mod clickhouse;
pub mod elasticsearch;
pub mod mysql;
pub mod postgres;
pub mod sqlite;
pub mod structure;

use std::sync::Once;

static CRYPTO_PROVIDER: Once = Once::new();

/// rustls 依赖树里同时存在 ring 与 aws-lc-rs 时不会自动选择 provider，
/// 原生驱动统一使用 ring，避免 TLS 握手前 panic。
pub fn install_crypto_provider() {
    CRYPTO_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub use clickhouse::NativeClickHouse;
pub use elasticsearch::NativeElasticsearch;
pub use mysql::NativeMysql;
pub use postgres::NativePostgres;
pub use sqlite::NativeSqlite;
pub use structure::{ColumnEntry, ForeignKeyEntry, IndexEntry, TableEntry, TableStructure};
