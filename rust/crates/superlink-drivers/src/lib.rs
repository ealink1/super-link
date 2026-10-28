//! Out-of-process database drivers.
//!
//! The Go agents already speak a JSON-lines protocol; this crate verifies what
//! is on disk, launches the right binary and maps frames into domain types.

pub mod native;
pub mod registry;
pub mod session;

pub use native::{
    NativeClickHouse, NativeElasticsearch, NativeMysql, NativePostgres, NativeSqlite,
};
pub use registry::{BundleRecord, InstalledDriver, RegistryError, scan};
pub use session::{ConnectInfo, DriverError, DriverSession};
