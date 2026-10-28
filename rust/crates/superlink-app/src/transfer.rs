//! 文件与任务：结果导出、CSV 导入与环境诊断。

use std::path::{Path, PathBuf};

use serde_json::Value;
use superlink_domain::ResultSet;
use superlink_drivers::registry;
use superlink_files::cancel::NeverCancelled;
use superlink_files::export::{Encoder, ExportOptions, Format};
use superlink_files::import::{ReadOptions, read_file};
use superlink_files::value::Cell;

const IMPORT_MAX_ROWS: usize = 5_000;
const INSERT_CHUNK_ROWS: usize = 100;

/// 与主程序一致的数据根目录解析。
pub fn data_root() -> PathBuf {
    if let Some(root) = std::env::var_os("SUPERLINK_DATA_ROOT") {
        return PathBuf::from(root);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/SuperLink")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("SuperLink")
    } else {
        home.join(".local/share/SuperLink")
    }
}

/// 驱动 agent 目录（与主程序一致）。
pub fn drivers_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SUPERLINK_DRIVERS_DIR") {
        return PathBuf::from(dir);
    }
    data_root().join("drivers")
}

/// 把结果集导出为 CSV，返回写入的行数。
pub fn export_result_csv(
    result: &ResultSet,
    path: &Path,
) -> Result<u64, superlink_files::error::FileError> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::File::create(path)
        .map_err(|error| superlink_files::error::FileError::io("create export", error))?;
    let cancel = NeverCancelled;
    let mut encoder = Encoder::new(file, &cancel, ExportOptions::new(Format::Csv))?;
    let names: Vec<String> = result
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    encoder.set_columns(&names)?;
    for row in &result.rows {
        let cells: Vec<Cell> = row.iter().map(json_to_cell).collect();
        encoder.consume_row_values(&cells)?;
    }
    encoder.finish()?;
    Ok(result.rows.len() as u64)
}

fn json_to_cell(value: &Value) -> Cell {
    match value {
        Value::Null => Cell::Null,
        Value::Bool(flag) => Cell::Bool(*flag),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                Cell::Int(int)
            } else if let Some(uint) = number.as_u64() {
                Cell::UInt(uint)
            } else {
                Cell::Float(number.as_f64().unwrap_or_default())
            }
        }
        Value::String(text) => Cell::Text(text.clone()),
        other => Cell::Text(other.to_string()),
    }
}

/// 导入 CSV 到目标表：按目标表的列对齐数据，生成多行 INSERT 语句。
/// 返回（SQL 语句, 行数）；调用方负责执行。
pub fn import_csv_to_table(
    path: &Path,
    kind: &str,
    table: &str,
    columns: &[superlink_drivers::ColumnEntry],
) -> Result<(String, usize), String> {
    let cancel = NeverCancelled;
    let dataset = read_file(&cancel, path, &ReadOptions::default())
        .map_err(|err| format!("读取导入文件失败：{err}"))?;
    if dataset.rows.is_empty() {
        return Err("导入文件没有数据行".into());
    }
    if dataset.rows.len() > IMPORT_MAX_ROWS {
        return Err(format!(
            "单次导入最多 {} 行，文件包含 {} 行",
            IMPORT_MAX_ROWS,
            dataset.rows.len()
        ));
    }
    // 目标表的列名集合（不区分大小写匹配）
    let known: Vec<(String, String)> = columns
        .iter()
        .map(|column| (column.name.to_ascii_lowercase(), column.name.clone()))
        .collect();
    // 文件表头 → 目标列名（按名字对齐；找不到的列跳过）
    let mut mapping: Vec<Option<String>> = Vec::new();
    for header in &dataset.columns {
        let lower = header.to_ascii_lowercase();
        let matched = known
            .iter()
            .find(|(lower_name, _)| *lower_name == lower)
            .map(|(_, real)| real.clone());
        mapping.push(matched);
    }
    let used: Vec<&String> = mapping.iter().flatten().collect();
    if used.is_empty() {
        return Err("导入文件的表头与目标表的列没有交集".into());
    }
    let quoted_table = quote_ident(kind, table);
    let quoted_columns: Vec<String> = used.iter().map(|name| quote_ident(kind, name)).collect();
    let prefix = format!(
        "INSERT INTO {quoted_table} ({}) VALUES ",
        quoted_columns.join(", ")
    );
    let mut chunks: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut total = 0usize;
    for row in &dataset.rows {
        let mut values: Vec<String> = Vec::new();
        for (position, cell) in row.iter().enumerate() {
            if matches!(mapping.get(position), Some(Some(_))) {
                values.push(cell_to_sql(cell));
            }
        }
        current.push(format!("({})", values.join(", ")));
        total += 1;
        if current.len() >= INSERT_CHUNK_ROWS {
            chunks.push(format!("{}{};", prefix, current.join(", ")));
            current.clear();
        }
    }
    if !current.is_empty() {
        chunks.push(format!("{}{};", prefix, current.join(", ")));
    }
    Ok((chunks.join("\n"), total))
}

