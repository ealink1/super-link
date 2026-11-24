//! 连接表单：新建与编辑共用，包含 SSL/TLS、超时与连接测试。

use std::sync::Arc;

use gpui::{
    App, Context, Entity, Focusable, FontWeight, IntoElement, Render, SharedString, Window, div,
    prelude::*, px,
};
use superlink_app::{Credentials, ProfileService, QueryService};
use superlink_domain::{Profile, catalog, resolve};

mod model;
mod network;
mod view;

use model::{FormValues, compose};

use crate::editor::SqlEditor;
use crate::icons;
use crate::theme::Theme;

type SavedHandler = Box<dyn Fn(&str, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Basic,
    Tls,
    Network,
    Advanced,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Basic => "基础",
            Section::Tls => "SSL/TLS",
            Section::Network => "SSH / 代理",
            Section::Advanced => "高级",
        }
    }
}

/// SSL 模式与旧版 Go 配置保持同一套取值，驱动已做兼容。
const SSL_MODES: [(&str, &str); 3] = [
    ("required", "加密"),
    ("verify-ca", "校验 CA"),
    ("verify-full", "完整校验"),
];

pub struct ConnectionForm {
    network: network::NetworkFields,
    remember_password: bool,
    read_only: bool,
    generation: u64,
    saving: bool,
    extra: Entity<SqlEditor>,
    group: Entity<SqlEditor>,
    environment: Entity<SqlEditor>,
    service: Arc<ProfileService>,
    queries: Arc<QueryService>,
    theme: Theme,
    open: bool,
    /// Some 表示编辑已有连接；保存时保留 id、revision 与未改动的配置。
    editing: Option<Profile>,
    kind: String,
    section: Section,
    use_ssl: bool,
    ssl_mode: String,
    name: Entity<SqlEditor>,
    host: Entity<SqlEditor>,
    port: Entity<SqlEditor>,
    user: Entity<SqlEditor>,
    database: Entity<SqlEditor>,
    password: Entity<SqlEditor>,
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
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let editors: Vec<Entity<SqlEditor>> = (0..11).map(|_| cx.new(SqlEditor::new)).collect();
        let mut iter = editors.into_iter();
        let mut next = || iter.next().expect("editor count");
        let mut form = Self {
            network: network::NetworkFields::new(cx),
            remember_password: false,
            read_only: false,
            generation: 0,
            saving: false,
            extra: cx.new(SqlEditor::new),
            group: cx.new(SqlEditor::new),
            environment: cx.new(SqlEditor::new),
            service,
            queries,
            theme,
            open: false,
            editing: None,
            kind: "sqlite".into(),
            section: Section::Basic,
            use_ssl: false,
            ssl_mode: "required".into(),
            name: next(),
            host: next(),
            port: next(),
            user: next(),
            database: next(),
            password: next(),
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
        form.wire_tab_order(cx);
        form
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn set_on_saved(
        &mut self,
        handler: impl Fn(&str, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) {
        self.on_saved = Some(Box::new(handler));
    }

    pub fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.open = true;
        self.editing = None;
        self.generation = self.generation.wrapping_add(1);
        self.remember_password = false;
        self.read_only = false;
        self.network.load(&Default::default(), cx);
        self.extra.update(cx, |e, cx| e.set_text("", cx));
        self.group.update(cx, |e, cx| e.set_text("", cx));
        self.environment.update(cx, |e, cx| e.set_text("", cx));
        self.kind = "sqlite".into();
        self.section = Section::Basic;
        self.use_ssl = false;
        self.ssl_mode = "required".into();
        self.error = None;
        self.notice = None;
        self.testing = false;
        for editor in [
            &self.name,
            &self.host,
            &self.port,
            &self.user,
            &self.database,
            &self.password,
            &self.ssl_ca,
            &self.ssl_cert,
            &self.ssl_key,
            &self.timeout,
            &self.query_timeout,
        ] {
            editor.update(cx, |editor, cx| editor.set_text(String::new(), cx));
        }
        self.set_placeholders(cx);
        self.apply_kind_port("sqlite", cx);
        self.focus_first(window, cx);
        cx.notify();
    }

    pub fn open_edit(&mut self, profile: Profile, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.open = true;
        self.generation = self.generation.wrapping_add(1);
        self.remember_password = profile.config.save_password;
        self.read_only = profile.read_only || profile.config.read_only;
        self.network.load(&profile.config, cx);
        self.group
            .update(cx, |e, cx| e.set_text(profile.group.clone(), cx));
        self.environment
            .update(cx, |e, cx| e.set_text(profile.environment.clone(), cx));
        self.extra.update(cx, |e, cx| e.set_text("", cx));
        self.kind = profile.config.kind.clone();
        if self.kind.is_empty() {
            self.kind = profile.sql_dialect().to_string();
        }
        self.section = Section::Basic;
        self.use_ssl = profile.config.use_ssl;
        self.ssl_mode = if profile.config.ssl_mode.trim().is_empty() {
            "required".into()
        } else {
            profile.config.ssl_mode.clone()
        };
        self.error = None;
        self.notice = None;
        self.testing = false;
        let password = String::new();
        let loader = self.service.clone();
        let snapshot = profile.clone();
        let generation = self.generation;
        let task = cx.background_executor().spawn(async move {
            loader
                .credentials(&snapshot)
                .map(|credentials| credentials.unwrap_or_default().apply(&snapshot.config))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || !this.open {
                    return;
                }
                match result {
                    Ok(config) => {
                        this.password
                            .update(cx, |e, cx| e.set_text(config.password.clone(), cx));
                        this.network.load(&config, cx);
                    }
                    Err(e) => this.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
        let port = if profile.config.port == 0 {
            String::new()
        } else {
            profile.config.port.to_string()
        };
        let set = |editor: &Entity<SqlEditor>, text: String, cx: &mut Context<Self>| {
            editor.update(cx, |editor, cx| editor.set_text(text, cx));
        };
        set(&self.name, profile.name.clone(), cx);
        set(&self.host, profile.config.host.clone(), cx);
        set(&self.port, port, cx);
        set(&self.user, profile.config.user.clone(), cx);
        set(&self.database, profile.config.database.clone(), cx);
        set(&self.password, password, cx);
        set(&self.ssl_ca, profile.config.ssl_ca_path.clone(), cx);
        set(&self.ssl_cert, profile.config.ssl_cert_path.clone(), cx);
        set(&self.ssl_key, profile.config.ssl_key_path.clone(), cx);
        set(&self.timeout, seconds(profile.config.timeout), cx);
        set(
            &self.query_timeout,
            seconds(profile.config.query_timeout),
            cx,
        );
        self.set_placeholders(cx);
        self.editing = Some(profile);
        self.focus_first(window, cx);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.cancel_test(cx);
        self.open = false;
        self.generation = self.generation.wrapping_add(1);
        self.testing = false;
        cx.notify();
    }

    fn set_placeholders(&self, cx: &mut Context<Self>) {
        let placeholders = [
            (&self.name, "例如 生产库"),
            (&self.host, "例如 127.0.0.1"),
            (&self.port, "例如 3306"),
            (&self.user, "例如 root"),
            (&self.database, "例如 mydb"),
            (&self.password, "密码"),
            (&self.ssl_ca, "例如 /etc/ssl/ca.pem"),
            (&self.ssl_cert, "客户端证书（可选）"),
            (&self.ssl_key, "客户端私钥（可选）"),
            (&self.timeout, "10"),
            (&self.query_timeout, "30"),
        ];
        for (editor, hint) in placeholders {
            editor.update(cx, |editor, cx| editor.set_placeholder(hint, cx));
        }
    }

    fn focus_first(&self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.name.read(cx).focus_handle(cx);
        window.focus(&handle);
    }

    fn wire_tab_order(&mut self, cx: &mut Context<Self>) {
        let handles: Vec<gpui::FocusHandle> = [
            &self.name,
            &self.host,
            &self.port,
            &self.user,
            &self.database,
            &self.password,
            &self.ssl_ca,
            &self.ssl_cert,
            &self.ssl_key,
            &self.timeout,
            &self.query_timeout,
        ]
        .into_iter()
        .map(|editor| editor.read(cx).focus_handle(cx))
        .collect();
        for editor in [
            &self.name,
            &self.host,
            &self.port,
            &self.user,
            &self.database,
            &self.password,
            &self.ssl_ca,
            &self.ssl_cert,
            &self.ssl_key,
            &self.timeout,
            &self.query_timeout,
        ] {
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

    fn apply_kind_port(&mut self, kind: &str, cx: &mut Context<Self>) {
        if let Ok(descriptor) = resolve(kind) {
            let port = descriptor
                .port
                .map(|port| port.to_string())
                .unwrap_or_default();
            self.port.update(cx, |editor, cx| editor.set_text(port, cx));
        }
    }

    pub fn set_kind(&mut self, kind: String, cx: &mut Context<Self>) {
        self.kind = kind.clone();
        self.apply_kind_port(&kind, cx);
        cx.notify();
    }

    fn text(editor: &Entity<SqlEditor>, cx: &Context<Self>) -> String {
        editor.read(cx).text().to_string()
    }

    /// 校验并生成待保存的连接；失败时把提示写进表单。
    fn build_profile(&mut self, cx: &mut Context<Self>) -> Option<(Profile, Credentials)> {
        self.error = None;
        let values = FormValues {
            kind: self.kind.clone(),
            name: Self::text(&self.name, cx),
            host: Self::text(&self.host, cx),
            port: Self::text(&self.port, cx),
            user: Self::text(&self.user, cx),
            database: Self::text(&self.database, cx),
            password: Self::text(&self.password, cx),
            ssl_ca: Self::text(&self.ssl_ca, cx),
            ssl_cert: Self::text(&self.ssl_cert, cx),
            ssl_key: Self::text(&self.ssl_key, cx),
            timeout: Self::text(&self.timeout, cx),
            query_timeout: Self::text(&self.query_timeout, cx),
            use_ssl: self.use_ssl,
            ssl_mode: self.ssl_mode.clone(),
        };
        match compose(&values, self.editing.as_ref()) {
            Ok((mut profile, credentials)) => {
                profile.group = Self::text(&self.group, cx).trim().into();
                profile.environment = Self::text(&self.environment, cx).trim().into();
                profile.read_only = self.read_only;
                profile.config.read_only = self.read_only;
                profile.config.save_password = self.remember_password;
                if let Err(message) = self.network.apply(&mut profile, cx) {
                    self.error = Some(message.into());
                    return None;
                }
                let extra = Self::text(&self.extra, cx);
                if !extra.trim().is_empty() {
                    let value: serde_json::Value = match serde_json::from_str(&extra) {
                        Ok(v) => v,
                        Err(e) => {
                            self.error = Some(format!("高级配置 JSON 无效：{e}").into());
                            return None;
                        }
                    };
                    let Some(patch) = value.as_object() else {
                        self.error = Some("高级配置应为 JSON 对象".into());
                        return None;
                    };
                    let mut base = serde_json::to_value(&profile.config).unwrap_or_default();
                    superlink_domain::secrets::merge(base.as_object_mut().unwrap(), patch);
                    profile.config = match serde_json::from_value(base) {
                        Ok(c) => c,
                        Err(e) => {
                            self.error = Some(format!("高级配置无效：{e}").into());
                            return None;
                        }
                    };
                }
                Some((profile, credentials))
            }
            Err(message) => {
                self.error = Some(message.into());
                None
            }
        }
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        let Some((profile, credentials)) = self.build_profile(cx) else {
            cx.notify();
            return;
        };
        if self.saving {
            return;
        }
        self.saving = true;
        let service = self.service.clone();
        let remember = self.remember_password;
        let generation = self.generation;
        let task = cx
            .background_executor()
            .spawn(async move { service.save_with_remember(profile, &credentials, remember) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.saving = false;
                match result {
                    Ok(saved) => {
                        this.open = false;
                        if let Some(handler) = this.on_saved.as_ref() {
                            handler(&saved.id, cx);
                        }
                    }
                    Err(e) => this.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn cancel_test(&mut self, cx: &mut Context<Self>) {
        self.testing = false;
        if let Some(cancel) = self.test_cancel.take() {
            cx.background_executor()
                .spawn(async move {
                    cancel.cancel();
                })
                .detach();
        }
    }
    pub fn test(&mut self, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        let Some((profile, credentials)) = self.build_profile(cx) else {
            cx.notify();
            return;
        };
        self.testing = true;
        let cancel = superlink_app::QueryCancel::new();
        self.test_cancel = Some(cancel.clone());
        let generation = self.generation;
        self.notice = None;
        self.error = None;
        let queries = self.queries.clone();
        cx.spawn(async move |this, cx| {
            let task = cx.background_executor().spawn(async move {
                queries.test_connection_with_cancel(&profile, &credentials, &cancel)
            });
            let outcome = task.await;
            this.update(cx, |form, cx| {
                if form.generation != generation || !form.open {
                    return;
                }
                form.test_cancel = None;
                form.testing = false;
                match outcome {
                    Ok(ms) => form.notice = Some(format!("连接成功 · {ms} ms").into()),
                    Err(err) => form.error = Some(err.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

fn seconds(value: u32) -> String {
    if value == 0 {
        String::new()
    } else {
        value.to_string()
    }
}
