//! Out-of-process database drivers.
//!
//! Rust driver hosts speak a bounded JSON-lines protocol; this crate verifies what
//! is on disk, launches the right binary and maps frames into domain types.

pub mod budget;
pub mod native;
pub mod registry;
pub mod session;
pub mod stream;

pub use native::structure::{
    ColumnEntry, ForeignKeyEntry, IndexEntry, TableEntry, TableStats, TableStructure,
};
pub use native::{
    NativeClickHouse, NativeElasticsearch, NativeMysql, NativePostgres, NativeSqlite,
    install_crypto_provider,
};
pub use registry::{BundleRecord, InstalledDriver, RegistryError, scan};
pub use session::{ConnectInfo, DriverError, DriverSession};
pub use superlink_infra::QueryCancel;

pub mod arguments;
