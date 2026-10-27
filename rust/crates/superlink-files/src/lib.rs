//! Bounded data-file import, export, SQL documents and database dumps.
//!
//! Ported from the Go packages internal/datafile, internal/queryfile and
//! internal/databaseexport. Every reader and writer keeps the original
//! byte/row/cell budgets, so a malformed or oversized file fails with a clear
//! error instead of growing memory without bound.
//!
//! The crate performs no UI work and never imports a toolkit.

pub mod atomic;
pub mod cancel;
pub mod database_export;
pub mod dialect;
pub mod error;
pub mod export;
pub mod import;
pub mod limits;
pub mod query_file;
pub mod sql_stream;
pub mod value;

pub use cancel::{CancelFlag, Cancellation, NeverCancelled};
pub use error::{FileError, Result};
pub use value::Cell;
