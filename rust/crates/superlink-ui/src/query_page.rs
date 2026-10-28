//! SQL workspace: query tabs, toolbar, statement input and a result table.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px,
};
use serde_json::Value;
use superlink_app::{
    Credentials, DraftService, HistoryService, ProfileService, QueryService, SettingsService,
    placeholder_count,
};
use superlink_domain::{ConnectionConfig, Draft, History, Profile, ResultSet, catalog, resolve};

use crate::editor::SqlEditor;
use crate::icons;
use crate::theme::Theme;

const DISPLAY_ROWS: usize = 300;
const COLUMN_WIDTH: f32 = 160.0;
const ROW_NUMBER_WIDTH: f32 = 52.0;
const HISTORY_WIDTH: f32 = 300.0;

type SavedHandler = Box<dyn Fn(&str, &mut App) + 'static>;

struct QueryTab {
    id: String,
    title: String,
    text: String,
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

pub struct QueryPage {
    profiles: Arc<ProfileService>,
    queries: Arc<QueryService>,
    drafts: Arc<DraftService>,
    history: Arc<HistoryService>,
    settings: Arc<SettingsService>,
    theme: Theme,
    editor: Entity<SqlEditor>,
    profile: Option<Profile>,
    tabs: Vec<QueryTab>,
    active: usize,
    result: Option<ResultSet>,
    error: Option<SharedString>,
    status: SharedString,
    result_tab: ResultTab,
    last_sql: Option<String>,
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
    /// New-connection form.
    form_open: bool,
    /// Notifies the shell after a connection is saved so it can refresh.
    on_saved: Option<SavedHandler>,
    form_kind: String,
    form_name: Entity<SqlEditor>,
    form_host: Entity<SqlEditor>,
    form_port: Entity<SqlEditor>,
    form_user: Entity<SqlEditor>,
    form_database: Entity<SqlEditor>,
    form_password: Entity<SqlEditor>,
    form_error: Option<SharedString>,
}

impl QueryPage {
    pub fn new(
        profiles: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        history: Arc<HistoryService>,
        settings: Arc<SettingsService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let page = Self {
            profiles,
            queries,
            drafts,
            history,
            settings,
            theme,
            editor: cx.new(SqlEditor::new),
            profile: None,
            tabs: Vec::new(),
            active: 0,
            result: None,
            result_tab: ResultTab::default(),
            last_sql: None,
            last_duration: None,
            error: None,
            status: "选择连接后执行查询".into(),
            running: false,
            history_open: false,
            entries: Vec::new(),
            recent_sql: HashMap::new(),
            redacted_notice: None,
            confirm_run: false,
            form_open: false,
            on_saved: None,
            form_kind: "sqlite".into(),
            form_name: cx.new(SqlEditor::new),
            form_host: cx.new(SqlEditor::new),
            form_port: cx.new(SqlEditor::new),
            form_user: cx.new(SqlEditor::new),
            form_database: cx.new(SqlEditor::new),
            form_password: cx.new(SqlEditor::new),
            form_error: None,
        };
        // Cmd+R in the editor runs the statement, matching the toolbar label.
        let weak = cx.entity().downgrade();
        page.editor.update(cx, move |editor, cx| {
            editor.set_on_run(
                move |_window, app| {
                    let weak = weak.clone();
                    let _ = weak.update(app, |page, cx| page.run(cx));
                },
                cx,
            );
        });
        page
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn select_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        let supported = self.queries.supports(profile.sql_dialect());
        self.redacted_notice = None;
        self.confirm_run = false;
        self.tabs = self.load_tabs(&profile);
        self.active = 0;
        let text = self
            .tabs
            .first()
            .map(|tab| tab.text.clone())
            .unwrap_or_default();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        self.profile = Some(profile);
        self.result = None;
        self.error = None;
        self.status = if supported {
            "就绪 · 结果按行数上限截断".into()
        } else {
            "该数据源尚未提供 Rust 驱动 agent".into()
        };
        cx.notify();
    }

    fn load_tabs(&self, profile: &Profile) -> Vec<QueryTab> {
        let mut tabs: Vec<QueryTab> = self
            .drafts
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|draft| draft.profile_id == profile.id && !draft.closed)
            .map(|draft| QueryTab {
                id: draft.id,
                title: draft.title,
                text: draft.text,
            })
            .collect();
        if tabs.is_empty() {
            tabs.push(Self::new_tab(profile, 1));
        }
        tabs
    }

