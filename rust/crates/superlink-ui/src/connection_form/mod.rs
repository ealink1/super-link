//! 连接表单：新建与编辑共用，包含 SSL/TLS、超时与连接测试。

use std::sync::Arc;

use gpui::{
    App, Context, Entity, Focusable, FontWeight, IntoElement, Render, SharedString, Window, div,
    prelude::*, px,
};
use superlink_app::{Credentials, ProfileService, QueryService};
use superlink_domain::{Profile, catalog, resolve};

mod model;

use model::{FormValues, compose};

use crate::editor::SqlEditor;
use crate::icons;
use crate::theme::Theme;

type SavedHandler = Box<dyn Fn(&str, &mut App) + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Basic,
    Tls,
    Advanced,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Basic => "基础",
            Section::Tls => "SSL/TLS",
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
            testing: false,
            on_saved: None,
        };
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
        self.open = true;
        self.editing = None;
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
        self.open = true;
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
        let password = self
            .service
            .credentials(&profile)
            .ok()
            .flatten()
            .map(|credentials| credentials.password)
            .unwrap_or_default();
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
        self.open = false;
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
            Ok(pair) => Some(pair),
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
        match self.service.save(profile, &credentials) {
            Ok(saved) => {
                self.open = false;
                if let Some(handler) = self.on_saved.as_ref() {
                    handler(&saved.id, cx);
                }
            }
            Err(err) => self.error = Some(err.to_string().into()),
        }
        cx.notify();
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
        self.notice = None;
        self.error = None;
        let queries = self.queries.clone();
        cx.spawn(async move |this, cx| {
            let task = cx
                .background_executor()
                .spawn(async move { queries.test_connection(&profile, &credentials) });
            let outcome = task.await;
            this.update(cx, |form, cx| {
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

impl Render for ConnectionForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let title = if self.editing.is_some() {
            "编辑连接"
        } else {
            "新建连接"
        };
        let kind = self.kind.clone();
        let section = self.section;
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
                        this.set_kind(key.to_string(), cx)
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.surface } else { theme.text })
                            .child(descriptor.name),
                    )
            })
            .collect::<Vec<_>>();

        let mut dialog = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .mt_6()
            .w(px(680.0))
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
                            .child(title),
                    )
                    .child(
                        div()
                            .id("close-form")
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| this.close(cx)))
                            .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .max_w(px(620.0))
                    .children(chips),
            )
            .child(div().flex().items_center().gap_1().children(
                [Section::Basic, Section::Tls, Section::Advanced].map(|item| {
                    let active = item == section;
                    div()
                        .id(SharedString::from(format!("section-{:?}", item)))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if active { theme.button } else { theme.canvas })
                        .border_1()
                        .border_color(if active { theme.border } else { theme.canvas })
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.section = item;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if active { theme.text } else { theme.muted })
                                .child(item.label()),
                        )
                }),
            ));

        match section {
            Section::Basic => {
                dialog = dialog
                    .child(self.row("连接名称", &self.name))
                    .child(self.row("主机", &self.host))
                    .child(self.row("端口", &self.port))
                    .child(self.row("用户名", &self.user))
                    .child(self.row("数据库", &self.database))
                    .child(self.row("密码", &self.password));
            }
            Section::Tls => {
                dialog = dialog
                    .child(self.toggle("启用 SSL/TLS", self.use_ssl, "toggle-ssl", cx))
                    .children(self.use_ssl.then(|| {
                        div().flex().items_center().gap_1().children(SSL_MODES.map(
                            |(key, label)| {
                                let active = self.ssl_mode == key;
                                div()
                                    .id(SharedString::from(format!("ssl-{key}")))
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(if active { theme.accent } else { theme.canvas })
                                    .border_1()
                                    .border_color(if active { theme.accent } else { theme.border })
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.ssl_mode = key.to_string();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(if active {
                                                theme.surface
                                            } else {
                                                theme.text
                                            })
                                            .child(label),
                                    )
                            },
                        ))
                    }))
                    .children(self.use_ssl.then(|| self.row("CA 证书", &self.ssl_ca)))
                    .children(self.use_ssl.then(|| self.row("客户端证书", &self.ssl_cert)))
                    .children(self.use_ssl.then(|| self.row("客户端私钥", &self.ssl_key)))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child("「加密」只验证连通性；「校验 CA」需要填写 CA 证书路径。"),
                    );
            }
            Section::Advanced => {
                dialog = dialog
                    .child(self.row("连接超时（秒）", &self.timeout))
                    .child(self.row("查询超时（秒）", &self.query_timeout))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child("留空或 0 表示使用驱动默认值，最大 3600 秒。"),
                    );
            }
        }

        dialog = dialog.children(self.error.clone().map(|error| {
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.danger)
                .child(error)
        }));
        dialog = dialog.children(self.notice.clone().map(|notice| {
            div()
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.success)
                .child(notice)
        }));

        dialog.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("test-connection")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(theme.border)
                        .on_click(cx.listener(|this, _event, _window, cx| this.test(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(if self.testing {
                                    theme.muted
                                } else {
                                    theme.text
                                })
                                .child(if self.testing {
                                    "测试中…"
                                } else {
                                    "测试连接"
                                }),
                        ),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id("cancel-connection")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .border_1()
                        .border_color(theme.border)
                        .on_click(cx.listener(|this, _event, _window, cx| this.close(cx)))
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
                        .on_click(cx.listener(|this, _event, _window, cx| this.save(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("保存"),
                        ),
                ),
        )
    }
}

impl ConnectionForm {
    fn row(&self, label: &str, editor: &Entity<SqlEditor>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(96.0))
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

    fn toggle(
        &self,
        label: &str,
        on: bool,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.use_ssl = !this.use_ssl;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(16.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .border_1()
                    .border_color(if on { theme.accent } else { theme.border })
                    .bg(if on { theme.accent } else { theme.canvas })
                    .children(on.then(|| {
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.surface)
                            .child("✓")
                    })),
            )
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(label.to_string()),
            )
    }
}
