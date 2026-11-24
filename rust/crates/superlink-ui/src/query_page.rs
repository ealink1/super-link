//! SQL workspace: query tabs, toolbar, statement input and a result table.

mod part_new;
mod part_saved_panel;
mod part_toolbar;
mod workspace_tabs;

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px,
};
use serde_json::Value;
use superlink_app::{
    DraftService, HistoryService, ProfileService, QueryService, SavedQueryService, SettingsService,
    placeholder_count,
};
use superlink_domain::{Draft, History, Profile, ResultSet, SavedQuery};

use crate::connection_form::ConnectionForm;
use crate::editor::SqlEditor;
use crate::icons;
use crate::multiline_editor::SqlTextEditor;
use crate::theme::Theme;
use superlink_sql::CompletionTable;

mod controls;
mod execution;
mod exporting;
mod grid;
mod library;
mod protocols;

const DISPLAY_ROWS: usize = 300;
pub(crate) const COLUMN_WIDTH: f32 = 160.0;
pub(crate) const ROW_NUMBER_WIDTH: f32 = 52.0;
const HISTORY_WIDTH: f32 = 300.0;
const SAVED_WIDTH: f32 = 320.0;

struct QueryTab {
    id: String,
    title: String,
    text: String,
    output: Option<ResultSet>,
    other_results: Vec<ResultSet>,
    selected_result: usize,
    error: Option<SharedString>,
    last_sql: Option<String>,
    last_request: Option<superlink_domain::Execution>,
    last_duration: Option<u64>,
    parameters: String,
    scope: String,
    schema: String,
}

/// 结果区分区标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultTab {
    Log,
    Params,
    #[default]
    Result,
    Fields,
    Ddl,
}

impl ResultTab {
    pub fn label(self) -> &'static str {
        match self {
            ResultTab::Log => "日志",
            ResultTab::Params => "参数",
            ResultTab::Result => "结果",
            ResultTab::Fields => "字段",
            ResultTab::Ddl => "DDL",
        }
    }

    pub const ALL: [ResultTab; 5] = [
        ResultTab::Log,
        ResultTab::Params,
        ResultTab::Result,
        ResultTab::Fields,
        ResultTab::Ddl,
    ];
}

pub struct QueryProfileChanged(pub String);
impl gpui::EventEmitter<QueryProfileChanged> for QueryPage {}

pub struct QueryPage {
    protocol: protocols::ProtocolControls,
    export_operation: Option<superlink_app::QueryCancel>,
    export_open: bool,
    other_results: Vec<ResultSet>,
    selected_result: usize,
    operation: Option<superlink_app::QueryCancel>,
    request_generation: u64,
    draft_generation: u64,
    library_generation: u64,
    library_busy: bool,
    profile_choices: Vec<Profile>,
    parameters: Entity<SqlEditor>,
    scope_input: Entity<SqlEditor>,
    schema_input: Entity<SqlEditor>,
    scope_open: bool,
    profiles_open: bool,
    scope_options: Vec<String>,
    scope: String,
    schema: String,
    editor_height: f32,
    resizing: Option<(gpui::Pixels, f32)>,
    display_page: usize,
    column_page: usize,
    column_widths: HashMap<usize, f32>,
    column_drag: Option<(usize, gpui::Pixels, f32)>,
    inspected: Option<(usize, usize)>,
    profiles: Arc<ProfileService>,
    queries: Arc<QueryService>,
    drafts: Arc<DraftService>,
    history: Arc<HistoryService>,
    settings: Arc<SettingsService>,
    theme: Theme,
    editor: Entity<SqlTextEditor>,
    profile: Option<Profile>,
    tab_cache: HashMap<String, (Vec<QueryTab>, usize)>,
    tabs: Vec<QueryTab>,
    active: usize,
    result: Option<ResultSet>,
    error: Option<SharedString>,
    status: SharedString,
    result_tab: ResultTab,
    last_sql: Option<String>,
    last_request: Option<superlink_domain::Execution>,
    last_duration: Option<u64>,
    running: bool,
    history_open: bool,
    entries: Vec<History>,
    /// Unredacted statements from this session, keyed by history row id.
    recent_sql: HashMap<i64, String>,
    /// Some(count) while a redacted statement is loaded and needs literals.
    redacted_notice: Option<usize>,
    /// Waiting for the user to confirm running a statement with placeholders.
    confirm_run: bool,
    /// 新建 / 编辑连接表单。
    connection_form: Entity<ConnectionForm>,
    /// Saved query library panel.
    saved: Arc<SavedQueryService>,
    saved_open: bool,
    saved_context: Option<(String, String)>,
    saved_items: Vec<SavedQuery>,
    saved_title: Entity<SqlEditor>,
    /// Inline rename: (entry id, title editor).
    saved_renaming: Option<(String, Entity<SqlEditor>)>,
    saved_note: Option<SharedString>,
}

