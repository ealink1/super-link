//! Streaming database structure and data dump.
//!
//! The exporter walks the tables of one database scope, writes every table
//! definition, then streams the rows of each table through the INSERT SQL
//! encoder. Rows never accumulate in memory and the destination file is only
//! published once the whole dump has been synced.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use superlink_domain::{Execution, Object, Profile, TableInfo};

use crate::atomic::AtomicTemp;
use crate::cancel::{Cancellation, check};
use crate::dialect::{normalize, object_name, quote_identifier};
use crate::error::{FileError, Result};
use crate::export::budget::QuotaWriter;
use crate::export::{Encoder, ExportOptions, Format};
use crate::limits::{MAX_DATABASE_OBJECTS, MAX_DUMP_BYTES};
use crate::value::Cell;

/// Receives result columns and rows pushed by a database driver.
pub trait RowConsumer {
    /// Declares the columns of the result set.
    fn set_columns(&mut self, columns: &[String]) -> Result<()>;
    /// Consumes one row; values follow the declared column order.
    fn consume_row_values(&mut self, row: &[Cell]) -> Result<()>;
}

/// Database access required by the exporter.
pub trait ExportSource {
    /// Lists the objects of one scope.
    fn objects(
        &self,
        cancel: &dyn Cancellation,
        profile_id: &str,
        scope: &str,
    ) -> Result<Vec<Object>>;
    /// Returns the metadata of one table, addressed by its lookup name.
    fn table_info(
        &self,
        cancel: &dyn Cancellation,
        profile_id: &str,
        scope: &str,
        lookup: &str,
    ) -> Result<TableInfo>;
    /// Streams one query into the consumer.
    fn stream_query(
        &self,
        cancel: &dyn Cancellation,
        profile_id: &str,
        execution: &Execution,
        consumer: &mut dyn RowConsumer,
    ) -> Result<()>;
}

/// Snapshot reported while a dump is running.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Progress {
    /// Human readable stage, for example the table being exported.
    pub phase: String,
    /// Table currently in progress.
    pub table: String,
    /// Number of exportable tables.
    pub total: usize,
    /// Tables whose data or structure has been completed.
    pub completed_tables: usize,
    /// Total work units; twice the table count when data is included.
    pub total_steps: usize,
    /// Completed work units.
    pub completed_steps: usize,
    /// Rows written across every table.
    pub rows: i64,
    /// Rows written for the current table.
    pub table_rows: i64,
    /// Bytes written to the destination so far.
    pub bytes: u64,
    /// Set once the destination file has been published.
    pub done: bool,
}

impl Progress {
    /// Completed fraction; 100% is reserved for a published file.
    pub fn fraction(&self) -> f64 {
        if self.done {
            return 1.0;
        }
        if self.total_steps == 0 {
            return 0.0;
        }
        (self.completed_steps as f64 / self.total_steps as f64).min(0.99)
    }
}

/// What one dump produced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportSummary {
    /// Tables whose structure was written.
    pub tables: usize,
    /// Rows written across every table.
    pub rows: i64,
}

/// Writes structure and data of one scope.
pub fn save(
    cancel: &dyn Cancellation,
    source: &dyn ExportSource,
    profile: &Profile,
    scope: &str,
    path: impl AsRef<Path>,
) -> Result<ExportSummary> {
    save_with_options(cancel, source, profile, scope, path, true)
}

/// Writes structure, and optionally the data, of one scope.
pub fn save_with_options(
    cancel: &dyn Cancellation,
    source: &dyn ExportSource,
    profile: &Profile,
    scope: &str,
    path: impl AsRef<Path>,
    include_data: bool,
) -> Result<ExportSummary> {
    save_with_progress(cancel, source, profile, scope, path, include_data, None)
}

/// Everything one dump needs from its caller.
struct DumpJob<'a> {
    cancel: &'a dyn Cancellation,
    source: &'a dyn ExportSource,
    profile: &'a Profile,
    scope: &'a str,
}

