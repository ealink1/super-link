use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Application, Bounds, Context, Entity, FontWeight, IntoElement, MouseButton,
    SharedString, Window, WindowBounds, WindowOptions, div, point, prelude::*, px, size,
};
use superlink_app::{
    DraftService, HistoryService, HostService, NotebookService, ProfileService, QueryService,
    SavedQueryService, SettingsService, StructureService,
};
use superlink_domain::Profile;

use crate::editor::SqlEditor;
use crate::icons;
use crate::note_page::NotePage;
use crate::query_page::QueryPage;
use crate::terminal_page::TerminalPage;
use crate::theme::{Theme, ThemeMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Workspace {
    Sql,
    Shell,
    Note,
}

/// 标题栏高度，对齐 Fyne 原生标题栏 accessory 的 36。
const TITLEBAR_HEIGHT: f32 = 36.0;
/// 顶部工具栏高度，对齐 Fyne header toolbar 的 56。
const HEADER_HEIGHT: f32 = 56.0;
/// SQL / Shell / Note 分段宽度，Fyne 每段 56。
const MODE_TAB_WIDTH: f32 = 56.0;
/// 单个库最多后台预取的表结构数量，避免一次性打满连接。
const PREFETCH_LIMIT: usize = 200;
/// 预取过程中每处理若干张表刷新一次补全快照。
const PREFETCH_REFRESH_EVERY: usize = 8;

/// 工具栏图标与标题字号，Fyne header 图标 24、标题 12。
const HEADER_ICON: f32 = 22.0;
const HEADER_CAPTION: f32 = 12.0;

impl Workspace {
    fn label(self) -> &'static str {
        match self {
            Workspace::Sql => "SQL",
            Workspace::Shell => "Shell",
            Workspace::Note => "Note",
        }
    }
}

/// 表结构编辑面板的临时状态。
struct StructureEdit {
    conn: usize,
    database: String,
    table: String,
    columns: Vec<superlink_drivers::ColumnEntry>,
    indexes: Vec<superlink_drivers::IndexEntry>,
    name: Entity<SqlEditor>,
    data_type: Entity<SqlEditor>,
    index_name: Entity<SqlEditor>,
    index_columns: Entity<SqlEditor>,
    index_unique: bool,
    nullable: bool,
    /// 变更集：预览并确认后一次性提交。
    pending: Vec<superlink_sql::StructureChange>,
    preview: Option<String>,
    confirmed: bool,
    pending_delete: Option<String>,
    rename_from: Option<String>,
    rename_editor: Option<Entity<SqlEditor>>,
    error: Option<SharedString>,
    statements: Vec<String>,
}

pub struct SuperLinkApp {
    service: Arc<ProfileService>,
    query_page: Entity<QueryPage>,
    note_page: Entity<NotePage>,
    terminal_page: Entity<TerminalPage>,
    profiles: Vec<Profile>,
    selected: usize,
    collapsed_groups: HashSet<String>,
    settings: Arc<SettingsService>,
    mode: ThemeMode,
    settings_open: bool,
    sql_search: Entity<SqlEditor>,
    queries: Arc<QueryService>,
    expanded_conns: HashSet<usize>,
    conn_dbs: HashMap<usize, Option<Vec<String>>>,
    conn_db_note: HashMap<usize, SharedString>,
    expanded_dbs: HashSet<(usize, String)>,
    db_tables: HashMap<(usize, String), Option<Vec<superlink_drivers::TableEntry>>>,
    expanded_tables: HashSet<(usize, String)>,
    table_structures: HashMap<(usize, String), superlink_drivers::TableStructure>,
    column_errors: HashMap<(usize, String), SharedString>,
    structure: Arc<StructureService>,
    groups: Arc<superlink_app::GroupService>,
    groups_open: bool,
    group_new: Entity<SqlEditor>,
    group_rename: Option<(String, Entity<SqlEditor>)>,
    declared_groups: Vec<String>,
    group_note: Option<SharedString>,
    /// 正在后台预取结构的（连接, 库），避免重复排队。
    prefetching: HashSet<(usize, String)>,
    /// 侧栏：搜索框是否展开、当前对象类型过滤。
    search_open: bool,
    filter_kind: String,
    structure_edit: Option<StructureEdit>,
    workbench: Option<crate::workbench::Workbench>,
    wb_column: Entity<SqlEditor>,
    wb_value: Entity<SqlEditor>,
    wb_import: Entity<SqlEditor>,
    workspace: Workspace,
    theme: Theme,
    resolved_dark: bool,
    notice: Option<SharedString>,
    error: Option<SharedString>,
}

