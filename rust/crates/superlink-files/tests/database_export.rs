//! Database dump behaviour: structure, streamed data, progress and safety.

use std::cell::RefCell;
use std::fs;

use superlink_domain::{ColumnDefinition, ConnectionConfig, Execution, Object, Profile, TableInfo};
use superlink_files::database_export::{self, ExportSource, Progress, RowConsumer};
use superlink_files::{CancelFlag, Cell, FileError, NeverCancelled};

struct Fixture {
    fail: bool,
    request: RefCell<Execution>,
}

impl Fixture {
    fn new(fail: bool) -> Self {
        Self {
            fail,
            request: RefCell::new(Execution::default()),
        }
    }
}

impl ExportSource for Fixture {
    fn objects(
        &self,
        _cancel: &dyn superlink_files::Cancellation,
        _profile: &str,
        _scope: &str,
    ) -> superlink_files::Result<Vec<Object>> {
        Ok(vec![
            Object {
                name: "items".to_string(),
                kind: "table".to_string(),
                ..Object::default()
            },
            Object {
                name: "summary".to_string(),
                kind: "view".to_string(),
                ..Object::default()
            },
        ])
    }

    fn table_info(
        &self,
        _cancel: &dyn superlink_files::Cancellation,
        _profile: &str,
        _scope: &str,
        _lookup: &str,
    ) -> superlink_files::Result<TableInfo> {
        let backtick = char::from(96);
        Ok(TableInfo {
            ddl: format!("CREATE TABLE {backtick}items{backtick} ({backtick}name{backtick} TEXT)"),
            columns: vec![ColumnDefinition {
                name: "name".to_string(),
                ..ColumnDefinition::default()
            }],
            ..TableInfo::default()
        })
    }

    fn stream_query(
        &self,
        _cancel: &dyn superlink_files::Cancellation,
        _profile: &str,
        execution: &Execution,
        consumer: &mut dyn RowConsumer,
    ) -> superlink_files::Result<()> {
        *self.request.borrow_mut() = execution.clone();
        consumer.set_columns(&["name".to_string()])?;
        consumer.consume_row_values(&[Cell::Text("O'Reilly".to_string())])?;
        if self.fail {
            return Err(FileError::invalid("transport failed"));
        }
        Ok(())
    }
}

fn profile() -> Profile {
    Profile {
        id: "p".to_string(),
        revision: 3,
        config: ConnectionConfig {
            kind: "mysql".to_string(),
            ..ConnectionConfig::default()
        },
        ..Profile::default()
    }
}

#[test]
fn database_export_writes_structure_and_streams_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dump.sql");
    let source = Fixture::new(false);
    let summary =
        database_export::save(&NeverCancelled, &source, &profile(), "demo", &path).unwrap();
    assert_eq!(summary.tables, 1);
    assert_eq!(summary.rows, 1);

    let text = fs::read_to_string(&path).unwrap();
    let backtick = char::from(96);
    for expected in [
        format!("USE {backtick}demo{backtick};"),
        "CREATE TABLE".to_string(),
        format!("INSERT INTO {backtick}demo{backtick}.{backtick}items{backtick}"),
        "FOREIGN_KEY_CHECKS=@SUPERLINK_OLD_FK".to_string(),
    ] {
        assert!(text.contains(&expected), "missing {expected} in dump");
    }

    let request = source.request.borrow();
    assert_eq!(request.revision, 3);
    assert_eq!(request.scope, "demo");
    assert!(!request.write);
    assert!(request.text.contains("SELECT"), "{}", request.text);
}

#[test]
fn structure_only_never_reads_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("structure.sql");
    let source = Fixture::new(true);
    let summary = database_export::save_with_options(
        &NeverCancelled,
        &source,
        &profile(),
        "demo",
        &path,
        false,
    )
    .unwrap();
    assert_eq!(summary.tables, 1);
    assert_eq!(summary.rows, 0);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("CREATE TABLE"));
    assert!(!text.contains("INSERT INTO"));
    assert!(source.request.borrow().text.is_empty());
}

#[test]
fn failed_export_preserves_an_existing_destination() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dump.sql");
    fs::write(&path, "original").unwrap();
    for fail in [true, false] {
        let result = database_export::save(
            &NeverCancelled,
            &Fixture::new(fail),
            &profile(),
            "demo",
            &path,
        );
        assert!(result.is_err(), "existing destination was replaced");
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn progress_reports_stages_rows_and_published_completion() {
    for include_data in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dump.sql");
        let source = Fixture::new(false);
        let mut updates: Vec<Progress> = Vec::new();
        database_export::save_with_progress(
            &NeverCancelled,
            &source,
            &profile(),
            "demo",
            &path,
            include_data,
            Some(&mut |progress: &Progress| updates.push(progress.clone())),
        )
        .unwrap();
        let mut previous = 0.0f64;
        let mut saw_table = false;
        for progress in &updates {
            assert!(progress.fraction() >= previous, "progress regressed");
            previous = progress.fraction();
            saw_table |= progress.table == "items";
            assert!(progress.done || progress.fraction() < 1.0);
        }
        let last = updates.last().unwrap();
        assert!(saw_table, "progress never named the table");
        assert!(last.done);
        assert_eq!(last.completed_tables, 1);
        assert_eq!(last.total, 1);
        assert!(last.bytes > 0);
        assert_eq!(last.fraction(), 1.0);
        assert_eq!(last.rows, if include_data { 1 } else { 0 });
    }
}

#[test]
fn cancelled_export_stops_before_publishing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancelled.sql");
    let flag = CancelFlag::new();
    flag.cancel();
    let error =
        database_export::save(&flag, &Fixture::new(false), &profile(), "demo", &path).unwrap_err();
    assert!(matches!(error, FileError::Cancelled));
    assert!(!path.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