impl DumpJob<'_> {
    fn dialect(&self) -> &str {
        self.profile.sql_dialect()
    }

    fn lookup(&self, object: &Object, name: &str) -> String {
        if object.schema.is_empty() {
            object.name.clone()
        } else {
            name.to_string()
        }
    }
}

/// Writes a dump and reports progress to the given callback.
pub fn save_with_progress(
    cancel: &dyn Cancellation,
    source: &dyn ExportSource,
    profile: &Profile,
    scope: &str,
    path: impl AsRef<Path>,
    include_data: bool,
    notify: Option<&mut dyn FnMut(&Progress)>,
) -> Result<ExportSummary> {
    let mut silent = |_: &Progress| {};
    let notify: &mut dyn FnMut(&Progress) = match notify {
        Some(callback) => callback,
        None => &mut silent,
    };
    let path = path.as_ref();
    let mut state = Progress {
        phase: "读取表列表".to_string(),
        ..Progress::default()
    };
    state_report(&state, notify);

    if scope.trim().is_empty() || path.as_os_str().is_empty() {
        return Err(FileError::invalid("请选择数据库与输出文件"));
    }
    let job = DumpJob {
        cancel,
        source,
        profile,
        scope,
    };
    let objects = source.objects(cancel, &profile.id, scope)?;
    if objects.len() > MAX_DATABASE_OBJECTS {
        return Err(FileError::invalid("数据库对象超过 10000 个"));
    }
    state.total = objects
        .iter()
        .filter(|object| object.kind == "table")
        .count();
    state.total_steps = state.total;
    if include_data {
        state.total_steps *= 2;
    }

    let directory = crate::atomic::parent_directory(path)?;
    let mut temp = AtomicTemp::create(&directory, ".superlink-database-")?;
    let summary = {
        let mut output = QuotaWriter::new(temp.file_mut()?, MAX_DUMP_BYTES, "数据库导出超过 8 GiB");
        write_dump_body(
            &job,
            &mut output,
            &objects,
            include_data,
            &mut state,
            notify,
        )?
    };
    check(cancel)?;
    state.phase = "写入文件并校验".to_string();
    state.table.clear();
    state_report(&state, notify);
    temp.commit(path, false)?;
    state.phase = "完成".to_string();
    state.done = true;
    state_report(&state, notify);
    Ok(summary)
}

fn write_dump_body(
    job: &DumpJob<'_>,
    output: &mut QuotaWriter<&mut std::fs::File>,
    objects: &[Object],
    include_data: bool,
    state: &mut Progress,
    notify: &mut dyn FnMut(&Progress),
) -> Result<ExportSummary> {
    let dialect = job.dialect();
    let mysql = matches!(normalize(dialect).as_str(), "mysql" | "mariadb");
    let mut summary = ExportSummary::default();
    if mysql {
        let database = quote_identifier(dialect, job.scope)?;
        write_dump(
            output,
            format!(
                "USE {database};\nSET @SUPERLINK_OLD_FK=@@FOREIGN_KEY_CHECKS;\nSET FOREIGN_KEY_CHECKS=0;\n"
            ),
        )?;
    }

    let mut tables: Vec<Object> = Vec::new();
    for object in objects {
        if object.kind != "table" {
            continue;
        }
        let mut object = object.clone();
        if object.scope.is_empty() {
            object.scope = job.scope.to_string();
        }
        if object.scope != job.scope {
            return Err(FileError::invalid("数据库对象范围不匹配"));
        }
        check(job.cancel)?;
        let name = object_name(dialect, &object)?;
        let lookup = job.lookup(&object, &name);
        state.phase = "导出表结构".to_string();
        state.table = object.name.clone();
        state.table_rows = 0;
        state_report(state, notify);
        let info = job
            .source
            .table_info(job.cancel, &job.profile.id, job.scope, &lookup)?;
        let ddl = info.ddl.trim();
        if ddl.is_empty() {
            return Err(FileError::invalid(format!(
                "表 {} 缺少建表结构",
                object.name
            )));
        }
        write_dump(output, format!("{};\n\n", ddl.trim_end_matches(';')))?;
        tables.push(object);
        summary.tables += 1;
        state.completed_steps += 1;
        if !include_data {
            state.completed_tables += 1;
        }
        state.bytes = output.written();
        state_report(state, notify);
    }

    if include_data {
        for object in &tables {
            summary.rows += export_table_data(job, object, output, state, notify)?;
        }
    }

    if mysql {
        write_dump(
            output,
            "SET FOREIGN_KEY_CHECKS=@SUPERLINK_OLD_FK;\n".to_string(),
        )?;
    }
    state.bytes = output.written();
    Ok(summary)
}

