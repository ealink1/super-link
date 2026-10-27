//! Pure domain model: no UI toolkit, no database, no I/O.
//!
//! Ported from the Go implementation in `internal/domain`. JSON field names stay
//! identical so existing workspaces keep loading.

pub mod catalog;
pub mod error;
pub mod note;
pub mod profile;
pub mod query;
pub mod shell;
pub mod structure;
pub mod table;

pub use catalog::{Descriptor, Family, catalog, resolve};
pub use error::{DomainError, DomainResult};
pub use note::{Note, NoteGroup, Notebook};
pub use profile::{ConnectionConfig, Profile};
pub use query::{
    Column, Draft, Execution, History, Parameter, ResultSet, RowChange, SavedQuery, Statement,
    TableChanges,
};
pub use shell::ShellHost;
pub use structure::{
    IndexColumn, NewIndex, StructureChange, StructureRequest, StructureResult, structure_hash,
};
pub use table::{
    ColumnDefinition, Filter, ForeignKeyDefinition, IndexDefinition, Object, Sort, TableInfo,
    TablePage, TableRequest, TriggerDefinition,
};