/// Only a redacted statement that still has placeholders needs a confirmation.
fn needs_execution_confirmation(redacted: bool, placeholders: usize) -> bool {
    redacted && placeholders > 0
}

/// Entries recorded in this window keep their exact text; older ones only have
/// the redacted copy that was persisted.
fn resolve_history_text(recent: Option<String>, redacted: String) -> (String, &'static str) {
    match recent {
        Some(text) => (text, "已回填上次执行的 SQL"),
        None => (redacted, "已回填历史语句（字面量已脱敏）"),
    }
}

pub(crate) fn cell_text(value: &Value) -> SharedString {
    match value {
        Value::Null => "NULL".into(),
        Value::String(text) => text.clone().into(),
        Value::Bool(flag) => flag.to_string().into(),
        Value::Number(number) => number.to_string().into(),
        other => other.to_string().into(),
    }
}

impl QueryPage {
    pub(crate) fn draft_service(&self) -> Arc<DraftService> {
        self.drafts.clone()
    }
    pub(crate) fn prepare_quit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<superlink_app::QueryCancel> {
        let mut cancels = Vec::new();
        if let Some(cancel) = self.operation.clone() {
            cancels.push(cancel);
        }
        if let Some(cancel) = self.export_operation.take() {
            cancels.push(cancel);
        }
        self.save_active(cx);
        self.stop_query(cx);
        cancels
    }
    pub(crate) fn save_document(&mut self, cx: &mut Context<Self>) {
        self.save_active(cx);
    }
}

impl Render for QueryPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.surface)
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if let Some((column, origin, width)) = this.column_drag {
                        this.column_widths.insert(
                            column,
                            (width + f32::from(event.position.x - origin)).clamp(64., 800.),
                        );
                        cx.notify();
                    }
                    if let Some((origin, height)) = this.resizing {
                        this.editor_height = (height + f32::from(event.position.y - origin)).clamp(
                            100.0,
                            (f32::from(window.viewport_size().height) - 220.0).max(100.0),
                        );
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.resizing = None;
                    this.column_drag = None;
                }),
            )
            .child(if self.connection_form.read(cx).is_open() {
                div()
                    .flex()
                    .flex_1()
                    .items_start()
                    .justify_center()
                    .bg(theme.canvas)
                    .child(self.connection_form.clone())
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(self.toolbar(cx))
                    .children(self.profiles_open.then(|| self.profile_options(cx)))
                    .children(self.scope_open.then(|| self.scope_controls(cx)))
                    .children(self.protocol_controls(cx))
                    // The confirmation replaces the notice: two stacked amber bars are noise.
                    .children(
                        (!self.confirm_run)
                            .then(|| self.redacted_notice.map(|_| self.redacted_banner(cx)))
                            .flatten(),
                    )
                    .children(self.confirm_run.then(|| self.confirm_bar(cx)))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h(px(0.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(self.editor_panel())
                                    .child(
                                        div()
                                            .id("query-divider")
                                            .h(px(8.0))
                                            .cursor(gpui::CursorStyle::ResizeUpDown)
                                            .border_b_1()
                                            .border_color(theme.border)
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(
                                                    |this, event: &gpui::MouseDownEvent, _, cx| {
                                                        this.resizing = Some((
                                                            event.position.y,
                                                            this.editor_height,
                                                        ));
                                                        cx.notify();
                                                    },
                                                ),
                                            ),
                                    )
                                    .child(self.status_row())
                                    .child(self.result_panel(cx)),
                            )
                            .children(self.history_open.then(|| self.history_panel(cx)))
                            .children(self.saved_open.then(|| self.saved_panel(cx))),
                    )
                    .into_any_element()
            })
    }
}

#[cfg(test)]
mod history_tests {
    use super::resolve_history_text;

    #[test]
    fn prefers_the_unredacted_text_from_this_session() {
        let (text, note) = resolve_history_text(Some("SELECT 'alice'".into()), "SELECT ?".into());
        assert_eq!(text, "SELECT 'alice'");
        assert!(note.contains("上次执行"));
    }

    #[test]
    fn falls_back_to_the_redacted_copy() {
        let (text, note) = resolve_history_text(None, "SELECT ?".into());
        assert_eq!(text, "SELECT ?");
        assert!(note.contains("脱敏"));
    }
}

#[cfg(test)]
mod confirm_tests {
    use super::needs_execution_confirmation;

    #[test]
    fn only_redacted_statements_with_placeholders_are_guarded() {
        assert!(needs_execution_confirmation(true, 2));
        assert!(!needs_execution_confirmation(false, 2));
        assert!(!needs_execution_confirmation(true, 0));
    }
}