fn export_table_data(
    job: &DumpJob<'_>,
    object: &Object,
    output: &mut dyn Write,
    state: &mut Progress,
    notify: &mut dyn FnMut(&Progress),
) -> Result<i64> {
    state.phase = "导出表数据".to_string();
    state.table = object.name.clone();
    state.table_rows = 0;
    state_report(state, notify);
    let dialect = job.dialect();
    let name = object_name(dialect, object)?;
    let lookup = job.lookup(object, &name);
    let info = job
        .source
        .table_info(job.cancel, &job.profile.id, job.scope, &lookup)?;
    let mut columns = Vec::new();
    for column in &info.columns {
        if !column.writable() {
            continue;
        }
        columns.push(quote_identifier(dialect, &column.name)?);
    }
    if columns.is_empty() {
        return Err(FileError::invalid(format!(
            "表 {} 无可导出的写入列",
            object.name
        )));
    }
    let mut options = ExportOptions::new(Format::InsertSql);
    options.dialect = dialect.to_string();
    options.table = name.clone();
    let mut encoder = Encoder::new(output, job.cancel, options)?;
    let execution = Execution {
        revision: job.profile.revision,
        scope: job.scope.to_string(),
        text: format!("SELECT {} FROM {name}", columns.join(", ")),
        ..Execution::default()
    };
    {
        let mut consumer = ProgressConsumer {
            row_consumer: &mut encoder,
            state: &mut *state,
            notify: &mut *notify,
            last: Instant::now(),
        };
        job.source
            .stream_query(job.cancel, &job.profile.id, &execution, &mut consumer)?;
    }
    encoder.finish()?;
    encoder.close();
    state.completed_steps += 1;
    state.completed_tables += 1;
    state_report(state, notify);
    Ok(encoder.count() as i64)
}

struct ProgressConsumer<'a> {
    row_consumer: &'a mut dyn RowConsumer,
    state: &'a mut Progress,
    notify: &'a mut dyn FnMut(&Progress),
    last: Instant,
}

impl RowConsumer for ProgressConsumer<'_> {
    fn set_columns(&mut self, columns: &[String]) -> Result<()> {
        self.row_consumer.set_columns(columns)
    }

    fn consume_row_values(&mut self, row: &[Cell]) -> Result<()> {
        self.row_consumer.consume_row_values(row)?;
        self.state.table_rows += 1;
        self.state.rows += 1;
        if self.last.elapsed() >= Duration::from_millis(100) {
            self.last = Instant::now();
            let state = &*self.state;
            let notify = &mut *self.notify;
            notify(state);
        }
        Ok(())
    }
}

impl RowConsumer for Encoder<'_> {
    fn set_columns(&mut self, columns: &[String]) -> Result<()> {
        Encoder::set_columns(self, columns)
    }

    fn consume_row_values(&mut self, row: &[Cell]) -> Result<()> {
        Encoder::consume_row_values(self, row)
    }
}

fn state_report(state: &Progress, notify: &mut dyn FnMut(&Progress)) {
    notify(state);
}

fn write_dump(output: &mut dyn Write, text: String) -> Result<()> {
    output
        .write_all(text.as_bytes())
        .map_err(|error| FileError::io("write database export", error))
}