fn quote_ident(kind: &str, name: &str) -> String {
    let bt: char = char::from_u32(96).unwrap();
    let dq: char = char::from_u32(34).unwrap();
    match kind {
        "mysql" => {
            let inner = name.replace(bt, &bt.to_string().repeat(2));
            format!("{bt}{inner}{bt}", bt = bt, inner = inner)
        }
        _ => {
            let inner = name.replace(dq, &dq.to_string().repeat(2));
            format!("{dq}{inner}{dq}", dq = dq, inner = inner)
        }
    }
}

fn cell_to_sql(cell: &Cell) -> String {
    match cell {
        Cell::Null => "NULL".into(),
        Cell::Bool(flag) => {
            if *flag {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Cell::Int(int) => int.to_string(),
        Cell::UInt(uint) => uint.to_string(),
        Cell::Float(float) => float.to_string(),
        Cell::Text(text) => format!("'{}'", text.replace('\'', "''")),
        other => format!("'{}'", other.text().replace('\'', "''")),
    }
}

/// 环境诊断：数据目录、agent、原生驱动开关。
pub fn diagnostics() -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = Vec::new();
    let root = data_root();
    rows.push(("数据目录".into(), root.display().to_string()));
    let drivers = drivers_dir();
    rows.push(("驱动目录".into(), drivers.display().to_string()));
    match registry::scan(&drivers) {
        Ok(installed) => {
            let names: Vec<String> = installed
                .iter()
                .filter(|driver| driver.is_usable())
                .map(|driver| driver.kind.clone())
                .collect();
            rows.push((
                "可用 agent".into(),
                if names.is_empty() {
                    "无".into()
                } else {
                    names.join(", ")
                },
            ));
        }
        Err(err) => rows.push(("驱动目录扫描".into(), err.to_string())),
    }
    for (label, key) in [
        ("原生 SQLite", "SUPERLINK_NATIVE_SQLITE"),
        ("原生 PostgreSQL", "SUPERLINK_NATIVE_PG"),
        ("原生 MySQL 族", "SUPERLINK_NATIVE_MYSQL"),
        ("原生 ClickHouse", "SUPERLINK_NATIVE_CLICKHOUSE"),
        ("原生 Elasticsearch", "SUPERLINK_NATIVE_ES"),
    ] {
        rows.push((
            label.into(),
            if std::env::var_os(key).is_some() {
                "开启".into()
            } else {
                "关闭（走 agent）".into()
            },
        ));
    }
    rows.push(("应用版本".into(), env!("CARGO_PKG_VERSION").to_string()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use superlink_domain::Column;

    #[test]
    fn exports_csv_and_reimports() {
        let result = ResultSet {
            columns: vec![
                Column {
                    id: "id".into(),
                    name: "id".into(),
                },
                Column {
                    id: "name".into(),
                    name: "name".into(),
                },
            ],
            rows: vec![
                vec![Value::from(1), Value::from("甲")],
                vec![Value::Null, Value::from("乙,丙")],
            ],
            messages: Vec::new(),
            rows_affected: 0,
            truncated: false,
            duration_ns: 0,
        };
        let path =
            std::env::temp_dir().join(format!("superlink-export-{}.csv", std::process::id()));
        let rows = export_result_csv(&result, &path).unwrap();
        assert_eq!(rows, 2);

        let target = vec![
            superlink_drivers::ColumnEntry {
                name: "id".into(),
                data_type: "INTEGER".into(),
                nullable: true,
                primary_key: false,
                default_value: None,
                comment: None,
            },
            superlink_drivers::ColumnEntry {
                name: "name".into(),
                data_type: "TEXT".into(),
                nullable: true,
                primary_key: false,
                default_value: None,
                comment: None,
            },
        ];
        let (sql, count) = import_csv_to_table(&path, "sqlite", "t", &target).unwrap();
        println!("SQL: {:?}", sql);
        assert_eq!(count, 2);
        assert!(sql.contains("INSERT INTO"), "{sql}");
        assert!(sql.contains("VALUES"), "{sql}");
        assert!(sql.contains("'乙,丙'"), "{sql}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn diagnostics_reports_environment() {
        let rows = diagnostics();
        assert!(rows.iter().any(|(label, _)| label == "应用版本"));
        assert!(rows.iter().any(|(label, _)| label == "数据目录"));
    }
}
