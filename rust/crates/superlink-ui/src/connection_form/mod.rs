//! 连接表单：新建与编辑共用。
//!
//! 分区对齐 Fyne 版：`基本`｜`网络与安全`｜`外观`｜`高级`。新建时先进
//! 数据源选择器（`picker`），选定类型后以「连接串优先」的表单填写。

use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, Entity, Focusable, FontWeight, IntoElement, Render, SharedString,
    Window, div, prelude::*, px,
};
use superlink_app::{Credentials, ProfileService, QueryService, SettingsService};
use superlink_domain::{Profile, catalog, resolve};

mod actions;
mod lifecycle;
mod model;
mod network;
mod picker;
mod picker_view;
mod sections;
mod uri;
mod view;
mod widgets;

use model::{FormValues, overlay_extra, uri_takes_over};
use picker::SourcePicker;

use crate::editor::SqlEditor;
use crate::icons;
use crate::multiline_editor::SqlTextEditor;
use crate::theme::{TEXT_SIZE, Theme};

type SavedHandler = Box<dyn Fn(&str, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Basic,
    Advanced,
    Database,
    Ssl,
    Ssh,
    Proxy,
    Appearance,
}

impl Section {
    pub(super) fn label(self) -> &'static str {
        match self {
            Section::Basic => "常规",
            Section::Advanced => "高级",
            Section::Database => "数据库",
            Section::Ssl => "SSL",
            Section::Ssh => "SSH",
            Section::Proxy => "代理",
            Section::Appearance => "外观",
        }
    }
    const ALL: [Section; 7] = [
        Section::Basic,
        Section::Advanced,
        Section::Database,
        Section::Ssl,
        Section::Ssh,
        Section::Proxy,
        Section::Appearance,
    ];
}

/// SSL 验证模式；取值与旧版 Go 配置一致，驱动已兼容全部别名。
const SSL_MODES: [(&str, &str); 4] = [
    ("required", "加密"),
    ("preferred", "优先加密"),
    ("disable", "禁用"),
    ("skip-verify", "跳过校验"),
];
const SSL_VERIFY_MODES: [(&str, &str); 2] = [("verify-ca", "校验 CA"), ("verify-full", "完整校验")];

const ENVIRONMENTS: [(&str, &str); 3] = [("dev", "开发"), ("test", "测试"), ("prod", "生产")];
const TOPOLOGIES: [(&str, &str); 4] = [
    ("single", "单节点"),
    ("replica", "主从"),
    ("cluster", "集群"),
    ("sentinel", "哨兵"),
];

/// 外观页的预设颜色，与 Fyne 版保持同一组。
const ICON_COLORS: [&str; 15] = [
    "#336791", "#003B57", "#DC382D", "#47A248", "#F80000", "#CC2927", "#1890FF", "#E6002D",
    "#FFBF00", "#2446A8", "#00A86B", "#0050B3", "#EA580C", "#7C3AED", "#0F766E",
];

pub struct ConnectionForm {
    picker: Entity<SourcePicker>,
    network: network::NetworkFields,
    remember_password: bool,
    connect_after_save: bool,
    read_only: bool,
    protect_edit: bool,
    protect_structure: bool,
    protect_script: bool,
    protect_import: bool,
    generation: u64,
    saving: bool,
    extra: Entity<SqlTextEditor>,
    group: Entity<SqlEditor>,
    service: Arc<ProfileService>,
    queries: Arc<QueryService>,
    settings: Arc<SettingsService>,
    theme: Theme,
    open: bool,
    /// Some 表示编辑已有连接；保存时保留 id、revision 与未改动的配置。
    editing: Option<Profile>,
    kind: String,
    section: Section,
    use_ssl: bool,
    ssl_mode: String,
    environment: String,
    topology: String,
    icon_type: String,
    icon_color: String,
    name: Entity<SqlEditor>,
    host: Entity<SqlEditor>,
    port: Entity<SqlEditor>,
    user: Entity<SqlEditor>,
    database: Entity<SqlEditor>,
    password: Entity<SqlEditor>,
    uri: Entity<SqlEditor>,
    params: Entity<SqlEditor>,
    dsn: Entity<SqlEditor>,
    driver: Entity<SqlEditor>,
    allow: Entity<SqlEditor>,
    include: Entity<SqlEditor>,
    exclude: Entity<SqlEditor>,
    hosts: Entity<SqlEditor>,
    ssl_ca: Entity<SqlEditor>,
    ssl_cert: Entity<SqlEditor>,
    ssl_key: Entity<SqlEditor>,
    timeout: Entity<SqlEditor>,
    query_timeout: Entity<SqlEditor>,
    error: Option<SharedString>,
    notice: Option<SharedString>,
    test_cancel: Option<superlink_app::QueryCancel>,
    testing: bool,
    on_saved: Option<SavedHandler>,
}

