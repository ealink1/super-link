//! Dialect-aware SQL construction and completion, ported from the Go
//! `internal/sqlworkbench` package.
//!
//! Everything here is pure: callers pass metadata snapshots in and receive SQL
//! text, bound arguments or completion candidates. No function performs I/O and
//! no user value is interpolated into SQL unless it is a validated identifier or
//! an escaped literal.
//!
//! Input structs mirror the Go domain shapes field for field (`nullable` is
//! `YES`/`NO`, `key` is `PRI`/`UNI`/`MUL`, a default keeps its presence
//! flag, and charset/collation travel with the column) so the builders keep the
//! same validation boundaries. Adapters map their own snapshots into them.

#![forbid(unsafe_code)]

mod base64;
pub mod completion;
pub mod completion_context;
pub mod create_table;
pub mod database;
pub mod error;
pub mod identifier;
pub mod lexer;
pub mod model;
pub mod mutations;
pub mod page;
pub mod preview;
pub mod structure;
mod structure_column;
mod structure_index;
pub mod table_catalog;
pub mod table_mutation;
pub mod value;

pub use completion::{CompletionCandidate, CompletionTable, complete};
pub use completion_context::{CompletionQuery, CompletionReference, analyze_completion};
pub use create_table::{build_create_table, can_create_table};
pub use database::{
    can_create_database, create_database_sql, create_database_with_options_sql,
    database_charset_supported,
};
pub use error::{SqlError, SqlResult};
pub use identifier::{object_name, parse_path, quote_identifier, structure_dialect, text_literal};
pub use lexer::{Token, TokenKind, tokenize};
pub use model::{
    ArgValue, ColumnDefinition, Filter, IndexColumn, IndexDefinition, NewIndex, Object, RowChange,
    Sort, Statement, StructureChange, StructureRequest, TableChanges, TableInfo, TableRequest,
};
pub use mutations::{
    build_changes, build_row, build_row_delete, build_row_insert, build_row_update, placeholder,
    preview_changes,
};
pub use page::{build_export_query, build_page_query};
pub use preview::preview_read;
pub use structure::{build_structure_changes, preview_structure, qualified_index};
pub use table_catalog::table_catalog_query;
pub use table_mutation::{can_mutate_table, table_mutation_sql};
pub use value::convert_value;