impl SuperLinkApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        history: Arc<HistoryService>,
        saved: Arc<SavedQueryService>,
        structure: Arc<StructureService>,
        groups: Arc<superlink_app::GroupService>,
        notebook: Arc<NotebookService>,
        hosts: Arc<HostService>,
        settings: Arc<SettingsService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = ThemeMode::from_key(&settings.theme_mode());
        let resolved_dark = window.appearance() == gpui::WindowAppearance::Dark;
        let theme = mode.resolve(resolved_dark);
        let (profiles, error) = match service.list() {
            Ok(profiles) => (profiles, None),
            Err(err) => (Vec::new(), Some(SharedString::from(err.to_string()))),
        };
        let query_page = cx.new(|cx| {
            QueryPage::new(
                service.clone(),
                queries.clone(),
                drafts,
                history,
                saved.clone(),
                settings.clone(),
                theme,
                cx,
            )
        });
        let weak_shell = cx.entity().downgrade();
        query_page.update(cx, |page, cx| {
            page.set_on_saved(
                move |id, app| {
                    // 回调发生在 QueryPage 更新期间，先排队再更新外壳，避免重入。
                    let weak_shell = weak_shell.clone();
                    let id = id.to_string();
                    app.defer(move |app| {
                        let _ = weak_shell
                            .update(app, |shell, cx| shell.select_profile_by_id(&id, cx));
                    });
                },
                cx,
            );
        });
        let sql_search = cx.new(SqlEditor::new);
        sql_search.update(cx, |editor, cx| {
            editor.set_placeholder("搜索连接…", cx);
        });
        // 过滤值直接读输入框文本，避免在回调里重入更新本实体。
        cx.observe(&sql_search, |_this, _editor, cx| cx.notify())
            .detach();
        let note_page = cx.new(|cx| NotePage::new(notebook, theme, cx));
        let terminal_page = cx.new(|cx| TerminalPage::new(hosts, theme, cx));
        let mut app = Self {
            service,
            query_page,
            note_page,
            terminal_page,
            profiles,
            selected: 0,
            collapsed_groups: HashSet::new(),
            settings,
            mode,
            settings_open: false,
            sql_search: sql_search.clone(),
            queries: queries.clone(),
            expanded_conns: HashSet::new(),
            conn_dbs: HashMap::new(),
            conn_db_note: HashMap::new(),
            expanded_dbs: HashSet::new(),
            db_tables: HashMap::new(),
            expanded_tables: HashSet::new(),
            table_structures: HashMap::new(),
            column_errors: HashMap::new(),
            structure: structure.clone(),
            groups: groups.clone(),
            groups_open: false,
            group_new: {
                let editor = cx.new(SqlEditor::new);
                editor.update(cx, |editor, cx| editor.set_placeholder("新分组名", cx));
                editor
            },
            group_rename: None,
            declared_groups: Vec::new(),
            group_note: None,
            prefetching: HashSet::new(),
            search_open: false,
            filter_kind: String::new(),
            structure_edit: None,
            workbench: None,
            wb_column: cx.new(SqlEditor::new),
            wb_value: cx.new(SqlEditor::new),
            wb_import: {
                let editor = cx.new(SqlEditor::new);
                editor.update(cx, |editor, cx| {
                    editor.set_placeholder("导入文件路径（CSV）", cx)
                });
                editor
            },
            workspace: Workspace::Sql,
            theme,
            resolved_dark,
            notice: None,
            error,
        };
        app.sync_selection(cx);
        app
    }

    /// Cycles 自动 -> 浅色 -> 深色 and persists the choice.
    fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_mode(self.mode.next(), window, cx);
    }

    fn set_mode(&mut self, mode: ThemeMode, window: &mut Window, cx: &mut Context<Self>) {
        self.resolved_dark = window.appearance() == gpui::WindowAppearance::Dark;
        let theme = mode.resolve(self.resolved_dark);
        self.mode = mode;
        self.theme = theme;
        self.query_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.terminal_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.note_page
            .update(cx, |page, cx| page.set_theme(theme, cx));
        self.settings.set_theme_mode(mode.as_key());
        cx.notify();
    }

    fn settings_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let modes = [
            (ThemeMode::System, "自动"),
            (ThemeMode::Light, "浅色"),
            (ThemeMode::Dark, "深色"),
        ];
        let limits = [1_000u32, 5_000, 20_000];

        let mut mode_row = div().flex().items_center().gap_2();
        for (mode, label) in modes {
            let active = self.mode == mode;
            mode_row =
                mode_row.child(
                    div()
                        .id(SharedString::from(format!("theme-mode-{}", mode.as_key())))
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if active { theme.accent } else { theme.canvas })
                        .border_1()
                        .border_color(if active { theme.accent } else { theme.border })
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.set_mode(mode, window, cx)
                        }))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if active { theme.surface } else { theme.text })
                                .child(label),
                        ),
                );
        }

        let current_limit = self.settings.row_limit();
        let mut limit_row = div().flex().items_center().gap_2();
        for limit in limits {
            let active = current_limit == limit;
            limit_row = limit_row.child(
                div()
                    .id(SharedString::from(format!("row-limit-{limit}")))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.settings.set_row_limit(limit);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(format!("{limit} 行")),
                    ),
            );
        }

        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_6()
                    .w(px(520.0))
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(icons::icon("superlink/settings.svg", 16.0, theme.muted))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text)
                                    .child("设置"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("close-settings")
                                    .px_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.settings_open = false;
                                        cx.notify();
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("外观"),
                            )
                            .child(mode_row),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("结果行数上限"),
                            )
                            .child(limit_row),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("诊断与版本"),
                    )
                    .children(superlink_app::transfer::diagnostics().into_iter().map(
                        |(label, value)| {
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child(SharedString::from(label)),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child(SharedString::from(value)),
                                )
                        },
                    )),
            )
    }

    /// A connection was saved in the form: refresh, select it, open its query.
    fn select_profile_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        self.refresh();
        if let Some(index) = self.profiles.iter().position(|profile| profile.id == id) {
            self.selected = index;
        }
        self.workspace = Workspace::Sql;
        self.sync_selection(cx);
        cx.notify();
    }

    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(profile) = self.profiles.get(self.selected).cloned() {
            self.query_page
                .update(cx, |page, cx| page.select_profile(profile, cx));
        }
        self.sync_catalog(cx);
    }

    /// Rebuilds the completion snapshot from the metadata the object tree has
    /// already cached for the selected connection.
    fn sync_catalog(&mut self, cx: &mut Context<Self>) {
        let index = self.selected;
        let mut tables: Vec<superlink_sql::CompletionTable> = Vec::new();
        for ((conn, database), entries) in self.db_tables.iter() {
            if *conn != index {
                continue;
            }
            let Some(entries) = entries else {
                continue;
            };
            for entry in entries {
                let columns = self
                    .table_structures
                    .get(&(index, entry.name.clone()))
                    .map(|structure| {
                        structure
                            .columns
                            .iter()
                            .map(|column| column.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                tables.push(superlink_sql::CompletionTable {
                    schema: database.clone(),
                    name: entry.name.clone(),
                    columns,
                });
            }
        }
        self.query_page
            .update(cx, |page, cx| page.set_catalog(tables, cx));
    }

    /// Fetches every uncached table structure for one database in the
    /// background so column completion works without expanding each table.
    fn prefetch_structures(&mut self, index: usize, database: String, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        if !self.prefetching.insert((index, database.clone())) {
            return;
        }
        let entries = self
            .db_tables
            .get(&(index, database.clone()))
            .and_then(|entries| entries.clone())
            .unwrap_or_default();
        let cached: HashSet<String> = self
            .table_structures
            .keys()
            .filter(|(conn, _)| *conn == index)
            .map(|(_, table)| table.clone())
            .collect();
        let pending = missing_structures(&entries, &cached, PREFETCH_LIMIT);
        if pending.is_empty() {
            self.prefetching.remove(&(index, database));
            return;
        }
        let queries = self.queries.clone();
        let profile_id = profile.id.clone();
        cx.spawn(async move |this, cx| {
            let mut done = 0usize;
            for table in pending {
                let queries = queries.clone();
                let profile = profile.clone();
                let db = database.clone();
                let table_for_task = table.clone();
                let task = cx
                    .background_executor()
                    .spawn(async move { queries.structure(&profile, &db, &table_for_task) });
                let outcome = task.await;
                done += 1;
                let refresh = done % PREFETCH_REFRESH_EVERY == 0;
                let updated = this.update(cx, |this, cx| {
                    let still_selected = this
                        .profiles
                        .get(index)
                        .map(|profile| profile.id == profile_id)
                        .unwrap_or(false);
                    if !still_selected {
                        return;
                    }
                    if let Ok(structure) = outcome {
                        this.table_structures.insert((index, table.clone()), structure);
                    }
                    if refresh {
                        this.sync_catalog(cx);
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
            this.update(cx, |this, cx| {
                this.prefetching.remove(&(index, database.clone()));
                this.sync_catalog(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 连接节点的库列表；SQLite 返回空即没有库层级，直接挂表。
    fn toggle_conn(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.expanded_conns.remove(&index) {
            cx.notify();
            return;
        }
        self.expanded_conns.insert(index);
        if !self.conn_dbs.contains_key(&index) {
            self.fetch_databases(index, cx);
        }
        cx.notify();
    }

    fn fetch_databases(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let queries = self.queries.clone();
        let task = cx
            .background_executor()
            .spawn(async move { queries.databases(&profile) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                match outcome {
                    Ok(databases) => {
                        let flat = databases.is_empty();
                        this.conn_dbs.insert(index, Some(databases));
                        this.conn_db_note.remove(&index);
                        if flat {
                            // SQLite 一类没有库层级的数据源：连接下直接取表。
                            this.fetch_tables(index, String::new(), cx);
                        }
                    }
                    Err(err) => {
                        this.conn_dbs.insert(index, Some(Vec::new()));
                        this.conn_db_note
                            .insert(index, SharedString::from(err.to_string()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_db(&mut self, index: usize, database: String, cx: &mut Context<Self>) {
        let key = (index, database.clone());
        if self.expanded_dbs.remove(&key) {
            cx.notify();
            return;
        }
        self.expanded_dbs.insert(key);
        let cache_key = (index, database.clone());
        if !self.db_tables.contains_key(&cache_key) {
            self.fetch_tables(index, database, cx);
        }
        cx.notify();
    }

    fn fetch_tables(&mut self, index: usize, database: String, cx: &mut Context<Self>) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let queries = self.queries.clone();
        let db_for_task = database.clone();
        let task = cx
            .background_executor()
            .spawn(async move { queries.tables(&profile, &db_for_task) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                let key = (index, database.clone());
                match outcome {
                    Ok(entries) => {
                        this.db_tables.insert(key, Some(entries));
                        this.sync_catalog(cx);
                        this.prefetch_structures(index, database.clone(), cx);
                    }
                    Err(err) => {
                        this.db_tables.insert(key, Some(Vec::new()));
                        this.column_errors
                            .insert((index, database), SharedString::from(err.to_string()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle_table(
        &mut self,
        index: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let key = (index, table.clone());
        if self.expanded_tables.remove(&key) {
            cx.notify();
            return;
        }
        self.expanded_tables.insert(key);
        let cache_key = (index, table.clone());
        if !self.table_structures.contains_key(&cache_key)
            && !self.column_errors.contains_key(&(index, table.clone()))
        {
            self.fetch_structure(index, database, table, cx);
        }
        cx.notify();
    }

    fn fetch_structure(
        &mut self,
        index: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profiles.get(index).cloned() else {
            return;
        };
        let queries = self.queries.clone();
        let db_for_task = database.clone();
        let table_for_task = table.clone();
        let task = cx
            .background_executor()
            .spawn(async move { queries.structure(&profile, &db_for_task, &table_for_task) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                let key = (index, table.clone());
                match outcome {
                    Ok(structure) => {
                        this.table_structures.insert(key, structure);
                        this.column_errors.remove(&(index, table));
                        this.sync_catalog(cx);
                    }
                    Err(err) => {
                        this.column_errors
                            .insert((index, table), SharedString::from(err.to_string()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh(&mut self) {
        match self.service.list() {
            Ok(profiles) => {
                self.profiles = profiles;
                self.error = None;
                if self.selected >= self.profiles.len() {
                    self.selected = self.profiles.len().saturating_sub(1);
                }
            }
            Err(err) => self.error = Some(SharedString::from(err.to_string())),
        }
    }

    fn open_structure_editor(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let name = cx.new(SqlEditor::new);
        let data_type = cx.new(SqlEditor::new);
        let index_name = cx.new(SqlEditor::new);
        let index_columns = cx.new(SqlEditor::new);
        name.update(cx, |editor, cx| editor.set_placeholder("列名", cx));
        index_name.update(cx, |editor, cx| editor.set_placeholder("索引名", cx));
        index_columns.update(cx, |editor, cx| editor.set_placeholder("列，逗号分隔", cx));
        data_type.update(cx, |editor, cx| editor.set_placeholder("类型，如 TEXT", cx));
        index_name.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("索引名", cx)
        });
        index_columns.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("列，逗号分隔", cx)
        });
        let (columns, indexes) = self
            .table_structures
            .get(&(conn, table.clone()))
            .map(|structure| (structure.columns.clone(), structure.indexes.clone()))
            .unwrap_or_default();
        self.structure_edit = Some(StructureEdit {
            conn,
            database,
            table,
            columns,
            indexes,
            name,
            data_type,
            index_name,
            index_columns,
            index_unique: false,
            nullable: true,
            pending: Vec::new(),
            preview: None,
            confirmed: false,
            pending_delete: None,
            rename_from: None,
            rename_editor: None,
            error: None,
            statements: Vec::new(),
        });
        cx.notify();
    }

    fn open_groups(&mut self, cx: &mut Context<Self>) {
        self.declared_groups = self.groups.declared();
        self.group_note = None;
        self.group_rename = None;
        self.groups_open = true;
        cx.notify();
    }

    fn close_groups(&mut self, cx: &mut Context<Self>) {
        self.groups_open = false;
        self.group_rename = None;
        cx.notify();
    }

    fn refresh_groups(&mut self, cx: &mut Context<Self>) {
        self.declared_groups = self.groups.declared();
        self.refresh();
        cx.notify();
    }

    fn create_group(&mut self, cx: &mut Context<Self>) {
        let name = self.group_new.read(cx).text().trim().to_string();
        match self.groups.create(&name) {
            Ok(()) => {
                self.group_note = Some(SharedString::from(format!("已创建分组「{name}」")));
                self.group_new
                    .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            }
            Err(err) => self.group_note = Some(SharedString::from(err)),
        }
        self.refresh_groups(cx);
    }

    fn start_rename_group(&mut self, name: String, cx: &mut Context<Self>) {
        let editor = cx.new(SqlEditor::new);
        editor.update(cx, |editor, cx| {
            editor.set_text(name.clone(), cx);
            editor.set_placeholder("新分组名", cx)
        });
        self.group_rename = Some((name, editor));
        cx.notify();
    }

    fn commit_rename_group(&mut self, from: String, cx: &mut Context<Self>) {
        let Some((_, editor)) = self.group_rename.as_ref() else {
            return;
        };
        let to = editor.read(cx).text().trim().to_string();
        match self.groups.rename(&from, &to) {
            Ok(changed) => {
                self.group_note = Some(SharedString::from(format!(
                    "已重命名「{from}」→「{to}」，{changed} 个连接随之迁移"
                )));
            }
            Err(err) => self.group_note = Some(SharedString::from(err)),
        }
        self.group_rename = None;
        self.refresh_groups(cx);
    }

    fn delete_group(&mut self, name: String, cx: &mut Context<Self>) {
        match self.groups.remove(&name) {
            Ok(changed) => {
                self.group_note = Some(SharedString::from(format!(
                    "已删除分组「{name}」，{changed} 个连接回到默认分组"
                )));
            }
            Err(err) => self.group_note = Some(SharedString::from(err)),
        }
        self.refresh_groups(cx);
    }

    /// 把连接移到下一个分组（循环）。
    fn move_profile_next(&mut self, id: String, cx: &mut Context<Self>) {
        let summaries = superlink_app::groups::summarize(&self.profiles, &self.declared_groups);
        let Some(profile) = self.profiles.iter().find(|profile| profile.id == id) else {
            return;
        };
        let current = superlink_app::groups::display_group(&profile.group);
        let index = summaries
            .iter()
            .position(|group| group.name == current)
            .map(|position| position + 1)
            .unwrap_or(0);
        let Some(next) = summaries.get(index % summaries.len().max(1)) else {
            return;
        };
        let target = next.name.clone();
        match self.groups.move_profile(&id, &target) {
            Ok(()) => self.group_note = Some(SharedString::from(format!("已移动到「{target}」"))),
            Err(err) => self.group_note = Some(SharedString::from(err)),
        }
        self.refresh_groups(cx);
    }

    fn close_structure_editor(&mut self, cx: &mut Context<Self>) {
        self.structure_edit = None;
        cx.notify();
    }

    /// 双击对象树中的表：打开工作台（数据页）。
    fn open_workbench(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let (columns, _indexes) = self
            .table_structures
            .get(&(conn, table.clone()))
            .map(|structure| (structure.columns.clone(), structure.indexes.clone()))
            .unwrap_or_default();
        self.structure_edit = None;
        self.wb_column.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("列名", cx)
        });
        self.wb_value.update(cx, |editor, cx| {
            editor.set_text(String::new(), cx);
            editor.set_placeholder("值", cx)
        });
        self.wb_import
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        let cached_columns = !columns.is_empty();
        self.workbench = Some(crate::workbench::Workbench::new(
            conn,
            database.clone(),
            table.clone(),
            columns,
        ));
        if cached_columns {
            self.fetch_workbench_data(cx);
        } else {
            // 结构还没缓存：先取一次结构，回来后再取数。
            let db = database.clone();
            let name = table.clone();
            self.fetch_structure(conn, db, name, cx);
            self.fetch_workbench_data(cx);
            cx.notify();
        }
    }

    fn close_workbench(&mut self, cx: &mut Context<Self>) {
        self.workbench = None;
        cx.notify();
    }

    fn fetch_workbench_data(&mut self, cx: &mut Context<Self>) {
        let Some(bench) = self.workbench.as_ref() else {
            return;
        };
        if bench.columns.is_empty() {
            return;
        }
        let Some(profile) = self.profiles.get(bench.conn).cloned() else {
            return;
        };
        let kind = profile.sql_dialect();
        let (data_sql, count_sql) = match bench.page_statements(kind) {
            Ok(statements) => statements,
            Err(err) => {
                if let Some(bench) = self.workbench.as_mut() {
                    bench.error = Some(SharedString::from(err.to_string()));
                }
                cx.notify();
                return;
            }
        };
        if let Some(bench) = self.workbench.as_mut() {
            bench.loading = true;
            bench.error = None;
        }
        let queries = self.queries.clone();
        let credentials = self
            .service
            .credentials(&profile)
            .ok()
            .flatten()
            .unwrap_or_default();
        let limit = crate::workbench::PAGE_SIZE;
        let data_task_data = data_sql.clone();
        let task = cx.background_executor().spawn(async move {
            let data = queries.execute(&profile, &credentials, &data_task_data, limit);
            let count = queries.execute(&profile, &credentials, &count_sql, 10);
            (data, count)
        });
        cx.spawn(async move |this, cx| {
            let (data, count) = task.await;
            this.update(cx, |this, cx| {
                if let Some(bench) = this.workbench.as_mut() {
                    bench.loading = false;
                    match (data, count) {
                        (Ok(data), Ok(count)) => {
                            bench.error = None;
                            bench.set_data(data, Some(&count));
                        }
                        (Err(err), _) | (_, Err(err)) => {
                            bench.error = Some(SharedString::from(err.to_string()));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn apply_workbench_filter(&mut self, cx: &mut Context<Self>) {
        let column = self.wb_column.read(cx).text().trim().to_string();
        let value = self.wb_value.read(cx).text().trim().to_string();
        let (committed, refresh) = {
            let Some(bench) = self.workbench.as_mut() else {
                return;
            };
            bench.draft.column = column;
            bench.draft.value = value;
            match bench.commit_draft() {
                Ok(()) => (true, bench.database.clone()),
                Err(err) => {
                    bench.error = Some(SharedString::from(err));
                    (false, String::new())
                }
            }
        };
        if !committed {
            cx.notify();
            return;
        }
        self.wb_column
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        self.wb_value
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        let _ = refresh;
        self.fetch_workbench_data(cx);
    }

    fn cycle_workbench_operator(&mut self, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.draft.operator = (bench.draft.operator + 1) % crate::workbench::OPERATORS.len();
        }
        cx.notify();
    }

    fn toggle_workbench_join(&mut self, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.draft.join_and = !bench.draft.join_and;
        }
        cx.notify();
    }

    fn remove_workbench_filter(&mut self, position: usize, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.filters.remove(position);
            bench.page = 1;
        }
        self.fetch_workbench_data(cx);
    }

    fn workbench_sort(&mut self, column: String, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.toggle_sort(&column);
        }
        self.fetch_workbench_data(cx);
    }

    /// 导入 CSV 到当前表：生成 INSERT 后执行，然后刷新数据。
    fn import_workbench_csv(&mut self, cx: &mut Context<Self>) {
        let path = self
            .wb_import
            .read(cx)
            .text()
            .trim()
            .trim_matches('"')
            .to_string();
        let Some(bench) = self.workbench.as_ref() else {
            return;
        };
        if path.is_empty() {
            if let Some(bench) = self.workbench.as_mut() {
                bench.error = Some("请填写导入文件的路径".into());
            }
            cx.notify();
            return;
        }
        let conn = bench.conn;
        let table = bench.table.clone();
        let columns = bench.columns.clone();
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return;
        };
        let kind = profile.sql_dialect();
        let outcome =
            superlink_app::transfer::import_csv_to_table(Path::new(&path), kind, &table, &columns);
        let (sql, _rows) = match outcome {
            Ok(result) => result,
            Err(err) => {
                if let Some(bench) = self.workbench.as_mut() {
                    bench.error = Some(SharedString::from(err));
                }
                cx.notify();
                return;
            }
        };
        let queries = self.queries.clone();
        let credentials = self
            .service
            .credentials(&profile)
            .ok()
            .flatten()
            .unwrap_or_default();
        let task = cx
            .background_executor()
            .spawn(async move { queries.execute(&profile, &credentials, &sql, 10) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if let Some(bench) = this.workbench.as_mut() {
                    match outcome {
                        Ok(_) => {
                            bench.error = None;
                            bench.page = 1;
                        }
                        Err(err) => {
                            bench.error = Some(SharedString::from(format!("导入执行失败：{err}")));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.fetch_workbench_data(cx);
    }

    fn workbench_page(&mut self, delta: i32, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            if delta < 0 {
                bench.go_prev();
            } else {
                bench.go_next();
            }
        }
        self.fetch_workbench_data(cx);
    }

    fn set_workbench_tab(&mut self, tab: crate::workbench::WorkbenchTab, cx: &mut Context<Self>) {
        if let Some(bench) = self.workbench.as_mut() {
            bench.tab = tab;
        }
        cx.notify();
    }

    /// 结构变更后刷新面板与表树缓存。
    fn refresh_structure(
        &mut self,
        conn: usize,
        database: String,
        table: String,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return;
        };
        let queries = self.queries.clone();
        let db_for_task = database.clone();
        let name = table.clone();
        let task = cx
            .background_executor()
            .spawn(async move { queries.structure(&profile, &db_for_task, &name) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if let Ok(structure) = outcome {
                    this.table_structures
                        .insert((conn, table.clone()), structure.clone());
                    if let Some(edit) = this.structure_edit.as_mut()
                        && edit.conn == conn
                        && edit.table == table
                    {
                        edit.columns = structure.columns;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// 新增列进入变更集（不立即执行）。
    fn queue_add_column(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.structure_edit.as_ref() else {
            return;
        };
        let name = edit.name.read(cx).text().trim().to_string();
        let data_type = edit.data_type.read(cx).text().trim().to_string();
        let nullable = edit.nullable;
        if name.is_empty() || data_type.is_empty() {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("列名和类型都不能为空".into());
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "addColumn".into(),
                column: superlink_sql::ColumnDefinition {
                    name: name.clone(),
                    data_type: data_type.clone(),
                    nullable: if nullable { "YES" } else { "NO" }.to_string(),
                    ..Default::default()
                },
                ..Default::default()
            });
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
            edit.name
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            edit.data_type
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        }
        cx.notify();
    }

    /// 删除列进入变更集（两步确认）。
    fn queue_drop_column(&mut self, column: String, cx: &mut Context<Self>) {
        let confirm_needed = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            edit.pending_delete.as_deref() != Some(column.as_str())
        };
        if confirm_needed {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.pending_delete = Some(column);
                edit.error = None;
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "dropColumn".into(),
                original_name: column.clone(),
                column: superlink_sql::ColumnDefinition {
                    name: column,
                    data_type: String::new(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
                ..Default::default()
            });
            edit.pending_delete = None;
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
        }
        cx.notify();
    }

    /// 改名进入变更集：第一次点击出现内联输入，第二次确认。
    fn queue_rename_column(&mut self, column: String, cx: &mut Context<Self>) {
        let rename_requested = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            edit.rename_from.as_deref() == Some(column.as_str())
        };
        if !rename_requested {
            if let Some(edit) = self.structure_edit.as_mut() {
                let editor = cx.new(SqlEditor::new);
                editor.update(cx, |editor, cx| {
                    editor.set_text(column.clone(), cx);
                    editor.set_placeholder("新列名", cx)
                });
                edit.rename_from = Some(column);
                edit.rename_editor = Some(editor);
                edit.error = None;
            }
            cx.notify();
            return;
        }
        let new_name = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            let Some(editor) = edit.rename_editor.as_ref() else {
                return;
            };
            editor.read(cx).text().trim().to_string()
        };
        if new_name.is_empty() || new_name == column {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("请输入与原列名不同的新列名".into());
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            let original = edit
                .columns
                .iter()
                .find(|entry| entry.name == column)
                .cloned()
                .unwrap_or_default();
            edit.pending.push(superlink_sql::StructureChange {
                kind: "alterColumn".into(),
                original_name: column.clone(),
                column: superlink_sql::ColumnDefinition {
                    name: new_name,
                    data_type: original.data_type,
                    nullable: if original.nullable { "YES" } else { "NO" }.to_string(),
                    key: if original.primary_key { "PRI" } else { "" }.to_string(),
                    default: original.default_value.clone(),
                    has_default: original.default_value.is_some(),
                    comment: original.comment.clone().unwrap_or_default(),
                    ..Default::default()
                },
                ..Default::default()
            });
            edit.rename_from = None;
            edit.rename_editor = None;
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
        }
        cx.notify();
    }

    /// 添加索引进入变更集。
    fn queue_add_index(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.structure_edit.as_ref() else {
            return;
        };
        let name = edit.index_name.read(cx).text().trim().to_string();
        let columns_text = edit.index_columns.read(cx).text().trim().to_string();
        let unique = edit.index_unique;
        if name.is_empty() || columns_text.is_empty() {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("索引名和列都不能为空".into());
            }
            cx.notify();
            return;
        }
        let columns: Vec<superlink_sql::IndexColumn> = columns_text
            .split(',')
            .map(|column| superlink_sql::IndexColumn {
                name: column.trim().to_string(),
                descending: false,
            })
            .filter(|column| !column.name.is_empty())
            .collect();
        if columns.is_empty() {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.error = Some("索引列不能为空".into());
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "addIndex".into(),
                index: superlink_sql::NewIndex {
                    name: name.clone(),
                    method: String::new(),
                    columns,
                    unique,
                },
                ..Default::default()
            });
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
            edit.index_name
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            edit.index_columns
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        }
        cx.notify();
    }

    /// 删除索引进入变更集（两步确认）。
    fn queue_drop_index(&mut self, name: String, cx: &mut Context<Self>) {
        let confirm_needed = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            edit.pending_delete.as_deref() != Some(format!("index:{name}").as_str())
        };
        if confirm_needed {
            if let Some(edit) = self.structure_edit.as_mut() {
                edit.pending_delete = Some(format!("index:{name}"));
                edit.error = None;
            }
            cx.notify();
            return;
        }
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.pending.push(superlink_sql::StructureChange {
                kind: "dropIndex".into(),
                original_name: name.clone(),
                ..Default::default()
            });
            edit.pending_delete = None;
            edit.preview = None;
            edit.confirmed = false;
            edit.error = None;
        }
        cx.notify();
    }

    fn toggle_index_unique(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.structure_edit.as_mut() {
            edit.index_unique = !edit.index_unique;
        }
        cx.notify();
    }

    fn remove_pending_change(&mut self, position: usize, cx: &mut Context<Self>) {
        if let Some(edit) = self.structure_edit.as_mut() {
            if position < edit.pending.len() {
                edit.pending.remove(position);
                edit.preview = None;
                edit.confirmed = false;
            }
        }
        cx.notify();
    }

    /// 受保护提交第一步：生成 SQL 预览。
    fn build_structure_preview(&mut self, cx: &mut Context<Self>) {
        let payload = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            if edit.pending.is_empty() {
                if let Some(edit) = self.structure_edit.as_mut() {
                    edit.error = Some("没有结构变更".into());
                }
                cx.notify();
                return;
            }
            let kind = self
                .profiles
                .get(edit.conn)
                .map(|profile| profile.sql_dialect().to_string())
                .unwrap_or_default();
            let info = superlink_app::structure::table_info_from(&edit.columns, &edit.indexes);
            let (schema, name) = match edit.table.split_once('.') {
                Some((schema, name)) => (schema.to_string(), name.to_string()),
                None => (String::new(), edit.table.clone()),
            };
            let request = superlink_sql::StructureRequest {
                object: superlink_sql::Object {
                    name,
                    kind: "table".into(),
                    schema,
                    ..Default::default()
                },
                changes: edit.pending.clone(),
                ..Default::default()
            };
            superlink_sql::structure::build_structure_changes(&kind, &request, &info)
                .map(|statements| superlink_sql::structure::preview_structure(&statements))
        };
        if let Some(edit) = self.structure_edit.as_mut() {
            match payload {
                Ok(preview) => {
                    edit.preview = Some(preview);
                    edit.confirmed = false;
                    edit.error = None;
                }
                Err(err) => edit.error = Some(SharedString::from(err.to_string())),
            }
        }
        cx.notify();
    }

    /// 受保护提交第二步：确认后在单事务内执行；失败整体回滚。
    fn commit_structure(&mut self, cx: &mut Context<Self>) {
        let payload = {
            let Some(edit) = self.structure_edit.as_ref() else {
                return;
            };
            if edit.preview.is_none() {
                if let Some(edit) = self.structure_edit.as_mut() {
                    edit.error = Some("请先生成 SQL 预览".into());
                }
                cx.notify();
                return;
            }
            if !edit.confirmed {
                if let Some(edit) = self.structure_edit.as_mut() {
                    edit.confirmed = true;
                }
                cx.notify();
                return;
            }
            (
                edit.conn,
                edit.database.clone(),
                edit.table.clone(),
                edit.pending.clone(),
            )
        };
        let (conn, database, table, changes) = payload;
        let refresh_db = database.clone();
        let refresh_table = table.clone();
        let Some(profile) = self.profiles.get(conn).cloned() else {
            return;
        };
        let service = self.structure.clone();
        let flags = superlink_app::query::NativeFlags::from_env();
        let task = cx
            .background_executor()
            .spawn(async move { service.apply_with(&profile, &database, &table, changes, flags) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                if let Some(edit) = this.structure_edit.as_mut() {
                    match outcome {
                        Ok(statements) => {
                            edit.statements = statements;
                            edit.pending.clear();
                            edit.preview = None;
                            edit.confirmed = false;
                            edit.error = None;
                        }
                        Err(err) => edit.error = Some(SharedString::from(err.to_string())),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.refresh_structure(conn, refresh_db, refresh_table, cx);
    }
    fn groups_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let summaries = superlink_app::groups::summarize(&self.profiles, &self.declared_groups);

        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("管理连接分组"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("close-groups")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.close_groups(cx)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("关闭"),
                            ),
                    ),
            );

        if let Some(note) = &self.group_note {
            panel = panel.child(
                div()
                    .mx_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(note.clone()),
            );
        }

        // 新建分组
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(200.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(self.group_new.clone()),
                )
                .child(
                    div()
                        .id("create-group")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(cx.listener(|this, _event, _window, cx| this.create_group(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("新建分组"),
                        ),
                ),
        );

        // 分组列表
        let mut list = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .pt_2()
            .flex_1()
            .min_h(px(0.0));
        for group in &summaries {
            let name = group.name.clone();
            let rename_target = name.clone();
            let is_default = name == superlink_app::groups::DEFAULT_GROUP;
            let renaming = self
                .group_rename
                .as_ref()
                .map(|(from, _)| from == &name)
                .unwrap_or(false);
            let mut block = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(theme.canvas);
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(SharedString::from(format!(
                                "{} · {}",
                                group.name, group.count
                            ))),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(SharedString::from(format!("rename-group-{name}")))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.accent)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.start_rename_group(rename_target.clone(), cx)
                            }))
                            .child("重命名"),
                    )
                    .children((!is_default).then(|| {
                        let delete_name = group.name.clone();
                        div()
                            .id(SharedString::from(format!("delete-group-{name}")))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.delete_group(delete_name.clone(), cx)
                            }))
                            .child("删除")
                    })),
            );

            if renaming {
                if let Some((_, editor)) = &self.group_rename {
                    let commit_name = group.name.clone();
                    block = block.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w(px(180.0))
                                    .h(px(crate::theme::CONTROL_MIN))
                                    .px_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.surface)
                                    .child(editor.clone()),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("confirm-rename-{name}")))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.commit_rename_group(commit_name.clone(), cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.surface)
                                            .child("确认改名"),
                                    ),
                            ),
                    );
                }
            }

            // 该分组的连接
            for profile in self.profiles.iter().filter(|profile| {
                superlink_app::groups::display_group(&profile.group) == group.name
            }) {
                let id = profile.id.clone();
                let profile_name = profile.name.clone();
                block = block.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl(px(12.0))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(SharedString::from(profile_name)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(SharedString::from(profile.sql_dialect().to_uppercase())),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(SharedString::from(format!("move-{id}")))
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.accent)
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.move_profile_next(id.clone(), cx)
                                }))
                                .child("移到下一组"),
                        ),
                );
            }
            list = list.child(block);
        }
        panel.child(list).into_any_element()
    }
    fn workbench_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(bench) = self.workbench.as_ref() else {
            return div().into_any_element();
        };
        let table = bench.table.clone();
        let tab = bench.tab;
        let operator = crate::workbench::OPERATORS[bench.draft.operator];
        let join_and = bench.draft.join_and;

        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(format!("表工作台 · {table}")),
                    )
                    .child(div().flex_1())
                    .children(crate::workbench::WorkbenchTab::ALL.iter().map(|t| {
                        let active = *t == tab;
                        div()
                            .id(SharedString::from(format!("wb-tab-{:?}", t)))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if active {
                                theme.accent_soft
                            } else {
                                theme.surface
                            })
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.set_workbench_tab(*t, cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(if active { theme.accent } else { theme.muted })
                                    .child(t.label()),
                            )
                    }))
                    .child(
                        div()
                            .id("close-workbench")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.close_workbench(cx)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("关闭"),
                            ),
                    ),
            );

        if let Some(error) = &bench.error {
            panel = panel.child(
                div()
                    .mx_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }

        match tab {
            crate::workbench::WorkbenchTab::Data => {
                // 条件构建行
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .w(px(150.0))
                                .h(px(crate::theme::CONTROL_MIN))
                                .px_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(self.wb_column.clone()),
                        )
                        .child(
                            div()
                                .id("wb-operator")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .border_1()
                                .border_color(theme.border)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.cycle_workbench_operator(cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.accent)
                                        .child(operator),
                                ),
                        )
                        .child(
                            div()
                                .id("wb-join")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .border_1()
                                .border_color(theme.border)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.toggle_workbench_join(cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child(if join_and { "且" } else { "或" }),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .w(px(150.0))
                                .h(px(crate::theme::CONTROL_MIN))
                                .px_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(self.wb_value.clone()),
                        )
                        .child(
                            div()
                                .id("wb-apply-filter")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(theme.accent)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.apply_workbench_filter(cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.surface)
                                        .child("添加条件"),
                                ),
                        ),
                );

                if !bench.filters.is_empty() {
                    let mut chips = div().flex().flex_wrap().items_center().gap_1().px_3();
                    for (position, filter) in bench.filters.iter().enumerate() {
                        let position_copy = position;
                        chips = chips.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(theme.canvas)
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child(SharedString::from(format!(
                                            "{} {} {}",
                                            filter.column, filter.operator, filter.value
                                        ))),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!("filter-x-{position}")))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.remove_workbench_filter(position_copy, cx)
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(crate::theme::TEXT_SIZE))
                                                .text_color(theme.muted)
                                                .child("×"),
                                        ),
                                ),
                        );
                    }
                    panel = panel.child(chips);
                }

                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("导入 CSV"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .w(px(320.0))
                                .h(px(crate::theme::CONTROL_MIN))
                                .px_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(self.wb_import.clone()),
                        )
                        .child(
                            div()
                                .id("wb-import")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .border_1()
                                .border_color(theme.border)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.import_workbench_csv(cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child("导入"),
                                ),
                        ),
                );

                if bench.loading {
                    panel = panel.child(
                        div()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("加载中…"),
                    );
                    return panel.into_any_element();
                }

                let Some(result) = bench.result.as_ref() else {
                    panel = panel.child(
                        div()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("选择条件后加载数据"),
                    );
                    return panel.into_any_element();
                };

                let total_label = bench
                    .total
                    .map(|total| format!("共 {total} 行"))
                    .unwrap_or_default();
                panel = panel.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(total_label),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id("wb-prev")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .border_1()
                                .border_color(theme.border)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.workbench_page(-1, cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child("上一页"),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(format!("第 {} 页", bench.page)),
                        )
                        .child(
                            div()
                                .id("wb-next")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .border_1()
                                .border_color(theme.border)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.workbench_page(1, cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child("下一页"),
                                ),
                        ),
                );

                let header = div()
                    .flex()
                    .items_center()
                    .h(px(crate::theme::CONTROL_MIN))
                    .bg(theme.canvas)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .w(px(crate::query_page::ROW_NUMBER_WIDTH))
                            .px_2()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("#"),
                    )
                    .children(result.columns.iter().map(|column| {
                        let mark = bench.sort_mark(&column.name);
                        let name = column.name.clone();
                        div()
                            .id(SharedString::from(format!("wb-sort-{}", column.name)))
                            .w(px(crate::query_page::COLUMN_WIDTH))
                            .px_2()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.muted)
                            .cursor_pointer()
                            .hover(move |style| style.text_color(theme.accent))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.workbench_sort(name.clone(), cx)
                            }))
                            .child(SharedString::from(format!("{}{mark}", column.name)))
                    }));

                let rows = result.rows.iter().enumerate().map(|(index, row)| {
                    div()
                        .flex()
                        .items_center()
                        .h(px(24.0))
                        .bg(if index % 2 == 0 {
                            theme.surface
                        } else {
                            theme.canvas
                        })
                        .child(
                            div()
                                .w(px(crate::query_page::ROW_NUMBER_WIDTH))
                                .px_2()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(format!("{}", index + 1)),
                        )
                        .children(row.iter().map(|cell| {
                            div()
                                .w(px(crate::query_page::COLUMN_WIDTH))
                                .px_2()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(crate::query_page::cell_text(cell))
                        }))
                });

                panel = panel.child(header).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.0))
                        .children(rows),
                );
            }
            crate::workbench::WorkbenchTab::Structure => {
                let mut section = div().flex().flex_col().gap_1().p_3();
                for column in &bench.columns {
                    let meta = if column.primary_key {
                        format!("{} · PK", column.data_type)
                    } else if column.nullable {
                        column.data_type.clone()
                    } else {
                        format!("{} · NOT NULL", column.data_type)
                    };
                    section = section.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(SharedString::from(column.name.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(SharedString::from(meta)),
                            )
                            .children(column.comment.as_ref().map(|comment| {
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(SharedString::from(comment.clone()))
                            })),
                    );
                }
                if let Some(structure) = self
                    .table_structures
                    .get(&(bench.conn, bench.table.clone()))
                {
                    if !structure.indexes.is_empty() {
                        section = section.child(
                            div()
                                .pt_2()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("索引"),
                        );
                        for index_entry in &structure.indexes {
                            section = section.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child(SharedString::from(index_entry.name.clone())),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.muted)
                                            .child(SharedString::from(
                                                index_entry.columns.join(", "),
                                            )),
                                    ),
                            );
                        }
                    }
                    if !structure.foreign_keys.is_empty() {
                        section = section.child(
                            div()
                                .pt_2()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("外键"),
                        );
                        for key in &structure.foreign_keys {
                            section = section.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child(SharedString::from(key.columns.join(", "))),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.muted)
                                            .child("→"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.muted)
                                            .child(SharedString::from(format!(
                                                "{}({})",
                                                key.referenced_table,
                                                key.referenced_columns.join(", ")
                                            ))),
                                    ),
                            );
                        }
                    }
                }
                panel = panel.child(section);
            }
            crate::workbench::WorkbenchTab::Ddl => {
                let kind = self
                    .profiles
                    .get(bench.conn)
                    .map(|profile| profile.sql_dialect().to_string())
                    .unwrap_or_default();
                let text = match bench.ddl(&kind) {
                    Ok(ddl) => SharedString::from(ddl),
                    Err(err) => SharedString::from(err.to_string()),
                };
                panel = panel.child(
                    div()
                        .p_3()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child(text),
                );
            }
        }
        panel.into_any_element()
    }

    fn structure_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(edit) = self.structure_edit.as_ref() else {
            return div().into_any_element();
        };
        let table = edit.table.clone();
        let confirmed = edit.confirmed;
        let nullable = edit.nullable;

        // 顶部：标题 + 关闭
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(format!("结构编辑 · {table}")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("close-structure")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.close_structure_editor(cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("关闭"),
                            ),
                    ),
            );

        // 错误提示
        if let Some(error) = &edit.error {
            panel = panel.child(
                div()
                    .mx_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }

        // 添加列表单
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(150.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(150.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.data_type.clone()),
                )
                .child(
                    div()
                        .id("toggle-nullable")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(if edit.nullable {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .bg(if nullable {
                            theme.accent_soft
                        } else {
                            theme.surface
                        })
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            if let Some(edit) = this.structure_edit.as_mut() {
                                edit.nullable = !edit.nullable;
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if nullable { theme.accent } else { theme.muted })
                                .child(if nullable { "可空" } else { "非空" }),
                        ),
                )
                .child(
                    div()
                        .id("queue-add-column")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.queue_add_column(cx)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("加入变更集"),
                        ),
                ),
        );

        // 索引：现有索引（可删）+ 添加索引表单
        let mut index_section = div().flex().flex_col().gap_1().px_3();
        index_section = index_section.child(
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child("索引"),
        );
        for index in &edit.indexes {
            let name = index.name.clone();
            let deletable =
                !name.eq_ignore_ascii_case("PRIMARY") && !name.starts_with("sqlite_autoindex");
            let deleting = edit.pending_delete.as_deref() == Some(format!("index:{name}").as_str());
            let mut row = div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(theme.canvas)
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child(SharedString::from(index.name.clone())),
                )
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .child(SharedString::from(index.columns.join(", "))),
                );
            if deletable {
                row = row.child(
                    div()
                        .id(SharedString::from(format!("index-drop-{name}")))
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(if deleting { theme.danger } else { theme.muted })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.queue_drop_index(name.clone(), cx)
                        }))
                        .child(if deleting { "确认删除" } else { "删除" }),
                );
            }
            index_section = index_section.child(row);
        }
        index_section = index_section.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(140.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.index_name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(180.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(edit.index_columns.clone()),
                )
                .child(
                    div()
                        .id("toggle-index-unique")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(if edit.index_unique {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .bg(if edit.index_unique {
                            theme.accent_soft
                        } else {
                            theme.surface
                        })
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.toggle_index_unique(cx)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if edit.index_unique {
                                    theme.accent
                                } else {
                                    theme.muted
                                })
                                .child("唯一"),
                        ),
                )
                .child(
                    div()
                        .id("queue-add-index")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(cx.listener(|this, _event, _window, cx| this.queue_add_index(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("添加索引"),
                        ),
                ),
        );
        panel = panel.child(index_section);

        // 变更集
        let mut changes = div().flex().flex_col().gap_1().px_3();
        changes = changes.child(
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child(format!("变更集 ({})", edit.pending.len())),
        );
        for (position, change) in edit.pending.iter().enumerate() {
            let summary = match change.kind.as_str() {
                "addColumn" => format!("+ 添加列 {}", change.column.name),
                "dropColumn" => format!("- 删除列 {}", change.original_name),
                "alterColumn" => {
                    format!("~ 重命名 {} → {}", change.original_name, change.column.name)
                }
                other => format!("… {other}"),
            };
            changes = changes.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.canvas)
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(summary)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("pending-x-{position}")))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.remove_pending_change(position, cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("×"),
                            ),
                    ),
            );
        }
        panel = panel.child(changes);

        // 预览与受保护提交
        panel =
            panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .child(
                        div()
                            .id("build-preview")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .border_1()
                            .border_color(theme.border)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.build_structure_preview(cx)
                            }))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child("生成 SQL 预览"),
                            ),
                    )
                    .child(
                        div()
                            .id("commit-structure")
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if edit.preview.is_some() {
                                theme.accent
                            } else {
                                theme.border
                            })
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.commit_structure(cx)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if edit.preview.is_some() {
                                        theme.surface
                                    } else {
                                        theme.muted
                                    })
                                    .child(if confirmed { "确认提交" } else { "提交" }),
                            ),
                    ),
            );
        if let Some(preview) = &edit.preview {
            panel = panel.child(
                div()
                    .mx_3()
                    .p_2()
                    .rounded_md()
                    .bg(theme.canvas)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(SharedString::from(preview.clone())),
            );
        }

        // 已执行
        if !edit.statements.is_empty() {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mx_3()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("已执行"),
                    )
                    .children(edit.statements.iter().map(|statement| {
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(statement.clone()))
                    })),
            );
        }

        // 当前列 + 每列的删除/改名
        let mut columns = div()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .flex_1()
            .min_h(px(0.0));
        columns = columns.child(
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child("当前列"),
        );
        for column in &edit.columns {
            let name = column.name.clone();
            let rename_name = name.clone();
            let drop_name = name.clone();
            let renaming = edit.rename_from.as_deref() == Some(name.as_str());
            let deleting = edit.pending_delete.as_deref() == Some(name.as_str());
            let meta = if column.primary_key {
                format!("{} · PK", column.data_type)
            } else if column.nullable {
                column.data_type.clone()
            } else {
                format!("{} · NOT NULL", column.data_type)
            };
            columns = columns.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(theme.canvas)
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(column.name.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child(SharedString::from(meta)),
                    )
                    .children(column.comment.as_ref().map(|comment| {
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child(SharedString::from(comment.clone()))
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(SharedString::from(format!("rename-{}", name)))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.accent)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.queue_rename_column(rename_name.clone(), cx)
                            }))
                            .child("改名"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("drop-{}", name.clone())))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if deleting { theme.danger } else { theme.muted })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.queue_drop_column(drop_name.clone(), cx)
                            }))
                            .child(if deleting { "确认删除" } else { "删除" }),
                    ),
            );
            if renaming {
                if let Some(editor) = &edit.rename_editor {
                    columns = columns.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pl(px(20.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .w(px(150.0))
                                    .h(px(crate::theme::CONTROL_MIN))
                                    .px_2()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.surface)
                                    .child(editor.clone()),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("rename-ok-{}", name.clone())))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.queue_rename_column(name.clone(), cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.surface)
                                            .child("确认改名"),
                                    ),
                            ),
                    );
                }
            }
        }

        panel.child(columns).into_any_element()
    }

    /// 当前连接的表结构；只有原生 SQLite 会填充，其余显示说明。
    /// 一张表的行 + 展开后的列/索引/外键。
    fn matches_filter(&self, entry: &superlink_drivers::TableEntry) -> bool {
        self.filter_kind.is_empty() || entry.kind == self.filter_kind
    }

    fn table_block(
        &self,
        index: usize,
        database: String,
        entry: &superlink_drivers::TableEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let name = entry.name.clone();
        let is_view = entry.kind == "view";
        let expanded = self.expanded_tables.contains(&(index, name.clone()));
        let toggle_name = entry.name.clone();
        let edit_database = database.clone();
        let open_name = entry.name.clone();
        let open_database = database.clone();
        let mut block = div().flex().flex_col();
        block = block.child(
            div()
                .id(SharedString::from(format!("table-{index}-{}", entry.name)))
                .flex()
                .items_center()
                .gap_1()
                .pl(px(30.0))
                .py(px(1.0))
                .rounded_md()
                .hover(move |style| style.bg(theme.accent_soft))
                .child(
                    div()
                        .id(SharedString::from(format!("fold-{index}-{}", entry.name)))
                        .w(px(14.0))
                        .flex()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.toggle_table(index, edit_database.clone(), toggle_name.clone(), cx)
                        }))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(if expanded { "▾" } else { "▸" }),
                        ),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("open-{index}-{}", entry.name)))
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .on_click(cx.listener(
                            move |this, event: &gpui::ClickEvent, _window, cx| {
                                if event.click_count() >= 2 {
                                    this.open_workbench(
                                        index,
                                        open_database.clone(),
                                        open_name.clone(),
                                        cx,
                                    );
                                    cx.notify();
                                    return;
                                }
                                let table = name.replace('"', "\"\"");
                                this.workspace = Workspace::Sql;
                                this.query_page.update(cx, |page, cx| {
                                    page.load_sql(
                                        format!("SELECT * FROM \"{table}\" LIMIT 100;"),
                                        cx,
                                    )
                                });
                                cx.notify();
                            },
                        ))
                        .child(icons::icon(
                            if is_view {
                                "superlink/all-objects.svg"
                            } else {
                                "superlink/table.svg"
                            },
                            12.0,
                            theme.success,
                        ))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(SharedString::from(entry.name.clone())),
                        ),
                ),
        );

        if expanded {
            let edit_name = entry.name.clone();
            block = block.child(
                div()
                    .id(SharedString::from(format!("edit-{index}-{}", entry.name)))
                    .flex()
                    .items_center()
                    .pl(px(44.0))
                    .py(px(1.0))
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.open_structure_editor(index, database.clone(), edit_name.clone(), cx)
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.accent)
                            .child("编辑结构…"),
                    ),
            );

            if let Some(error) = self.column_errors.get(&(index, entry.name.clone())) {
                block = block.child(
                    div()
                        .pl(px(44.0))
                        .py_1()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .child(error.clone()),
                );
            } else if let Some(structure) = self.table_structures.get(&(index, entry.name.clone()))
            {
                for column in &structure.columns {
                    let meta = if column.primary_key {
                        format!("{} · PK", column.data_type)
                    } else if column.nullable {
                        column.data_type.clone()
                    } else {
                        format!("{} · NOT NULL", column.data_type)
                    };
                    block = block.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .pl(px(44.0))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(SharedString::from(column.name.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(SharedString::from(meta)),
                            )
                            .children(column.comment.as_ref().map(|comment| {
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(SharedString::from(comment.clone()))
                            })),
                    );
                }
                if structure.columns.is_empty() {
                    block = block.child(
                        div()
                            .pl(px(44.0))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("没有列"),
                    );
                }
                if !structure.indexes.is_empty() {
                    block = block.child(
                        div()
                            .pl(px(44.0))
                            .pt_1()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("索引"),
                    );
                    for index_entry in &structure.indexes {
                        let columns = index_entry.columns.join(", ");
                        let label = if index_entry.unique {
                            format!("{columns} · 唯一")
                        } else {
                            columns
                        };
                        block = block.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .pl(px(52.0))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child(SharedString::from(index_entry.name.clone())),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child(SharedString::from(label)),
                                ),
                        );
                    }
                }
                if !structure.foreign_keys.is_empty() {
                    block = block.child(
                        div()
                            .pl(px(44.0))
                            .pt_1()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("外键"),
                    );
                    for key in &structure.foreign_keys {
                        let from = key.columns.join(", ");
                        let to = format!(
                            "{}({})",
                            key.referenced_table,
                            key.referenced_columns.join(", ")
                        );
                        block = block.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .pl(px(52.0))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.text)
                                        .child(SharedString::from(from)),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child("→"),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child(SharedString::from(to)),
                                ),
                        );
                    }
                }
            } else {
                block = block.child(
                    div()
                        .pl(px(44.0))
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .child("加载中…"),
                );
            }
        }
        block.into_any_element()
    }

    /// 连接节点的对象子树：库（或直接表）→ 表 → 列。
    fn conn_subtree(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let mut subtree = div().flex().flex_col();
        match self.conn_dbs.get(&index) {
            None | Some(None) => {
                subtree = subtree.child(
                    div()
                        .pl(px(30.0))
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .child("加载中…"),
                )
            }
            Some(Some(databases)) if databases.is_empty() => {
                // 无库层级（SQLite）：连接下直接挂表。
                let state = self.db_tables.get(&(index, String::new()));
                match state {
                    Some(Some(tables)) => {
                        if tables.is_empty() {
                            subtree = subtree.child(
                                div()
                                    .pl(px(30.0))
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("没有表"),
                            );
                        }
                        for entry in tables
                            .iter()
                            .filter(|entry| self.matches_filter(entry))
                            .take(200)
                        {
                            subtree =
                                subtree.child(self.table_block(index, String::new(), entry, cx));
                        }
                    }
                    _ => {
                        subtree = subtree.child(
                            div()
                                .pl(px(30.0))
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("加载中…"),
                        )
                    }
                }
            }
            Some(Some(databases)) => {
                if let Some(note) = self.conn_db_note.get(&index) {
                    subtree = subtree.child(
                        div()
                            .pl(px(30.0))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child(note.clone()),
                    );
                }
                for database in databases.iter().take(100) {
                    let expanded = self.expanded_dbs.contains(&(index, database.clone()));
                    let toggle_db = database.clone();
                    subtree = subtree.child(
                        div()
                            .id(SharedString::from(format!("db-{index}-{database}")))
                            .flex()
                            .items_center()
                            .gap_2()
                            .mx_2()
                            .my(px(1.0))
                            .h(px(36.0))
                            .pl(px(40.0))
                            .pr_2()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.toggle_db(index, toggle_db.clone(), cx)
                            }))
                            .child(icons::icon(
                                if expanded {
                                    "shell/chevron-down.svg"
                                } else {
                                    "shell/chevron-right.svg"
                                },
                                13.0,
                                theme.muted,
                            ))
                            .child(icons::icon("superlink/database.svg", 17.0, theme.accent))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(theme.text)
                                    .child(SharedString::from(database.clone())),
                            ),
                    );
                    if !expanded {
                        continue;
                    }
                    let state = self.db_tables.get(&(index, database.clone()));
                    match state {
                        Some(Some(tables)) => {
                            if tables.is_empty() {
                                subtree = subtree.child(
                                    div()
                                        .pl(px(44.0))
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child("没有表"),
                                );
                            }
                            for entry in tables
                                .iter()
                                .filter(|entry| self.matches_filter(entry))
                                .take(200)
                            {
                                subtree = subtree.child(self.table_block(
                                    index,
                                    database.clone(),
                                    entry,
                                    cx,
                                ));
                            }
                        }
                        _ => {
                            subtree = subtree.child(
                                div()
                                    .pl(px(44.0))
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("加载中…"),
                            )
                        }
                    }
                }
            }
        }
        subtree.into_any_element()
    }
    fn sidebar_icon_button(
        &self,
        id: &'static str,
        icon: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.0))
            .rounded_md()
            .cursor_pointer()
            .bg(if active { theme.accent_soft } else { theme.sidebar })
            .hover(move |style| style.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _event, window, cx| handler(this, window, cx)))
            .child(icons::icon(
                icon,
                16.0,
                if active { theme.accent } else { theme.muted },
            ))
    }

    fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = !self.search_open;
        if self.search_open {
            use gpui::Focusable as _;
            let handle = self.sql_search.read(cx).focus_handle(cx);
            window.focus(&handle);
        } else {
            self.sql_search
                .update(cx, |editor, cx| editor.set_text("", cx));
        }
        cx.notify();
    }

    fn locate_connection(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let index = self.selected;
        self.expanded_conns.insert(index);
        if !self.conn_dbs.contains_key(&index) {
            self.fetch_databases(index, cx);
        }
        self.workspace = Workspace::Sql;
        self.sync_selection(cx);
        cx.notify();
    }

    fn refresh_connections(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.refresh();
        cx.notify();
    }

    fn open_connection_menu(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_groups(cx);
    }

    fn filter_all(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind.clear();
        cx.notify();
    }

    fn filter_tables(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind = "table".into();
        cx.notify();
    }

    fn filter_views(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind = "view".into();
        cx.notify();
    }

    fn filter_functions(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.filter_kind = "function".into();
        cx.notify();
    }

    fn saved_queries_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace = Workspace::Sql;
        self.query_page
            .update(cx, |page, cx| page.open_saved(window, cx));
        cx.notify();
    }

    fn saved_open_indicator(&self, cx: &Context<Self>) -> bool {
        self.query_page.read(cx).saved_open()
    }

    /// 连接侧栏：标题与工具行、对象类型过滤行、分组树。
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut column = div()
            .flex()
            .flex_col()
            .w(px(264.0))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(52.0))
                    .pl_3()
                    .pr_2()
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("连接"),
                    )
                    .child(div().flex_1())
                    .child(self.sidebar_icon_button(
                        "conn-search",
                        "superlink/search.svg",
                        self.search_open,
                        cx,
                        Self::toggle_search,
                    ))
                    .child(self.sidebar_icon_button(
                        "conn-locate",
                        "superlink/locate.svg",
                        false,
                        cx,
                        Self::locate_connection,
                    ))
                    .child(self.sidebar_icon_button(
                        "refresh-connections",
                        "superlink/refresh.svg",
                        false,
                        cx,
                        Self::refresh_connections,
                    ))
                    .child(self.sidebar_icon_button(
                        "connection-menu",
                        "superlink/connection-menu.svg",
                        false,
                        cx,
                        Self::open_connection_menu,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_3()
                    .pb_2()
                    .child(self.sidebar_icon_button(
                        "filter-all",
                        "superlink/all-objects.svg",
                        self.filter_kind.is_empty(),
                        cx,
                        Self::filter_all,
                    ))
                    .child(self.sidebar_icon_button(
                        "filter-table",
                        "superlink/table.svg",
                        self.filter_kind == "table",
                        cx,
                        Self::filter_tables,
                    ))
                    .child(self.sidebar_icon_button(
                        "filter-view",
                        "superlink/view.svg",
                        self.filter_kind == "view",
                        cx,
                        Self::filter_views,
                    ))
                    .child(self.sidebar_icon_button(
                        "filter-function",
                        "superlink/function.svg",
                        self.filter_kind == "function",
                        cx,
                        Self::filter_functions,
                    ))
                    .child(self.sidebar_icon_button(
                        "saved-queries-icon",
                        "superlink/sql-doc.svg",
                        self.saved_open_indicator(cx),
                        cx,
                        Self::saved_queries_action,
                    ))
                    .child(self.sidebar_icon_button(
                        "execution-history",
                        "superlink/history.svg",
                        false,
                        cx,
                        Self::history_action,
                    )),
            );

        if self.search_open {
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .mx_2()
                    .mb_2()
                    .px_2()
                    .h(px(crate::theme::CONTROL_MIN))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                    .child(self.sql_search.clone()),
            );
        }

        if self.profiles.is_empty() {
            column = column.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("还没有连接。点击上方「新建连接」开始。"),
            );
        }

        let needle = self.sql_search.read(cx).text().to_lowercase();
        let mut groups: BTreeMap<String, Vec<(usize, &Profile)>> = BTreeMap::new();
        for (index, profile) in self.profiles.iter().enumerate() {
            if !needle.is_empty()
                && !profile.name.to_lowercase().contains(&needle)
                && !profile.sql_dialect().to_lowercase().contains(&needle)
                && !profile.group.to_lowercase().contains(&needle)
            {
                continue;
            }
            let key = if profile.group.trim().is_empty() {
                "未分组".to_string()
            } else {
                profile.group.clone()
            };
            groups.entry(key).or_default().push((index, profile));
        }
        for name in &self.declared_groups {
            groups.entry(name.clone()).or_default();
        }
        if groups.is_empty() {
            groups.entry("未分组".to_string()).or_default();
        }

        for (group, entries) in groups {
            let collapsed = self.collapsed_groups.contains(&group);
            let group_key = group.clone();
            column = column.child(
                div()
                    .id(SharedString::from(format!("group-{group}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .mx_2()
                    .mt_1()
                    .h(px(40.0))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.button)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if !this.collapsed_groups.remove(&group_key) {
                            this.collapsed_groups.insert(group_key.clone());
                        }
                        cx.notify();
                    }))
                    .child(icons::icon(
                        if collapsed {
                            "shell/chevron-right.svg"
                        } else {
                            "shell/chevron-down.svg"
                        },
                        14.0,
                        theme.muted,
                    ))
                    .child(icons::icon("superlink/folder.svg", 18.0, theme.accent))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(SharedString::from(group.clone())),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child(format!("{}", entries.len())),
                    ),
            );
            if collapsed {
                continue;
            }
            for (index, profile) in entries {
                let active = index == self.selected;
                // 连接名没有长度上限，超长时截断，避免撑出侧栏。
                let name = SharedString::from(crate::text::shorten(&profile.name, 16));
                let expanded = self.expanded_conns.contains(&index);
                column = column.child(
                    div()
                        .id(("profile", index))
                        .flex()
                        .items_center()
                        .gap_2()
                        .mx_2()
                        .mt_1()
                        .h(px(40.0))
                        .pl_5()
                        .pr_2()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if active {
                            theme.accent_soft
                        } else {
                            theme.sidebar
                        })
                        .hover(move |style| style.bg(theme.accent_soft))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.selected = index;
                            this.workspace = Workspace::Sql;
                            this.sync_selection(cx);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .id(SharedString::from(format!("fold-conn-{index}")))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(18.0))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    cx.stop_propagation();
                                    this.toggle_conn(index, cx);
                                }))
                                .child(icons::icon(
                                    if expanded {
                                        "shell/chevron-down.svg"
                                    } else {
                                        "shell/chevron-right.svg"
                                    },
                                    13.0,
                                    theme.muted,
                                )),
                        )
                        .child(icons::icon(
                            icons::datasource_icon(profile.sql_dialect()),
                            20.0,
                            theme.muted,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(px(14.0))
                                .text_color(if active { theme.accent } else { theme.text })
                                .child(name),
                        )
                        .children(active.then(|| {
                            div()
                                .size(px(8.0))
                                .rounded_full()
                                .bg(theme.success)
                                .into_any_element()
                        })),
                );
                if expanded {
                    column = column.child(self.conn_subtree(index, cx));
                }
            }
        }

        column = column.child(div().flex_1()).child(
            div()
                .id("saved-queries")
                .flex()
                .items_center()
                .gap_2()
                .mx_2()
                .my_2()
                .h(px(36.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .hover(move |style| style.bg(theme.accent_soft))
                .on_click(cx.listener(|this, _event, window, cx| {
                    this.workspace = Workspace::Sql;
                    this.query_page
                        .update(cx, |page, cx| page.open_saved(window, cx));
                    cx.notify();
                }))
                .child(icons::icon("superlink/folder-open.svg", 16.0, theme.muted))
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.muted)
                        .child("全部已存查询"),
                ),
        );

        column
    }
    fn global_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        // 顶部工具栏：Fyne header 高 56，图标在上、标题 12 号
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(8.0))
            .h(px(HEADER_HEIGHT))
            .w_full()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .child(self.header_action(
                "global-new-query",
                "superlink/new-query.svg",
                "新建查询",
                cx,
                Self::new_query_action,
            ))
            .child(self.header_action(
                "global-new-connection",
                "superlink/link.svg",
                "新建连接",
                cx,
                Self::new_connection_action,
            ))
            .child(self.header_action(
                "global-groups",
                "superlink/all-objects.svg",
                "管理连接分组",
                cx,
                Self::open_groups_action,
            ))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(24.0))
                    .mx(px(6.0))
                    .bg(theme.border),
            )
            .child(self.header_action(
                "global-history",
                "superlink/history.svg",
                "执行历史",
                cx,
                Self::history_action,
            ))
            .into_any_element()
    }

    fn workspace_tab(
        &self,
        workspace: Workspace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.workspace == workspace;
        let theme = self.theme;
        let mut line = div()
            .pb(px(3.0))
            .border_b_2()
            .border_color(gpui::transparent_black())
            .text_size(px(14.0))
            .text_color(if active { theme.text } else { theme.muted })
            .child(workspace.label());
        if active {
            line = line.border_color(theme.accent);
        }
        div()
            .id(("workspace", workspace as usize))
            .flex()
            .items_center()
            .justify_center()
            .w(px(MODE_TAB_WIDTH))
            .h_full()
            .cursor_pointer()
            .child(line)
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.notice = None;
                this.workspace = workspace;
                cx.notify();
            }))
    }

    /// 工具栏按钮：图标在上、12 号标题在下，最小宽度 64（对齐 Fyne headerAction）。
    fn header_action(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .min_w(px(64.0))
            .h_full()
            .px(px(8.0))
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _event, window, cx| {
                handler(this, window, cx)
            }))
            .child(icons::icon(icon, HEADER_ICON, theme.text))
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(HEADER_CAPTION))
                    .text_color(theme.text)
                    .child(label),
            )
    }

    /// 标题栏右侧的系统图标按钮：32×30，无边框，悬停给一层浅底。
    fn utility_button(
        &self,
        id: &'static str,
        icon: &'static str,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .w(px(32.0))
            .h(px(30.0))
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _event, window, cx| {
                handler(this, window, cx)
            }))
            .child(icons::icon(icon, 16.0, theme.muted))
    }

    fn new_query_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.workspace = Workspace::Sql;
        self.query_page.update(cx, |page, cx| page.add_tab(cx));
        cx.notify();
    }

    fn new_connection_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.workspace = Workspace::Sql;
        self.query_page
            .update(cx, |page, cx| page.open_connection_form(window, cx));
        cx.notify();
    }

    fn open_groups_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_groups(cx);
    }

    fn history_action(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.workspace = Workspace::Sql;
        self.query_page
            .update(cx, |page, cx| page.toggle_history(cx));
        cx.notify();
    }

    fn open_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = None;
        self.settings_open = true;
        cx.notify();
    }

    fn open_assistant(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = Some(SharedString::from("AI 助手尚未接入原生工作台"));
        cx.notify();
    }
}