impl ConnectionForm {
    pub fn new(
        service: Arc<ProfileService>,
        queries: Arc<QueryService>,
        settings: Arc<SettingsService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| SourcePicker::new(theme, cx));
        let editors: Vec<Entity<SqlEditor>> = (0..20).map(|_| cx.new(SqlEditor::new)).collect();
        let mut iter = editors.into_iter();
        let mut next = || iter.next().expect("editor count");
        let mut form = Self {
            picker,
            network: network::NetworkFields::new(cx),
            remember_password: false,
            connect_after_save: true,
            read_only: false,
            protect_edit: false,
            protect_structure: false,
            protect_script: false,
            protect_import: false,
            generation: 0,
            saving: false,
            extra: {
                let editor = cx.new(SqlTextEditor::new);
                editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
                editor
            },
            group: cx.new(SqlEditor::new),
            service,
            queries,
            settings,
            theme,
            open: false,
            editing: None,
            kind: "mysql".into(),
            section: Section::Basic,
            use_ssl: false,
            ssl_mode: "required".into(),
            environment: "dev".into(),
            topology: "single".into(),
            icon_type: String::new(),
            icon_color: String::new(),
            name: next(),
            host: next(),
            port: next(),
            user: next(),
            database: next(),
            password: next(),
            uri: next(),
            params: next(),
            dsn: next(),
            driver: next(),
            allow: next(),
            include: next(),
            exclude: next(),
            hosts: next(),
            ssl_ca: next(),
            ssl_cert: next(),
            ssl_key: next(),
            timeout: next(),
            query_timeout: next(),
            error: None,
            notice: None,
            test_cancel: None,
            testing: false,
            on_saved: None,
        };
        form.password.update(cx, |e, cx| e.set_masked(true, cx));
        form.uri.update(cx, |e, cx| e.set_masked(true, cx));
        form.dsn.update(cx, |e, cx| e.set_masked(true, cx));
        // 连接串非空时其它字段变为只读并置灰，避免两处取值互相覆盖。
        // 连接串内容变化时重绘，让其余字段的只读状态与提示跟着更新。
        let uri = form.uri.clone();
        cx.observe(&uri, |_this, _editor, cx| cx.notify()).detach();
        form.wire_tab_order(cx);
        form
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.picker
            .update(cx, |picker, cx| picker.set_theme(theme, cx));
        self.extra
            .update(cx, |editor, cx| editor.set_theme(theme, cx));
        cx.notify();
    }

    pub fn set_on_saved(
        &mut self,
        handler: impl Fn(&str, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_saved = Some(Box::new(handler));
    }

    fn wire_tab_order(&mut self, cx: &mut Context<Self>) {
        let order: Vec<Entity<SqlEditor>> = vec![
            self.name.clone(),
            self.host.clone(),
            self.port.clone(),
            self.user.clone(),
            self.database.clone(),
            self.password.clone(),
            self.uri.clone(),
            self.params.clone(),
            self.dsn.clone(),
            self.driver.clone(),
            self.allow.clone(),
            self.include.clone(),
            self.exclude.clone(),
            self.hosts.clone(),
            self.group.clone(),
            self.ssl_ca.clone(),
            self.ssl_cert.clone(),
            self.ssl_key.clone(),
            self.timeout.clone(),
            self.query_timeout.clone(),
        ];
        let handles: Vec<gpui::FocusHandle> = order
            .iter()
            .map(|editor| editor.read(cx).focus_handle(cx))
            .collect();
        for editor in &order {
            let handles = handles.clone();
            editor.update(cx, |editor, cx| {
                editor.set_on_tab(
                    move |shift, window, _app| {
                        let current = handles
                            .iter()
                            .position(|handle| handle.is_focused(window))
                            .unwrap_or(0);
                        let last = handles.len() - 1;
                        let next = if shift {
                            current.checked_sub(1).unwrap_or(last)
                        } else if current == last {
                            0
                        } else {
                            current + 1
                        };
                        window.focus(&handles[next]);
                    },
                    cx,
                );
            });
        }
    }

    pub(super) fn text(editor: &Entity<SqlEditor>, cx: &Context<Self>) -> String {
        editor.read(cx).text().to_string()
    }

    fn values(&self, cx: &Context<Self>) -> FormValues {
        FormValues {
            kind: self.kind.clone(),
            name: Self::text(&self.name, cx),
            group: Self::text(&self.group, cx),
            environment: self.environment.clone(),
            host: Self::text(&self.host, cx),
            port: Self::text(&self.port, cx),
            user: Self::text(&self.user, cx),
            database: Self::text(&self.database, cx),
            password: Self::text(&self.password, cx),
            uri: Self::text(&self.uri, cx),
            dsn: Self::text(&self.dsn, cx),
            driver: Self::text(&self.driver, cx),
            connection_params: Self::text(&self.params, cx),
            hosts: Self::text(&self.hosts, cx),
            topology: self.topology.clone(),
            allow: Self::text(&self.allow, cx),
            include: Self::text(&self.include, cx),
            exclude: Self::text(&self.exclude, cx),
            ssl_ca: Self::text(&self.ssl_ca, cx),
            ssl_cert: Self::text(&self.ssl_cert, cx),
            ssl_key: Self::text(&self.ssl_key, cx),
            timeout: Self::text(&self.timeout, cx),
            query_timeout: Self::text(&self.query_timeout, cx),
            use_ssl: self.use_ssl,
            ssl_mode: self.ssl_mode.clone(),
            read_only: self.read_only,
            protect_edit: self.protect_edit,
            protect_structure: self.protect_structure,
            protect_script: self.protect_script,
            protect_import: self.protect_import,
            icon_type: self.icon_type.clone(),
            icon_color: self.icon_color.clone(),
        }
    }

    pub(super) fn current_section(&self) -> Section {
        self.section
    }

    pub(super) fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    pub(super) fn icon_type(&self) -> &str {
        &self.icon_type
    }

    pub(super) fn icon_color(&self) -> &str {
        &self.icon_color
    }

    pub(super) fn set_icon(
        &mut self,
        icon_type: String,
        icon_color: String,
        cx: &mut Context<Self>,
    ) {
        self.icon_type = icon_type;
        self.icon_color = icon_color;
        cx.notify();
    }

    pub(super) fn flags(&self) -> Flags {
        Flags {
            remember_password: self.remember_password,
            connect_after_save: self.connect_after_save,
            read_only: self.read_only,
            protect_edit: self.protect_edit,
            protect_structure: self.protect_structure,
            protect_script: self.protect_script,
            protect_import: self.protect_import,
            use_ssl: self.use_ssl,
        }
    }

    pub(super) fn ssl_mode(&self) -> &str {
        &self.ssl_mode
    }

    pub(super) fn environment(&self) -> &str {
        &self.environment
    }

    pub(super) fn topology(&self) -> &str {
        &self.topology
    }

    pub(super) fn editing(&self) -> bool {
        self.editing.is_some()
    }

    pub(super) fn testing(&self) -> bool {
        self.testing
    }

    pub(super) fn error(&self) -> Option<SharedString> {
        self.error.clone()
    }

    pub(super) fn notice(&self) -> Option<SharedString> {
        self.notice.clone()
    }

    pub(super) fn kind(&self) -> &str {
        &self.kind
    }

    pub(super) fn kind_descriptor(&self) -> Option<&'static superlink_domain::Descriptor> {
        resolve(&self.kind).ok()
    }

    /// 类型说明，对齐旧版 protocolHint：提示驱动 Agent 与可查询对象。
    pub(super) fn protocol_hint(&self) -> String {
        let Some(descriptor) = self.kind_descriptor() else {
            return String::new();
        };
        let mut hint = format!("{} · ", descriptor.name);
        if descriptor.agent {
            hint.push_str("需要对应的独立驱动 Agent。");
        }
        use superlink_domain::Family;
        hint.push_str(match descriptor.family {
            Family::Sql => "可查询库表；SQLite / DuckDB 使用文件路径或 :memory:。",
            Family::Cache => "Redis 集群与 Sentinel 可在高级配置中填写。",
            Family::Message => "消息读取会限量，ACK 行为显示在工作台。",
            _ => "协议特有参数可在高级配置中填写。",
        });
        hint
    }

    pub(super) fn catalog_entries() -> &'static [superlink_domain::Descriptor] {
        catalog()
    }

    pub(super) fn editors(&self) -> Editors {
        Editors {
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port.clone(),
            user: self.user.clone(),
            database: self.database.clone(),
            password: self.password.clone(),
            uri: self.uri.clone(),
            params: self.params.clone(),
            dsn: self.dsn.clone(),
            driver: self.driver.clone(),
            allow: self.allow.clone(),
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            hosts: self.hosts.clone(),
            group: self.group.clone(),
            ssl_ca: self.ssl_ca.clone(),
            ssl_cert: self.ssl_cert.clone(),
            ssl_key: self.ssl_key.clone(),
            timeout: self.timeout.clone(),
            query_timeout: self.query_timeout.clone(),
            extra: self.extra.clone(),
        }
    }

    pub(super) fn network(&self) -> &network::NetworkFields {
        &self.network
    }

    /// 连接串相关动作：解析并填充、从字段生成、复制。
    pub(super) fn parse_uri_action(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        let text = Self::text(&self.uri, cx);
        match uri::parse_uri(&text) {
            Ok(parsed) => {
                let set = |editor: &Entity<SqlEditor>, value: String, cx: &mut Context<Self>| {
                    editor.update(cx, |e, cx| e.set_text(value, cx));
                };
                set(&self.host, parsed.host, cx);
                set(&self.port, parsed.port, cx);
                set(&self.user, parsed.user, cx);
                set(&self.password, parsed.password, cx);
                set(&self.database, parsed.database, cx);
                set(&self.params, parsed.params, cx);
                self.notice = Some("已按连接串填充字段；清空连接串可改用手动字段".into());
            }
            Err(message) => self.error = Some(message.into()),
        }
        cx.notify();
    }

    pub(super) fn generate_uri_action(&mut self, cx: &mut Context<Self>) {
        let scheme = match self.kind.as_str() {
            "postgres" => "postgresql",
            "sqlserver" => "sqlserver",
            other => other,
        };
        let uri = uri::build_uri(
            scheme,
            &Self::text(&self.host, cx),
            &Self::text(&self.port, cx),
            &Self::text(&self.user, cx),
            &Self::text(&self.password, cx),
            &Self::text(&self.database, cx),
            &Self::text(&self.params, cx),
        );
        self.uri.update(cx, |e, cx| e.set_text(uri, cx));
        cx.notify();
    }

    pub(super) fn copy_uri_action(&mut self, cx: &mut Context<Self>) {
        let text = Self::text(&self.uri, cx);
        if text.trim().is_empty() {
            self.error = Some("连接串为空，无可复制内容".into());
        } else {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            self.notice = Some("连接串已复制".into());
        }
        cx.notify();
    }

    /// 本地数据库文件选择：写入数据库字段（驱动读取的是文件路径）。
    pub(super) fn pick_database_file(&mut self, cx: &mut Context<Self>) {
        let generation = self.generation;
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择数据库文件".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.first()
            {
                let value = path.to_string_lossy().to_string();
                let _ = this.update(cx, |this, cx| {
                    if this.generation == generation {
                        this.database.update(cx, |e, cx| e.set_text(value, cx));
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}

/// 布尔开关的聚集体，供视图层一次性读写。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Flags {
    pub remember_password: bool,
    pub connect_after_save: bool,
    pub read_only: bool,
    pub protect_edit: bool,
    pub protect_structure: bool,
    pub protect_script: bool,
    pub protect_import: bool,
    pub use_ssl: bool,
}

pub(super) struct Editors {
    pub name: Entity<SqlEditor>,
    pub host: Entity<SqlEditor>,
    pub port: Entity<SqlEditor>,
    pub user: Entity<SqlEditor>,
    pub database: Entity<SqlEditor>,
    pub password: Entity<SqlEditor>,
    pub uri: Entity<SqlEditor>,
    pub params: Entity<SqlEditor>,
    pub dsn: Entity<SqlEditor>,
    pub driver: Entity<SqlEditor>,
    pub allow: Entity<SqlEditor>,
    pub include: Entity<SqlEditor>,
    pub exclude: Entity<SqlEditor>,
    pub hosts: Entity<SqlEditor>,
    pub group: Entity<SqlEditor>,
    pub ssl_ca: Entity<SqlEditor>,
    pub ssl_cert: Entity<SqlEditor>,
    pub ssl_key: Entity<SqlEditor>,
    pub timeout: Entity<SqlEditor>,
    pub query_timeout: Entity<SqlEditor>,
    pub extra: Entity<SqlTextEditor>,
}

pub(super) const SSL_MODE_OPTIONS: [(&str, &str); 4] = SSL_MODES;
pub(super) const SSL_VERIFY_OPTIONS: [(&str, &str); 2] = SSL_VERIFY_MODES;
pub(super) const ENVIRONMENT_OPTIONS: [(&str, &str); 3] = ENVIRONMENTS;
pub(super) const TOPOLOGY_OPTIONS: [(&str, &str); 4] = TOPOLOGIES;
pub(super) const ICON_COLOR_OPTIONS: [&str; 15] = ICON_COLORS;

fn seconds(value: u32) -> String {
    if value == 0 {
        String::new()
    } else {
        value.to_string()
    }
}

impl Render for ConnectionForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 视图层在 view.rs；这里只选择渲染表单还是选择器。
        self.render_form(window, cx)
    }
}
