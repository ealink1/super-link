//! 表工作台：数据页的条件/排序/分页请求构建与 DDL 生成。
//!
//! SQL 组装完全复用 superlink-sql 的校验器；这里只持有状态并把
//! 结构条目映射成建表模型。

mod preview;
pub use preview::json_preview;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TableSection {
    #[default]
    Columns,
    Indexes,
    ForeignKeys,
    Triggers,
}
impl TableSection {
    pub const ALL: [Self; 4] = [
        Self::Columns,
        Self::Indexes,
        Self::ForeignKeys,
        Self::Triggers,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Columns => "列",
            Self::Indexes => "索引",
            Self::ForeignKeys => "外键",
            Self::Triggers => "触发器",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Columns => "superlink/fields.svg",
            Self::Indexes => "superlink/index.svg",
            Self::ForeignKeys => "superlink/link.svg",
            Self::Triggers => "superlink/bolt.svg",
        }
    }
}

/// 条件构建的草稿（编辑框内容 + 当前选中的操作符）。
#[derive(Debug, Clone, Default)]
pub struct FilterDraft {
    pub column: String,
    pub value: String,
    pub operator: usize,
    pub join_and: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    /// 当前结果下标（`matches` 里的位置），0 表示第一个命中。
    pub current: usize,
}

impl FindState {
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.current = 0;
    }

    /// 在当前页结果里收集命中的 (行, 列)，按行优先顺序，大小写不敏感。
    pub fn matches(&self, result: &ResultSet) -> Vec<(usize, usize)> {
        let needle = self.query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (row, values) in result.rows.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                if crate::query_page::cell_text(value)
                    .to_lowercase()
                    .contains(&needle)
                {
                    hits.push((row, column));
                }
            }
        }
        hits
    }

    /// 是否命中给定单元格，用于绘制高亮。
    pub fn hits(&self, value: &serde_json::Value) -> bool {
        let needle = self.query.trim().to_lowercase();
        !needle.is_empty()
            && crate::query_page::cell_text(value)
                .to_lowercase()
                .contains(&needle)
    }
}

#[derive(Debug, Clone)]
pub struct Workbench {
    pub stale: bool,
    pub id: String,
    pub request: u64,
    pub edits: std::collections::BTreeMap<usize, superlink_sql::RowChange>,
    pub selected: std::collections::BTreeSet<usize>,
    pub cell: Option<(usize, usize)>,
    pub preview: Option<String>,
    pub operation: Option<superlink_app::QueryCancel>,
    pub find: FindState,
    pub conn: usize,
    pub database: String,
    pub table: String,
    pub tab: WorkbenchTab,
    pub section: TableSection,
    pub designer: Option<gpui::Entity<crate::schema_designer::SchemaDesigner>>,
    pub metadata_only: bool,
    pub triggers: Option<ResultSet>,
    pub trigger_error: Option<SharedString>,
    pub columns: Vec<ColumnEntry>,
    pub filters: Vec<Filter>,
    pub sorts: Vec<Sort>,
    pub page: u32,
    pub page_size: u32,
    pub tools_open: bool,
    pub inspected: Option<(usize, usize)>,
    pub text_view: bool,
    pub column_page: usize,
    pub column_widths: std::collections::BTreeMap<usize, f32>,
    pub horizontal: gpui::ScrollHandle,
    pub vertical: gpui::ScrollHandle,
    pub result: Option<ResultSet>,
    pub total: Option<i64>,
    pub import_plan: Option<std::sync::Arc<superlink_app::imports::ImportPlan>>,
    pub ddl_source: Option<String>,
    pub ddl_loading: bool,
    pub error: Option<SharedString>,
    pub loading: bool,
    pub draft: FilterDraft,
}

