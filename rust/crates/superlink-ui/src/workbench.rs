//! 表工作台：数据页的条件/排序/分页请求构建与 DDL 生成。
//!
//! SQL 组装完全复用 superlink-sql 的校验器；这里只持有状态并把
//! 结构条目映射成建表模型。

use gpui::SharedString;
use superlink_domain::ResultSet;
use superlink_drivers::ColumnEntry;
use superlink_sql::create_table::build_create_table;
use superlink_sql::error::SqlError;
use superlink_sql::model::{ColumnDefinition, Filter, Object, Sort, TableInfo, TableRequest};
use superlink_sql::page::build_page_query;

pub const PAGE_SIZE: u32 = 50;
pub const OPERATORS: [&str; 7] = ["=", "!=", ">", "<", ">=", "<=", "LIKE"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkbenchTab {
    #[default]
    Data,
    Structure,
    Ddl,
}

impl WorkbenchTab {
    pub fn label(self) -> &'static str {
        match self {
            WorkbenchTab::Data => "数据",
            WorkbenchTab::Structure => "结构",
            WorkbenchTab::Ddl => "DDL",
        }
    }

    pub const ALL: [WorkbenchTab; 3] = [
        WorkbenchTab::Data,
        WorkbenchTab::Structure,
        WorkbenchTab::Ddl,
    ];
}

/// 条件构建的草稿（编辑框内容 + 当前选中的操作符）。
#[derive(Debug, Clone, Default)]
pub struct FilterDraft {
    pub column: String,
    pub value: String,
    pub operator: usize,
    pub join_and: bool,
}

#[derive(Debug, Clone)]
pub struct Workbench {
    pub conn: usize,
    pub database: String,
    pub table: String,
    pub tab: WorkbenchTab,
    pub columns: Vec<ColumnEntry>,
    pub filters: Vec<Filter>,
    pub sorts: Vec<Sort>,
    pub page: u32,
    pub result: Option<ResultSet>,
    pub total: Option<i64>,
    pub error: Option<SharedString>,
    pub loading: bool,
    pub draft: FilterDraft,
}

impl Workbench {
    pub fn new(conn: usize, database: String, table: String, columns: Vec<ColumnEntry>) -> Self {
        Self {
            conn,
            database,
            table,
            tab: WorkbenchTab::default(),
            columns,
            filters: Vec::new(),
            sorts: Vec::new(),
            page: 1,
            result: None,
            total: None,
            error: None,
            loading: false,
            draft: FilterDraft {
                operator: 0,
                join_and: true,
                ..Default::default()
            },
        }
    }

    pub fn object(&self) -> Object {
        let (schema, name) = match self.table.split_once('.') {
            Some((schema, name)) => (schema.to_string(), name.to_string()),
            None => (String::new(), self.table.clone()),
        };
        Object {
            name,
            kind: "table".into(),
            schema,
            ..Default::default()
        }
    }

