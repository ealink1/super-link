//! Hard budgets shared by every reader and writer.
//!
//! Values mirror the Go implementation. Changing one changes the behaviour of
//! both the importer and the exporter, so they live in a single place.

/// Largest file accepted by the import path.
pub const MAX_IMPORT_BYTES: u64 = 64 << 20;
/// Largest number of data rows accepted by the import path.
pub const MAX_IMPORT_ROWS: usize = 100_000;
/// Largest number of cells accepted by the import path.
pub const MAX_IMPORT_CELLS: usize = 2_000_000;
/// Largest number of columns accepted by the import path.
pub const MAX_IMPORT_COLUMNS: usize = 1024;
/// Longest accepted column name.
pub const MAX_IMPORT_COLUMN_NAME: usize = 512;
/// Largest single imported field.
pub const MAX_FIELD_BYTES: usize = 1 << 20;

/// Largest export produced by the encoder.
pub const MAX_EXPORT_BYTES: u64 = 8 << 30;
/// Largest number of rows accepted by the encoder.
pub const MAX_EXPORT_ROWS: u64 = 5_000_000;
/// Largest number of export columns.
pub const MAX_EXPORT_COLUMNS: usize = 16_384;

/// Largest SQL document loaded into the editor.
pub const MAX_SQL_BYTES: usize = 1 << 20;
/// Largest database dump produced by the exporter.
pub const MAX_DUMP_BYTES: u64 = 8 << 30;
/// Largest number of database objects inspected by the exporter.
pub const MAX_DATABASE_OBJECTS: usize = 10_000;

/// Excel's own worksheet row limit.
pub const MAX_XLSX_ROWS: u64 = 1_048_576;
/// Excel's own cell text limit, counted in characters.
pub const MAX_XLSX_CELL_CHARS: usize = 32_767;