    fn new_tab(profile: &Profile, index: usize) -> QueryTab {
        QueryTab {
            id: format!("query-tab-{}-{index}", profile.id),
            title: format!("查询 {index}"),
            text: String::new(),
        }
    }

    fn stash(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_string();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.text = text;
        }
    }

    fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.active || index >= self.tabs.len() {
            return;
        }
        self.save_active(cx);
        self.active = index;
        self.redacted_notice = None;
        self.confirm_run = false;
        let text = self.tabs[index].text.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        cx.notify();
    }

    pub fn add_tab(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        self.save_active(cx);
        self.redacted_notice = None;
        self.confirm_run = false;
        let tab = Self::new_tab(&profile, self.tabs.len() + 1);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.editor
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.save_active(cx);
        if self.tabs.len() <= 1 {
            if let Some(tab) = self.tabs.get_mut(0) {
                tab.text.clear();
            }
            self.editor
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            cx.notify();
            return;
        }
        let tab = self.tabs.remove(index);
        let _ = self.drafts.delete(&tab.id);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        let text = self.tabs[self.active].text.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        cx.notify();
    }

    fn save_active(&mut self, cx: &mut Context<Self>) {
        self.stash(cx);
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let draft = Draft {
            id: tab.id.clone(),
            profile_id: profile.id,
            title: tab.title.clone(),
            text: tab.text.clone(),
            ..Default::default()
        };
        let _ = self.drafts.save(&draft);
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        self.run_impl(false, cx);
    }

    fn run_impl(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(profile) = self.profile.clone() else {
            self.error = Some("请先在左侧选择一个连接".into());
            cx.notify();
            return;
        };
        self.save_active(cx);
        let sql = self.editor.read(cx).text().to_string();
        if sql.trim().is_empty() {
            self.error = Some("请输入 SQL".into());
            cx.notify();
            return;
        }
        if !force
            && needs_execution_confirmation(self.redacted_notice.is_some(), placeholder_count(&sql))
        {
            self.confirm_run = true;
            cx.notify();
            return;
        }
        self.confirm_run = false;
        let credentials = self
            .profiles
            .credentials(&profile)
            .ok()
            .flatten()
            .unwrap_or_default();
        let limit = self.settings.row_limit().max(100);
        let queries = self.queries.clone();
        let history = self.history.clone();
        let profile_id = profile.id.clone();
        let recorded = sql.clone();
        self.running = true;
        self.error = None;
        self.status = "执行中…".into();
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { queries.execute(&profile, &credentials, &sql, limit) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let entry_id = match &outcome {
                Ok(result) => history.record(
                    &profile_id,
                    &recorded,
                    true,
                    result.rows.len() as i64,
                    result.duration_ns,
                ),
                Err(_) => history.record(&profile_id, &recorded, false, 0, 0),
            };
            let original = recorded.clone();
            this.update(cx, |this, cx| {
                if let Some(id) = entry_id {
                    if this.recent_sql.len() >= 50
                        && let Some(oldest) = this.recent_sql.keys().min().copied()
                    {
                        this.recent_sql.remove(&oldest);
                    }
                    this.recent_sql.insert(id, original.clone());
                }
                this.running = false;
                this.last_sql = Some(original.clone());
                if outcome.is_err() {
                    this.last_duration = None;
                }
                match outcome {
                    Ok(result) => {
                        this.last_duration = Some(result.duration_ns);
                        // A successful run means the literals were supplied.
                        this.redacted_notice = None;
                        this.status = format!(
                            "{} 行 · {} 列{} · {:.1} ms",
                            result.rows.len(),
                            result.columns.len(),
                            if result.truncated {
                                " · 已截断"
                            } else {
                                ""
                            },
                            result.duration_ns as f64 / 1_000_000.0
                        )
                        .into();
                        this.result = Some(result);
                    }
                    Err(err) => {
                        this.status = "执行失败".into();
                        this.error = Some(err.to_string().into());
                    }
                }
                this.editor
                    .update(cx, |editor, cx| editor.set_enabled(true, cx));
                if this.history_open {
                    this.reload_history();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.editor
            .update(cx, |editor, cx| editor.set_enabled(false, cx));
    }

    fn cancel_execution(&mut self, cx: &mut Context<Self>) {
        self.confirm_run = false;
        cx.notify();
    }

    /// Re-reads the editor so edits made while the prompt was open are kept.
    fn confirm_execution(&mut self, cx: &mut Context<Self>) {
        self.run_impl(true, cx);
    }

    fn reload_history(&mut self) {
        self.entries = self
            .profile
            .as_ref()
            .map(|profile| self.history.list(&profile.id))
            .unwrap_or_default();
    }

    /// Clicking a row restores this session's exact statement, or the redacted
    /// template for entries recorded before this window opened.
    fn load_history(&mut self, id: i64, redacted: String, cx: &mut Context<Self>) {
        let from_session = self.recent_sql.get(&id).cloned();
        let placeholders = placeholder_count(&redacted);
        let (text, note) = resolve_history_text(from_session.clone(), redacted);
        self.redacted_notice = from_session.is_none().then_some(placeholders);
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        self.status = note.into();
        cx.notify();
    }

    fn redacted_banner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let count = self.redacted_notice.unwrap_or(0);
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(theme.warning_bg)
            .border_b_1()
            .border_l_2()
            .border_color(theme.warning_border)
            .child(icons::icon(
                "shell/circle-help.svg",
                15.0,
                theme.warning_text,
            ))
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.warning_text)
                    .child(format!(
                        "该语句来自历史记录，{count} 处字面量已被替换为 ?，执行前请手动补值"
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("dismiss-redacted")
                    .px_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.warning_border))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.redacted_notice = None;
                        cx.notify();
                    }))
                    .child(icons::icon("shell/x.svg", 12.0, theme.warning_text)),
            )
    }

    fn confirm_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let placeholders = placeholder_count(self.editor.read(cx).text());
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(theme.warning_bg)
            .border_b_1()
            .border_l_2()
            .border_color(theme.warning_border)
            .child(icons::icon(
                "shell/circle-help.svg",
                15.0,
                theme.warning_text,
            ))
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.warning_text)
                    .child(format!(
                        "语句仍有 {placeholders} 处 ? 占位符，直接执行大概率失败。仍要执行吗？"
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("cancel-run")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.warning_border)
                    .on_click(cx.listener(|this, _event, _window, cx| this.cancel_execution(cx)))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.warning_text)
                            .child("取消"),
                    ),
            )
            .child(
                div()
                    .id("confirm-run")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.warning_text)
                    .on_click(cx.listener(|this, _event, _window, cx| this.confirm_execution(cx)))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.surface)
                            .child("仍然执行"),
                    ),
            )
    }

    pub fn toggle_history(&mut self, cx: &mut Context<Self>) {
        self.history_open = !self.history_open;
        if self.history_open {
            self.reload_history();
        }
        cx.notify();
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        if let Some(profile) = self.profile.clone() {
            self.history.clear(&profile.id);
        }
        self.entries.clear();
        cx.notify();
    }

    fn history_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut panel = div()
            .flex()
            .flex_col()
            .w(px(HISTORY_WIDTH))
            .h_full()
            .bg(theme.sidebar)
            .border_l_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .h(px(36.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(icons::icon("superlink/history.svg", 13.0, theme.muted))
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("执行历史"),
                            ),
                    )
                    .child(
                        div()
                            .id("clear-history")
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.clear_history(cx)),
                            )
                            .child(icons::icon("superlink/trash.svg", 13.0, theme.muted)),
                    ),
            );

        if self.entries.is_empty() {
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("还没有执行记录"),
            );
        }

        for entry in &self.entries {
            let theme = self.theme;
            let time = entry
                .created_at
                .map(|value| {
                    value
                        .with_timezone(&chrono::Local)
                        .format("%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();
            let line = entry.text.lines().next().unwrap_or("");
            let text = if line.chars().count() > 48 {
                format!("{}…", line.chars().take(48).collect::<String>())
            } else {
                line.to_string()
            };
            let entry_id = entry.id;
            let redacted = entry.text.clone();
            panel = panel.child(
                div()
                    .id(SharedString::from(format!("history-{}", entry.id)))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mx_2()
                    .my_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.load_history(entry_id, redacted.clone(), cx)
                    }))
                    .bg(theme.surface)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(if entry.success {
                                        theme.accent
                                    } else {
                                        theme.danger
                                    })
                                    .child(if entry.success { "成功" } else { "失败" }),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(time),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(text)),
                    ),
            );
        }
        panel
    }

    pub fn set_on_saved(
        &mut self,
        handler: impl Fn(&str, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_saved = Some(Box::new(handler));
    }

    /// 从结构树点选表后回填查询。
    pub fn load_sql(&mut self, sql: String, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_text(sql, cx));
        self.redacted_notice = None;
        self.confirm_run = false;
        self.status = "已回填表查询".into();
        cx.notify();
    }

    pub fn open_connection_form(&mut self, cx: &mut Context<Self>) {
        self.form_open = true;
        self.form_kind = "sqlite".into();
        self.form_error = None;
        for editor in [
            &self.form_name,
            &self.form_host,
            &self.form_port,
            &self.form_user,
            &self.form_database,
            &self.form_password,
        ] {
            editor.update(cx, |editor, cx| {
                editor.set_text(String::new(), cx);
                editor.set_masked(false, cx);
            });
        }
        self.form_name
            .update(cx, |editor, cx| editor.set_placeholder("例如 生产库", cx));
        self.form_host.update(cx, |editor, cx| {
            editor.set_placeholder("例如 127.0.0.1", cx)
        });
        self.form_user
            .update(cx, |editor, cx| editor.set_placeholder("例如 root", cx));
        self.form_database
            .update(cx, |editor, cx| editor.set_placeholder("例如 mydb", cx));
        self.form_password
            .update(cx, |editor, cx| editor.set_placeholder("密码", cx));
        if let Ok(descriptor) = resolve("sqlite") {
            let port = descriptor
                .port
                .map(|value| value.to_string())
                .unwrap_or_else(|| "例如 3306".into());
            self.form_port
                .update(cx, |editor, cx| editor.set_text(port, cx));
        }
        cx.notify();
    }

    pub fn close_connection_form(&mut self, cx: &mut Context<Self>) {
        self.form_open = false;
        cx.notify();
    }

    pub fn set_form_kind(&mut self, kind: String, cx: &mut Context<Self>) {
        self.form_kind = kind.clone();
        if let Ok(descriptor) = resolve(&kind)
            && let Some(port) = descriptor.port
        {
            self.form_port
                .update(cx, |editor, cx| editor.set_text(port.to_string(), cx));
        }
        cx.notify();
    }

    fn editor_text(entity: &Entity<SqlEditor>, cx: &Context<Self>) -> String {
        entity.read(cx).text().to_string()
    }

    pub fn save_connection(&mut self, cx: &mut Context<Self>) {
        self.form_error = None;
        let name = Self::editor_text(&self.form_name, cx);
        let host = Self::editor_text(&self.form_host, cx);
        let port_text = Self::editor_text(&self.form_port, cx);
        let user = Self::editor_text(&self.form_user, cx);
        let database = Self::editor_text(&self.form_database, cx);
        let password = Self::editor_text(&self.form_password, cx);

        if name.trim().is_empty() {
            self.form_error = Some("请输入连接名称".into());
            cx.notify();
            return;
        }
        let port: u16 = if port_text.trim().is_empty() {
            0
        } else {
            match port_text.trim().parse() {
                Ok(value) => value,
                Err(_) => {
                    self.form_error = Some("端口无效，请输入 1-65535".into());
                    cx.notify();
                    return;
                }
            }
        };
        let profile = Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            config: ConnectionConfig {
                kind: self.form_kind.clone(),
                host: host.trim().to_string(),
                port,
                user: user.trim().to_string(),
                database: database.trim().to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let credentials = Credentials {
            password: password.trim().to_string(),
            ..Default::default()
        };
        match self.profiles.save(profile, &credentials) {
            Ok(saved) => {
                self.form_open = false;
                self.status = format!("已保存连接「{}」", saved.name).into();
                if let Some(handler) = self.on_saved.as_ref() {
                    handler(&saved.id, cx);
                }
            }
            Err(err) => self.form_error = Some(err.to_string().into()),
        }
        cx.notify();
    }

    fn form_row(&self, label: &str, editor: &Entity<SqlEditor>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(72.0))
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .px_2()
                    .h(px(30.0))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.canvas)
                    .text_color(theme.text)
                    .child(editor.clone()),
            )
    }

    fn connection_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let kind = self.form_kind.clone();
        let chips = catalog()
            .iter()
            .map(|descriptor| {
                let key = descriptor.key;
                let active = key == kind;
                div()
                    .id(SharedString::from(format!("kind-{key}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.accent } else { theme.border })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_form_kind(key.to_string(), cx)
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(descriptor.name),
                    )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_1()
            .items_start()
            .justify_center()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_6()
                    .mt_6()
                    .w(px(640.0))
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.text)
                                    .child("新建连接"),
                            )
                            .child(
                                div()
                                    .id("close-form")
                                    .px_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.close_connection_form(cx)
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .max_w(px(580.0))
                            .children(chips),
                    )
                    .child(self.form_row("连接名称", &self.form_name))
                    .child(self.form_row("主机", &self.form_host))
                    .child(self.form_row("端口", &self.form_port))
                    .child(self.form_row("用户名", &self.form_user))
                    .child(self.form_row("数据库", &self.form_database))
                    .child(self.form_row("密码", &self.form_password))
                    .children(self.form_error.clone().map(|error| {
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.danger)
                            .child(error)
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .id("cancel-connection")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.close_connection_form(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child("取消"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("save-connection")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.save_connection(cx)
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.surface)
                                            .child("保存"),
                                    ),
                            ),
                    ),
            )
    }

    fn tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut strip = div()
            .flex()
            .items_center()
            .h(px(34.0))
            .px_2()
            .gap_1()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border);
        for (index, tab) in self.tabs.iter().enumerate() {
            let active = index == self.active;
            let title = SharedString::from(tab.title.clone());
            strip = strip.child(
                div()
                    .id(("tab", index))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(26.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.surface } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.border } else { theme.canvas })
                    .on_click(
                        cx.listener(move |this, _event, _window, cx| this.switch_tab(index, cx)),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.text } else { theme.muted })
                            .child(title),
                    )
                    .child(
                        div()
                            .id(("close-tab", index))
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.close_tab(index, cx)
                            }))
                            .child(icons::icon("shell/x.svg", 11.0, theme.muted)),
                    ),
            );
        }
        strip.child(
            div()
                .id("new-tab")
                .px_2()
                .h(px(26.0))
                .flex()
                .items_center()
                .rounded_md()
                .cursor_pointer()
                .hover(move |style| style.bg(theme.accent_soft))
                .on_click(cx.listener(|this, _event, _window, cx| this.add_tab(cx)))
                .child(icons::icon("shell/plus.svg", 13.0, theme.muted)),
        )
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let profile_label = self
            .profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| "未选择连接".to_string());
        let profile_kind = self
            .profile
            .as_ref()
            .map(|profile| profile.sql_dialect().to_string())
            .unwrap_or_default();
        let database = self
            .profile
            .as_ref()
            .map(|profile| {
                if profile.config.database.is_empty() {
                    "数据库 / Namespace".to_string()
                } else {
                    profile.config.database.clone()
                }
            })
            .unwrap_or_else(|| "数据库 / Namespace".to_string());

        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(42.0))
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("new-query")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.add_tab(cx)))
                    .child(icons::icon("superlink/new-query.svg", 14.0, theme.muted))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("新建查询"),
                    ),
            )
            .child(
                div()
                    .id("save-query")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.save_active(cx);
                        this.status = "已保存到草稿".into();
                        cx.notify();
                    }))
                    .child(icons::icon("shell/save.svg", 14.0, theme.muted))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("保存"),
                    ),
            )
            .child(
                div()
                    .id("history")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if self.history_open {
                        theme.accent_soft
                    } else {
                        theme.surface
                    })
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.toggle_history(cx)))
                    .child(icons::icon(
                        "superlink/history.svg",
                        14.0,
                        if self.history_open {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if self.history_open {
                                theme.accent
                            } else {
                                theme.muted
                            })
                            .child("执行历史"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(icons::icon(
                        icons::datasource_icon(profile_kind.as_str()),
                        14.0,
                        theme.muted,
                    ))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(profile_label)),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(SharedString::from(database)),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(format!("{}", self.settings.row_limit())),
            )
            .child(
                div()
                    .id("run-query")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if self.running {
                        theme.border
                    } else {
                        theme.accent
                    })
                    .on_click(cx.listener(|this, _event, _window, cx| this.run(cx)))
                    .child(icons::icon("superlink/send.svg", 13.0, theme.surface))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.surface)
                            .child(if self.running {
                                "执行中"
                            } else {
                                "运行 ⌘R"
                            }),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(self.status.clone()),
            )
    }

    fn editor_panel(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .gap_1()
            .h(px(220.0))
            .border_b_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .child(self.editor.clone())
    }

    fn result_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .h(px(32.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .children(ResultTab::ALL.iter().map(|tab| {
                        let active = *tab == self.result_tab;
                        div()
                            .id(SharedString::from(format!("result-tab-{:?}", tab)))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.accent } else { theme.muted })
                            .font_weight(if active {
                                FontWeight::BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .border_b_2()
                            .border_color(if active { theme.accent } else { theme.surface })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.result_tab = *tab;
                                cx.notify();
                            }))
                            .child(tab.label())
                    })),
            );

        let content: AnyElement = match self.result_tab {
            ResultTab::Log => {
                let mut log = div().flex().flex_col().gap_2().p_3();
                match (&self.last_sql, &self.error) {
                    (Some(sql), error) => {
                        let status_text: String = if let Some(err) = error {
                            err.to_string()
                        } else {
                            format!(
                                "成功 · {:.1} ms",
                                self.last_duration
                                    .map(|ns| ns as f64 / 1_000_000.0)
                                    .unwrap_or_default()
                            )
                        };
                        log = log.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.muted)
                                        .child("状态"),
                                )
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(if error.is_some() {
                                            theme.danger
                                        } else {
                                            theme.text
                                        })
                                        .child(SharedString::from(status_text)),
                                ),
                        );
                        log = log.child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("语句"),
                        );
                        log = log.child(
                            div()
                                .p_2()
                                .rounded_md()
                                .bg(theme.canvas)
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(SharedString::from(sql.clone())),
                        );
                    }
                    _ => {
                        log = log.child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("还没有执行记录"),
                        );
                    }
                }
                log.into_any_element()
            }
            ResultTab::Params => div()
                .p_3()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child("当前查询未使用绑定参数；参数面板随后续参数支持开放。")
                .into_any_element(),
            ResultTab::Fields => {
                let Some(result) = self.result.as_ref() else {
                    return panel
                        .child(
                            div()
                                .flex_1()
                                .p_3()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("执行查询后显示字段"),
                        )
                        .into_any_element();
                };
                let mut fields = div().flex().flex_col().gap_1().p_3();
                for (position, column) in result.columns.iter().enumerate() {
                    fields = fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(28.0))
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(format!("{}", position + 1)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(SharedString::from(column.name.clone())),
                            ),
                    );
                }
                fields.into_any_element()
            }
            ResultTab::Ddl => {
                let Some(sql) = self.last_sql.as_ref() else {
                    return panel
                        .child(
                            div()
                                .flex_1()
                                .p_3()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("执行查询后显示语句"),
                        )
                        .into_any_element();
                };
                div()
                    .p_3()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(SharedString::from(sql.clone()))
                    .into_any_element()
            }
            ResultTab::Result => {
                if let Some(error) = self.error.clone() {
                    return panel
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.danger)
                                .child(error),
                        )
                        .into_any_element();
                }
                let Some(result) = self.result.as_ref() else {
                    return panel
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child("执行查询后显示结果"),
                        )
                        .into_any_element();
                };

                let header = div()
                    .flex()
                    .items_center()
                    .h(px(crate::theme::CONTROL_MIN))
                    .bg(theme.canvas)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .w(px(ROW_NUMBER_WIDTH))
                            .px_2()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child("#"),
                    )
                    .children(result.columns.iter().map(|column| {
                        div()
                            .w(px(COLUMN_WIDTH))
                            .px_2()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.muted)
                            .child(SharedString::from(column.name.clone()))
                    }));

                let rows = result
                    .rows
                    .iter()
                    .take(DISPLAY_ROWS)
                    .enumerate()
                    .map(|(index, row)| {
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
                                    .w(px(ROW_NUMBER_WIDTH))
                                    .px_2()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child(format!("{}", index + 1)),
                            )
                            .children(row.iter().map(|cell| {
                                div()
                                    .w(px(COLUMN_WIDTH))
                                    .px_2()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.text)
                                    .child(cell_text(cell))
                            }))
                    });

                panel
                    .child(header)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h(px(0.0))
                            .children(rows),
                    )
                    .into_any_element()
            }
        };
        content
    }
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

fn cell_text(value: &Value) -> SharedString {
    match value {
        Value::Null => "NULL".into(),
        Value::String(text) => text.clone().into(),
        Value::Bool(flag) => flag.to_string().into(),
        Value::Number(number) => number.to_string().into(),
        other => other.to_string().into(),
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
            .child(if self.form_open {
                self.connection_form(cx).into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(self.tab_strip(cx))
                    .child(self.toolbar(cx))
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
                                    .child(self.result_panel(cx)),
                            )
                            .children(self.history_open.then(|| self.history_panel(cx))),
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