    pub fn info(&self) -> TableInfo {
        TableInfo {
            columns: self
                .columns
                .iter()
                .map(|column| ColumnDefinition {
                    name: column.name.clone(),
                    data_type: column.data_type.clone(),
                    nullable: if column.nullable { "YES" } else { "NO" }.to_string(),
                    key: if column.primary_key { "PRI" } else { "" }.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// 数据页与计数两条 SQL。
    pub fn page_statements(&self, kind: &str) -> Result<(String, String), SqlError> {
        let request = TableRequest {
            object: self.object(),
            revision: 0,
            page: self.page as i64,
            size: PAGE_SIZE as i64,
            filters: self.filters.clone(),
            sorts: self.sorts.clone(),
            condition: String::new(),
        };
        build_page_query(kind, &request, &self.info())
    }

    pub fn ddl(&self, kind: &str) -> Result<String, SqlError> {
        let columns: Vec<ColumnDefinition> = self
            .columns
            .iter()
            .map(|column| ColumnDefinition {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                nullable: if column.nullable { "YES" } else { "NO" }.to_string(),
                key: if column.primary_key { "PRI" } else { "" }.to_string(),
                ..Default::default()
            })
            .collect();
        build_create_table(kind, &self.object(), &columns)
    }

    /// 把草稿追加为一条条件；列必须在当前表里。
    pub fn commit_draft(&mut self) -> Result<(), String> {
        let column = self.draft.column.trim().to_string();
        let value = self.draft.value.clone();
        if column.is_empty() {
            return Err("请选择条件列".into());
        }
        if !self.columns.iter().any(|entry| entry.name == column) {
            return Err(format!("列不存在：{column}"));
        }
        if value.is_empty()
            && matches!(
                OPERATORS[self.draft.operator],
                "=" | "!=" | ">" | "<" | ">=" | "<=" | "LIKE"
            )
        {
            return Err("请填写条件值".into());
        }
        self.filters.push(Filter {
            column,
            operator: OPERATORS[self.draft.operator].to_string(),
            value,
            join: if self.filters.is_empty() || self.draft.join_and {
                "AND".into()
            } else {
                "OR".into()
            },
        });
        self.draft.column.clear();
        self.draft.value.clear();
        self.page = 1;
        Ok(())
    }

    /// 点击数据网格表头：无排序 → 升序 → 降序 → 取消。
    pub fn toggle_sort(&mut self, column: &str) {
        if let Some(existing) = self.sorts.iter_mut().find(|sort| sort.column == column) {
            if !existing.descending {
                existing.descending = true;
            } else {
                self.sorts.retain(|sort| sort.column != column);
            }
            self.page = 1;
            return;
        }
        if self.sorts.len() >= 3 {
            self.sorts.remove(0);
        }
        self.sorts.push(Sort {
            column: column.to_string(),
            descending: false,
        });
        self.page = 1;
    }

    pub fn sort_mark(&self, column: &str) -> &'static str {
        match self.sorts.iter().find(|sort| sort.column == column) {
            Some(sort) if sort.descending => " ↓",
            Some(_) => " ↑",
            None => "",
        }
    }

    pub fn go_prev(&mut self) {
        if self.page > 1 {
            self.page -= 1;
        }
    }

    pub fn go_next(&mut self) {
        let full = self
            .result
            .as_ref()
            .map(|value| value.rows.len() as u32 >= PAGE_SIZE)
            .unwrap_or(false);
        if full {
            self.page += 1;
        }
    }

    /// 写入数据页结果与总数（来自 COUNT 查询的第一行第一列）。
    pub fn set_data(&mut self, result: ResultSet, count: Option<&ResultSet>) {
        self.total = count.and_then(|count| {
            count
                .rows
                .first()
                .and_then(|row| row.first())
                .and_then(|value| value.as_i64())
        });
        self.result = Some(result);
    }

    pub fn has_result(&self) -> bool {
        self.result.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workbench() -> Workbench {
        Workbench::new(
            0,
            String::new(),
            "items".into(),
            vec![
                ColumnEntry {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                    comment: None,
                },
                ColumnEntry {
                    name: "title".into(),
                    data_type: "TEXT".into(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    comment: None,
                },
            ],
        )
    }

    #[test]
    fn builds_page_statements_with_filters_and_sorts() {
        let mut bench = workbench();
        bench.draft.column = "title".into();
        bench.draft.value = "甲".into();
        bench.commit_draft().unwrap();
        bench.toggle_sort("id");

        let (data, count) = bench.page_statements("sqlite").unwrap();
        assert!(data.contains("WHERE"), "{data}");
        assert!(data.contains("ORDER BY"), "{data}");
        assert!(data.contains("LIMIT 50"), "{data}");
        assert!(count.contains("COUNT(*)"), "{count}");
    }

    #[test]
    fn rejects_unknown_filter_column() {
        let mut bench = workbench();
        bench.draft.column = "nope".into();
        bench.draft.value = "1".into();
        assert!(bench.commit_draft().is_err());
    }

    #[test]
    fn cycles_sort_directions() {
        let mut bench = workbench();
        bench.toggle_sort("id");
        assert_eq!(bench.sort_mark("id"), " ↑");
        bench.toggle_sort("id");
        assert_eq!(bench.sort_mark("id"), " ↓");
        bench.toggle_sort("id");
        assert_eq!(bench.sort_mark("id"), "");
    }

    #[test]
    fn generates_ddl_from_structure() {
        let bench = workbench();
        let ddl = bench.ddl("sqlite").unwrap();
        assert!(ddl.contains("CREATE TABLE"), "{ddl}");
        assert!(ddl.contains("PRIMARY KEY"), "{ddl}");
    }

    #[test]
    fn pagination_moves_within_bounds() {
        let mut bench = workbench();
        bench.go_prev();
        assert_eq!(bench.page, 1);
        bench.page = 3;
        bench.go_prev();
        assert_eq!(bench.page, 2);
    }
}