impl Workbench {
    pub fn new(conn: usize, database: String, table: String, columns: Vec<ColumnEntry>) -> Self {
        Self {
            stale: false,
            id: uuid::Uuid::new_v4().to_string(),
            request: 0,
            edits: Default::default(),
            selected: Default::default(),
            cell: None,
            preview: None,
            operation: None,
            find: FindState::default(),
            conn,
            database,
            table,
            tab: WorkbenchTab::default(),
            section: TableSection::default(),
            designer: None,
            metadata_only: false,
            triggers: None,
            trigger_error: None,
            columns,
            filters: Vec::new(),
            sorts: Vec::new(),
            page: 1,
            page_size: PAGE_SIZE,
            tools_open: false,
            inspected: None,
            text_view: false,
            column_page: 0,
            column_widths: Default::default(),
            horizontal: gpui::ScrollHandle::new(),
            vertical: gpui::ScrollHandle::new(),
            result: None,
            total: None,
            import_plan: None,
            ddl_source: None,
            ddl_loading: false,
            error: None,
            loading: false,
            draft: FilterDraft {
                operator: 0,
                join_and: true,
                ..Default::default()
            },
        }
    }

    pub fn export_statement(&self, kind: &str) -> Result<String, SqlError> {
        let request = TableRequest {
            object: self.object(),
            page: 1,
            size: self.page_size.into(),
            filters: self.filters.clone(),
            sorts: self.sorts.clone(),
            ..Default::default()
        };
        superlink_sql::build_export_query(kind, &request, &self.info())
    }
    pub fn object(&self) -> Object {
        let (schema, name) = match self.table.split_once('.') {
            Some((schema, name)) => (schema.to_string(), name.to_string()),
            None => (String::new(), self.table.clone()),
        };
        Object {
            name,
            scope: self.database.clone(),
            kind: "table".into(),
            schema,
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
                    default: column.default_value.clone(),
                    has_default: column.default_value.is_some(),
                    extra: column.extra.clone(),
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
            size: self.page_size as i64,
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
            .map(|value| value.rows.len() as u32 >= self.page_size)
            .unwrap_or(false);
        if full
            && self
                .total
                .is_none_or(|total| i64::from(self.page) * i64::from(self.page_size) < total)
        {
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
        self.edits.clear();
        self.selected.clear();
        self.cell = None;
        self.inspected = None;
        self.preview = None;
        self.vertical
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
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
                    extra: String::new(),
                },
                ColumnEntry {
                    name: "title".into(),
                    data_type: "TEXT".into(),
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    comment: None,
                    extra: String::new(),
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
    #[test]
    fn find_matches_are_row_major_and_case_insensitive() {
        let result = ResultSet {
            columns: Vec::new(),
            rows: vec![
                vec![serde_json::json!("Alice"), serde_json::json!("x")],
                vec![serde_json::json!(1), serde_json::json!("ALICE")],
            ],
            ..Default::default()
        };
        let mut find = FindState {
            query: "alice".into(),
            ..Default::default()
        };
        assert_eq!(find.matches(&result), vec![(0, 0), (1, 1)]);
        find.query = "  ".into();
        assert!(find.matches(&result).is_empty());
        assert!(!find.hits(&serde_json::json!("Miss")));
    }

    #[test]
    fn closing_find_clears_query_and_position() {
        let mut find = FindState {
            open: true,
            query: "x".into(),
            current: 3,
        };
        find.close();
        assert!(!find.open);
        assert!(find.query.is_empty());
        assert_eq!(find.current, 0);
    }

    #[test]
    fn last_full_page_does_not_advance_and_page_size_controls_sql() {
        let mut bench = workbench();
        bench.page_size = 100;
        bench.total = Some(200);
        bench.result = Some(ResultSet {
            rows: vec![vec![serde_json::json!(1)]; 100],
            ..Default::default()
        });
        bench.go_next();
        assert_eq!(bench.page, 2);
        bench.go_next();
        assert_eq!(bench.page, 2);
        let (sql, _) = bench.page_statements("sqlite").unwrap();
        assert!(sql.contains("LIMIT 100"), "{sql}");
        assert!(sql.contains("OFFSET 100"), "{sql}");
    }
}