/// Tables whose structure is not cached yet, capped at "limit".
fn missing_structures(
    entries: &[superlink_drivers::TableEntry],
    cached: &HashSet<String>,
    limit: usize,
) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| !cached.contains(&entry.name))
        .take(limit)
        .map(|entry| entry.name.clone())
        .collect()
}

impl Render for SuperLinkApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut tabs = Vec::with_capacity(3);
        for workspace in [Workspace::Sql, Workspace::Shell, Workspace::Note] {
            tabs.push(self.workspace_tab(workspace, cx));
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .font_family("PingFang SC")
            // 根节点统一处理 Tab：焦点在任意输入框时都能前后切换。
            .on_key_down(cx.listener(|_this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    cx.stop_propagation();
                    if event.keystroke.modifiers.shift {
                        window.focus_prev();
                    } else {
                        window.focus_next();
                    }
                }
            }))
            .child(
                // 标题栏：对齐 Fyne 的原生标题栏（分段切换 + 右侧工具图标，无品牌文字）
                div()
                    .flex()
                    .items_center()
                    .h(px(TITLEBAR_HEIGHT))
                    .w_full()
                    .bg(theme.canvas)
                    .child(
                        div()
                            .id("titlebar-lead")
                            .w(px(72.0))
                            .h_full()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move()
                            }),
                    )
                    .children(tabs)
                    .child(
                        div()
                            .id("titlebar-drag")
                            .flex_1()
                            .h_full()
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move()
                            }),
                    )
                    .child(self.utility_button(
                        "theme-toggle",
                        if self.resolved_dark {
                            "superlink/sun.svg"
                        } else {
                            "superlink/moon.svg"
                        },
                        cx,
                        Self::cycle_theme,
                    ))
                    .child(self.utility_button(
                        "ai-entry",
                        "superlink/insight.svg",
                        cx,
                        Self::open_assistant,
                    ))
                    .child(self.utility_button(
                        "open-settings",
                        "superlink/settings.svg",
                        cx,
                        Self::open_settings,
                    ))
                    .child(div().w(px(8.0))),
            )
            .children(if self.workspace == Workspace::Sql {
                Some(self.global_toolbar(cx))
            } else {
                None
            })
            .child(if self.settings_open {
                self.settings_view(cx).into_any_element()
            } else if self.groups_open {
                self.groups_panel(cx).into_any_element()
            } else if self.workbench.is_some() {
                self.workbench_panel(cx)
            } else if self.structure_edit.is_some() {
                self.structure_panel(cx).into_any_element()
            } else if self.workspace == Workspace::Sql {
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .bg(theme.canvas)
                            .child(match self.workspace {
                                Workspace::Sql => self.query_page.clone().into_any_element(),
                                Workspace::Note => self.note_page.clone().into_any_element(),
                                Workspace::Shell => self.terminal_page.clone().into_any_element(),
                            }),
                    )
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(match self.workspace {
                        Workspace::Shell => self.terminal_page.clone().into_any_element(),
                        Workspace::Note => self.note_page.clone().into_any_element(),
                        Workspace::Sql => div().into_any_element(),
                    })
                    .into_any_element()
            })
            .children(if self.workspace == Workspace::Sql {
                Some(
                div()
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .w_full()
                    .px_3()
                    .bg(theme.surface)
                    .border_t_1()
                    .border_color(theme.border)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(format!("本地工作区 · {} 个连接", self.profiles.len()))
                    .child(div().w(px(12.0)))
                    .children(self.error.as_ref().map(|error| {
                        div()
                            .text_color(theme.danger)
                            .child(error.clone())
                            .into_any_element()
                    }))
                    .children(self.notice.as_ref().map(|notice| {
                        div()
                            .text_color(theme.muted)
                            .child(notice.clone())
                            .into_any_element()
                    })),
                )
            } else {
                None
            })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    service: Arc<ProfileService>,
    queries: Arc<QueryService>,
    drafts: Arc<DraftService>,
    history: Arc<HistoryService>,
    saved: Arc<SavedQueryService>,
    structure: Arc<StructureService>,
    groups: Arc<superlink_app::GroupService>,
    notebook: Arc<NotebookService>,
    hosts: Arc<HostService>,
    settings: Arc<SettingsService>,
) {
    Application::new()
        .with_assets(icons::Assets::discover())
        .run(move |cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: Some(gpui::TitlebarOptions {
                            title: Some("SuperLink".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(8.0), px(11.0))),
                        }),
                        focus: true,
                        ..Default::default()
                    },
                    |window, cx| {
                        let service = service.clone();
                        let queries = queries.clone();
                        let drafts = drafts.clone();
                        let history = history.clone();
                        let structure = structure.clone();
                        let groups = groups.clone();
                        let notebook = notebook.clone();
                        let hosts = hosts.clone();
                        let settings = settings.clone();
                        cx.new(|cx| {
                            SuperLinkApp::new(
                                service, queries, drafts, history, saved, structure, groups,
                                notebook, hosts, settings, window, cx,
                            )
                        })
                    },
                )
                .expect("failed to open the SuperLink window");
            let _ = handle;
            cx.activate(true);
        });
}

#[cfg(test)]
mod prefetch_tests {
    use super::*;
    use superlink_drivers::TableEntry;

    fn entries(names: &[&str]) -> Vec<TableEntry> {
        names
            .iter()
            .map(|name| TableEntry {
                name: (*name).to_string(),
                kind: "table".into(),
            })
            .collect()
    }

    #[test]
    fn skips_cached_structures() {
        let cached: HashSet<String> = ["b".to_string()].into_iter().collect();
        let pending = missing_structures(&entries(&["a", "b", "c"]), &cached, 10);
        assert_eq!(pending, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn caps_the_prefetch_batch() {
        let cached = HashSet::new();
        let pending = missing_structures(&entries(&["a", "b", "c"]), &cached, 2);
        assert_eq!(pending, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn empty_list_needs_no_work() {
        assert!(missing_structures(&[], &HashSet::new(), 10).is_empty());
    }

    #[test]
    fn everything_cached_needs_no_work() {
        let cached: HashSet<String> = ["a".to_string()].into_iter().collect();
        assert!(missing_structures(&entries(&["a"]), &cached, 10).is_empty());
    }
}
