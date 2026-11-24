mod field_designer;
mod object_preview;
mod object_tree;
mod part_groups_panel;
mod part_new;
mod part_refresh_structure;
mod part_sidebar;
mod part_structure_panel;
mod part_table_block;
mod part_toggle_table;
mod part_workbench_panel;
mod query_tabs;
mod sql_workspace;
mod tab_menu;
mod tab_overflow;
mod table_counts;
mod table_sections;
mod tree_style;
mod workbench_controls;
mod workbench_grid;
mod workbench_header_actions;
mod workbench_scrollbar;
mod workbench_structure;
mod workbench_tooltip;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use gpui::{
    AnyElement, App, Application, Bounds, Context, Entity, FontWeight, IntoElement, MouseButton,
    SharedString, Window, WindowBounds, WindowOptions, div, point, prelude::*, px, size,
};
use superlink_app::{
    Credentials, DraftService, HistoryService, HostService, NotebookService, ProfileService,
    QueryService, SavedQueryService, SettingsService, StructureService,
};
use superlink_domain::Profile;

use crate::editor::SqlEditor;
mod data_edit;
mod desktop;
mod driver_ui;
mod files;
mod group_actions;
mod metadata;
mod object_actions;
mod object_queries;
use metadata::read_credentials;
mod update_ui;
use crate::icons;
use crate::note_page::NotePage;
use crate::query_page::QueryPage;
use crate::shell_workspace::ShellWorkspace;
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
    stale: bool,
    busy: bool,
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
    drivers: driver_ui::DriverUi,
    updates: update_ui::UpdateUi,
    diagnostics: Vec<(String, String)>,
    designer: Option<object_actions::Designer>,
    connection_actions: Option<usize>,
    connection_delete_confirm: bool,
    sidebar_width: f32,
    sidebar_drag: Option<(gpui::Pixels, f32)>,
    chat_panel: Entity<crate::chat_panel::ChatPanel>,
    chat_open: bool,
    quitting: bool,
    service: Arc<ProfileService>,
    query_page: Entity<QueryPage>,
    note_page: Entity<NotePage>,
    terminal_page: Entity<ShellWorkspace>,
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
    expanded_tables: HashSet<(usize, String, String)>,
    table_counts: HashMap<(usize, String, String), i64>,
    table_structures: HashMap<(usize, String, String), superlink_drivers::TableStructure>,
    column_errors: HashMap<(usize, String, String), SharedString>,
    structure: Arc<StructureService>,
    groups: Arc<superlink_app::GroupService>,
    groups_open: bool,
    group_busy: bool,
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
    object_preview: Option<(usize, String)>,
    last_object_scope: Option<(usize, String)>,
    sql_tab_menu: Option<tab_menu::TabMenu>,
    tab_overflow_position: Option<gpui::Point<gpui::Pixels>>,
    workbench: Option<crate::workbench::Workbench>,
    parked_workbenches: Vec<crate::workbench::Workbench>,
    table_tab_order: Vec<String>,
    collapsed_object_groups: HashSet<String>,
    workbench_scroll_drag: Option<workbench_scrollbar::ScrollbarDrag>,
    wb_column_drag: Option<(usize, gpui::Pixels, f32)>,
    wb_column: Entity<SqlEditor>,
    wb_value: Entity<SqlEditor>,
    wb_import: Entity<SqlEditor>,
    wb_editor: Entity<SqlEditor>,
    workspace: Workspace,
    theme: Theme,
    resolved_dark: bool,
    notice: Option<SharedString>,
    error: Option<SharedString>,
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
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .font_family("PingFang SC")
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                this.drag_workbench_scrollbar(event, cx);
                if let Some((column, origin, width)) = this.wb_column_drag
                    && let Some(bench) = this.workbench.as_mut()
                {
                    bench.column_widths.insert(
                        column,
                        (width + f32::from(event.position.x - origin)).clamp(80., 640.),
                    );
                    cx.notify();
                }
                if let Some((origin, width)) = this.sidebar_drag {
                    this.sidebar_width =
                        (width + f32::from(event.position.x - origin)).clamp(200., 480.);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.wb_column_drag = None;
                    this.workbench_scroll_drag = None;
                    if this.sidebar_drag.take().is_some() {
                        this.settings
                            .set("sql.sidebarWidth", serde_json::json!(this.sidebar_width));
                        cx.notify();
                    }
                }),
            )
            // 根节点统一处理 Tab：焦点在任意输入框时都能前后切换。
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && (this.sql_tab_menu.take().is_some()
                        | this.tab_overflow_position.take().is_some())
                {
                    cx.stop_propagation();
                    cx.notify();
                } else if event.keystroke.key == "tab" {
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
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(if self.designer.is_some() {
                        self.designer_panel(cx)
                    } else if self.settings_open {
                        self.settings_view(cx).into_any_element()
                    } else if self.groups_open {
                        self.groups_panel(cx).into_any_element()
                    } else if self.workspace == Workspace::Sql {
                        self.sql_workspace(cx)
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
                    .children(self.chat_open.then(|| self.chat_panel.clone())),
            )
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
            .children(self.sql_tab_menu_panel(cx))
            .children(self.tab_overflow_panel(cx))
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
    let application = Application::new().with_assets(icons::Assets::discover());
    application.on_reopen(|cx| {
        for window in cx.windows() {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        cx.activate(true);
    });
    application.run(move |cx: &mut App| {
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
                            service, queries, drafts, history, saved, structure, groups, notebook,
                            hosts, settings, window, cx,
                        )
                    })
                },
            )
            .expect("failed to open the SuperLink window");
        desktop::register(handle, cx);
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
