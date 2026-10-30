//! Shell workspace: host list, interactive PTY and a real VT screen.
//!
//! The transport blocks on its own runtime, so connecting and reading happen
//! on a worker thread; the GPUI thread only drains events and paints.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, FontWeight, GlobalElementId, InspectorElementId,
    KeyDownEvent, LayoutId, MouseButton, PaintQuad, Pixels, Render, ShapedLine, SharedString,
    Style, TextRun, UTF16Selection, Window, div, fill, point, prelude::*, px, relative, rgb, size,
};
use superlink_app::HostService;
use superlink_domain::ShellHost;
use superlink_terminal::{
    CancelToken, HostKeyPolicy, LocalSession, SessionHandle, SshClient, TerminalError,
    TerminalGeometry,
};

use superlink_terminal::Session;

use crate::editor::SqlEditor;
use crate::icons;
use crate::theme::Theme;
use crate::vt::{Color, Screen};

const GEOMETRY: TerminalGeometry = TerminalGeometry {
    cols: 100,
    rows: 28,
};
const POLL_INTERVAL: Duration = Duration::from_millis(16);
/// Automatic reconnect only applies to remote hosts: an exited local shell is
/// intentional and must not be restarted behind the user's back.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
const RECONNECT_BACKOFF_SECS: [u64; 5] = [1, 2, 4, 8, 15];

fn backoff_delay(attempt: u32) -> Option<u64> {
    RECONNECT_BACKOFF_SECS
        .get(attempt.checked_sub(1)? as usize)
        .copied()
}

enum Event {
    Opened {
        handle: SessionHandle,
        fingerprint: Option<String>,
    },
    Data(Vec<u8>),
    Closed(String),
    Failed(String),
    HostKeyUnknown {
        fingerprint: String,
        message: String,
    },
}

#[derive(Clone)]
enum Target {
    Local,
    Remote {
        host: Box<ShellHost>,
        policy: HostKeyPolicy,
    },
}

pub struct TerminalPage {
    hosts: Arc<HostService>,
    theme: Theme,
    host_list: Vec<ShellHost>,
    selected: usize,
    status: SharedString,
    session: Option<SessionHandle>,
    pending: Option<(ShellHost, String)>,
    screen: Screen,
    parser: vte::Parser,
    focus: FocusHandle,
    running: Arc<AtomicBool>,
    scroll_offset: usize,
    filter: String,
    search: Entity<SqlEditor>,
    /// New-host form.
    form_open: bool,
    form_name: Entity<SqlEditor>,
    form_host: Entity<SqlEditor>,
    form_port: Entity<SqlEditor>,
    form_user: Entity<SqlEditor>,
    form_password: Entity<SqlEditor>,
    form_notes: Entity<SqlEditor>,
    form_error: Option<SharedString>,
    error: Option<SharedString>,
    /// IME preedit text, shown locally and never sent before commit.
    composing: Composition,
    /// Last connection attempt, kept so a dropped transport can be rebuilt.
    target: Option<Target>,
    auto_reconnect: bool,
    attempts: u32,
    /// Invalidates a pending reconnect after a manual disconnect.
    generation: u64,
    /// Host page filters mirrored from the Fyne shell.
    privacy: bool,
    list_mode: bool,
    group: String,
    rail_expanded: bool,
    notice: Option<SharedString>,
}

impl TerminalPage {
    pub fn new(hosts: Arc<HostService>, theme: Theme, cx: &mut Context<Self>) -> Self {
        let host_list = hosts.list().unwrap_or_default();
        let weak_page = cx.entity().downgrade();
        let search = cx.new(SqlEditor::new);
        search.update(cx, move |editor, cx| {
            editor.set_placeholder("搜索主机…", cx);
            let weak_page = weak_page.clone();
            editor.set_on_change(
                move |text, app| {
                    let weak_page = weak_page.clone();
                    let _ = weak_page.update(app, |page, cx| {
                        page.filter = text.to_string();
                        cx.notify();
                    });
                },
                cx,
            );
        });
        Self {
            hosts,
            theme,
            host_list,
            selected: 0,
            status: "选择主机或打开本地终端".into(),
            session: None,
            pending: None,
            screen: Screen::new(GEOMETRY.cols as usize, GEOMETRY.rows as usize),
            parser: vte::Parser::new(),
            focus: cx.focus_handle(),
            running: Arc::new(AtomicBool::new(false)),
            scroll_offset: 0,
            filter: String::new(),
            search,
            form_open: false,
            form_name: cx.new(SqlEditor::new),
            form_host: cx.new(SqlEditor::new),
            form_port: cx.new(SqlEditor::new),
            form_user: cx.new(SqlEditor::new),
            form_password: cx.new(SqlEditor::new),
            form_notes: cx.new(SqlEditor::new),
            form_error: None,
            error: None,
            composing: Composition::default(),
            target: None,
            auto_reconnect: false,
            attempts: 0,
            generation: 0,
            privacy: false,
            list_mode: false,
            group: String::new(),
            rail_expanded: false,
            notice: None,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn open_host_form(&mut self, cx: &mut Context<Self>) {
        self.form_open = true;
        self.form_error = None;
        for editor in [
            &self.form_name,
            &self.form_host,
            &self.form_port,
            &self.form_user,
            &self.form_password,
            &self.form_notes,
        ] {
            editor.update(cx, |editor, cx| {
                editor.set_text(String::new(), cx);
                editor.set_masked(false, cx);
            });
        }
        self.form_password
            .update(cx, |editor, cx| editor.set_masked(true, cx));
        self.form_port
            .update(cx, |editor, cx| editor.set_text("22", cx));
        cx.notify();
    }

    pub fn close_host_form(&mut self, cx: &mut Context<Self>) {
        self.form_open = false;
        cx.notify();
    }

    pub fn save_host(&mut self, cx: &mut Context<Self>) {
        use superlink_app::HostCredentials;

        self.form_error = None;
        let name = Self::editor_text(&self.form_name, cx);
        let host = Self::editor_text(&self.form_host, cx);
        let port_text = Self::editor_text(&self.form_port, cx);
        let user = Self::editor_text(&self.form_user, cx);
        let password = Self::editor_text(&self.form_password, cx);
        let notes = Self::editor_text(&self.form_notes, cx);

        let port: u16 = if port_text.trim().is_empty() {
            22
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
        let record = ShellHost {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            host: host.trim().to_string(),
            user: user.trim().to_string(),
            port: if port == 0 { 22 } else { port },
            notes: notes.trim().to_string(),
            ..Default::default()
        };
        let credentials = HostCredentials {
            password: password.trim().to_string(),
            ..Default::default()
        };
        match self.hosts.save(record, &credentials) {
            Ok(saved) => {
                self.form_open = false;
                self.status = format!("已保存主机「{}」", saved.name).into();
                self.reload_hosts(cx);
            }
            Err(err) => self.form_error = Some(err.to_string().into()),
        }
        cx.notify();
    }

    fn editor_text(entity: &Entity<SqlEditor>, cx: &Context<Self>) -> String {
        entity.read(cx).text().to_string()
    }

    fn reload_hosts(&mut self, cx: &mut Context<Self>) {
        self.host_list = self.hosts.list().unwrap_or_default();
        if self.selected >= self.host_list.len() {
            self.selected = self.host_list.len().saturating_sub(1);
        }
        cx.notify();
    }

    fn connect_host_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = index;
        self.connect_selected(cx);
        self.focus.focus(window);
        cx.notify();
    }

    fn connect_local(&mut self, cx: &mut Context<Self>) {
        self.attempts = 0;
        self.auto_reconnect = true;
        self.begin(Target::Local, true, cx);
    }

    fn connect_selected(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host_list.get(self.selected).cloned() else {
            self.error = Some("还没有可连接的主机".into());
            cx.notify();
            return;
        };
        let policy = if host.fingerprint.is_empty() {
            HostKeyPolicy::Ask
        } else {
            HostKeyPolicy::Pinned(host.fingerprint.clone())
        };
        self.attempts = 0;
        self.auto_reconnect = true;
        self.begin(
            Target::Remote {
                host: Box::new(host),
                policy,
            },
            true,
            cx,
        );
    }

    /// Manual reconnect after a drop, or a retry from the header button.
    fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.auto_reconnect = false;
        self.generation += 1;
        self.attempts = 0;
        if let Some(handle) = self.session.take() {
            let _ = handle.close();
        }
        self.status = "已断开".into();
        self.error = None;
        cx.notify();
    }

    fn begin(&mut self, target: Target, clear_screen: bool, cx: &mut Context<Self>) {
        if self.session.is_some() {
            return;
        }
        // Any new attempt invalidates a retry that is still counting down, so a
        // manual connect cannot be hijacked by the previous host's timer.
        self.generation += 1;
        self.target = Some(target.clone());
        self.error = None;
        self.pending = None;
        self.status = "连接中…".into();
        if clear_screen {
            self.screen = Screen::new(GEOMETRY.cols as usize, GEOMETRY.rows as usize);
            self.scroll_offset = 0;
        }
        cx.notify();

        let (tx, rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        self.running = running.clone();
        spawn_worker(target, tx);
        self.poll(rx, running, cx);
    }

    fn should_auto_reconnect(&self) -> bool {
        self.auto_reconnect && matches!(self.target, Some(Target::Remote { .. }))
    }

    fn schedule_reconnect(&mut self, reason: String, cx: &mut Context<Self>) {
        if !self.should_auto_reconnect() {
            self.status = SharedString::from(reason);
            return;
        }
        let Some(target) = self.target.clone() else {
            return;
        };
        let attempt = self.attempts + 1;
        let Some(delay) = backoff_delay(attempt) else {
            self.status = "重连失败，已达上限，请手动重连".into();
            self.error = Some(reason.into());
            cx.notify();
            return;
        };
        self.attempts = attempt;
        self.generation += 1;
        let generation = self.generation;
        self.screen.feed(
            &mut self.parser,
            format!("\r\n\x1b[33m[{reason}]\x1b[0m\r\n").as_bytes(),
        );
        self.status =
            format!("已断开，{delay} 秒后重连（{attempt}/{MAX_RECONNECT_ATTEMPTS}）").into();
        cx.notify();

        cx.spawn(async move |this, cx| {
            for remaining in (1..=delay).rev() {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this
                    .update(cx, |this, cx| {
                        if this.generation != generation {
                            return false;
                        }
                        this.status = format!(
                            "已断开，{remaining} 秒后重连（{attempt}/{MAX_RECONNECT_ATTEMPTS}）"
                        )
                        .into();
                        cx.notify();
                        true
                    })
                    .is_ok();
                if !alive {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.auto_reconnect {
                    this.begin(target, false, cx);
                }
            });
        })
        .detach();
    }

    fn poll(&mut self, rx: Receiver<Event>, running: Arc<AtomicBool>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while running.load(Ordering::Relaxed) {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let events: Vec<Event> = rx.try_iter().collect();
                if events.is_empty() {
                    continue;
                }
                let alive = this.update(cx, |this, cx| this.apply(events, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn apply(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        let mut changed = false;
        for event in events {
            changed = true;
            match event {
                Event::Opened {
                    handle,
                    fingerprint,
                } => {
                    self.session = Some(handle);
                    self.attempts = 0;
                    self.error = None;
                    self.status = match fingerprint {
                        Some(fingerprint) => SharedString::from(format!("已连接 · {fingerprint}")),
                        None => "本地终端".into(),
                    };
                    self.pending = None;
                }
                Event::Data(bytes) => {
                    if self.scroll_offset > 0 {
                        self.scroll_offset = 0;
                    }
                    self.screen.feed(&mut self.parser, &bytes[..]);
                }
                Event::Closed(message) => {
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.schedule_reconnect(message, cx);
                }
                Event::Failed(message) => {
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.status = "连接失败".into();
                    self.schedule_reconnect(message, cx);
                }
                Event::HostKeyUnknown {
                    fingerprint,
                    message,
                } => {
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.status = "等待确认主机指纹".into();
                    self.error = Some(message.into());
                    if let Some(host) = self.host_list.get(self.selected).cloned() {
                        self.pending = Some((host, fingerprint));
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn trust(&mut self, remember: bool, cx: &mut Context<Self>) {
        let Some((mut host, fingerprint)) = self.pending.take() else {
            return;
        };
        if remember {
            host.fingerprint = fingerprint.clone();
            if let Err(err) = self.hosts.save(host.clone(), &Default::default()) {
                self.error = Some(format!("保存指纹失败：{err}").into());
                cx.notify();
                return;
            }
        }
        let policy = if remember {
            HostKeyPolicy::TrustAndRemember(fingerprint)
        } else {
            HostKeyPolicy::TrustOnce(fingerprint)
        };
        self.attempts = 0;
        self.auto_reconnect = true;
        self.begin(
            Target::Remote {
                host: Box::new(host),
                policy,
            },
            true,
            cx,
        );
    }

    fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let Some(handle) = self.session.clone() else {
            return;
        };
        if let Err(err) = handle.write(bytes) {
            self.error = Some(err.to_string().into());
            cx.notify();
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        // While an IME owns the composition, its keys belong to the candidate
        // window: forwarding Enter or arrows here would corrupt the input.
        if self.composing.is_active() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        if modifiers.platform {
            if key == "v" {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.send(text.as_bytes(), cx);
                }
                return;
            }
            if key == "c" {
                if let Some(handle) = self.session.clone() {
                    let _ = handle.write(&[0x03]);
                }
                return;
            }
        }
        if modifiers.shift && key == "pageup" {
            self.scroll_offset = (self.scroll_offset + 5).min(self.screen.scrollback_len());
            cx.notify();
            return;
        }
        if modifiers.shift && key == "pagedown" {
            self.scroll_offset = self.scroll_offset.saturating_sub(5);
            cx.notify();
            return;
        }
        if let Some(bytes) = encode_key(key, modifiers.control) {
            self.send(&bytes, cx);
        }
    }

    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let width = if self.rail_expanded { 144.0 } else { 64.0 };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .w(px(width))
            .h_full()
            .py_3()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(self.rail_button(
                "rail-toggle",
                "shell/chevron-right.svg",
                "",
                false,
                cx,
                Self::toggle_rail,
            ))
            .child(div().h(px(8.0)))
            .child(self.rail_button(
                "rail-hosts",
                "shell/server.svg",
                "主机",
                self.session.is_none(),
                cx,
                Self::show_hosts,
            ))
            .child(self.rail_button(
                "rail-terminal",
                "shell/terminal.svg",
                "终端",
                self.session.is_some(),
                cx,
                Self::focus_terminal,
            ))
            .child(div().flex_1())
            .child(self.rail_button(
                "rail-settings",
                "shell/settings.svg",
                "设置",
                false,
                cx,
                Self::shell_settings_notice,
            ))
    }

    fn rail_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        handler: fn(&mut Self, &mut Window, &mut Context<Self>),
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        let expanded = self.rail_expanded;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_2()
            .when(expanded, |style| style.w_full().px_2().justify_start())
            .when(!expanded, |style| style.size(px(40.0)).justify_center())
            .h(px(40.0))
            .rounded_md()
            .cursor_pointer()
            .bg(if active { theme.accent_soft } else { theme.sidebar })
            .hover(move |style| style.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _event, window, cx| handler(this, window, cx)))
            .child(icons::icon(
                icon,
                22.0,
                if active { theme.accent } else { theme.muted },
            ))
            .children(if label.is_empty() || !expanded {
                None
            } else {
                Some(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child(label),
                )
            })
    }

    fn toggle_rail(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.rail_expanded = !self.rail_expanded;
        cx.notify();
    }

    fn show_hosts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.form_open = false;
        self.disconnect(cx);
    }

    fn focus_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_none() {
            self.connect_local(cx);
        }
        self.focus.focus(window);
        cx.notify();
    }

    fn shell_settings_notice(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.notice = Some(SharedString::from("Shell 设置尚未接入"));
        cx.notify();
    }

    /// 会话/表单模式的紧凑头部，保留连接状态与断开入口。
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(56.0))
            .border_b_1()
            .border_color(theme.border)
            .child(icons::icon("shell/terminal.svg", 18.0, theme.text))
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text)
                    .child(if self.session.is_some() {
                        "终端"
                    } else {
                        "新建主机"
                    }),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(self.status.clone()),
            )
            .child(div().flex_1())
            .child(if self.session.is_some() {
                div()
                    .id("disconnect")
                    .px_3()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.button)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .on_click(cx.listener(|this, _event, _window, cx| this.disconnect(cx)))
                    .child("断开")
                    .into_any_element()
            } else {
                div()
                    .id("close-host-form")
                    .px_3()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.button)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .on_click(cx.listener(|this, _event, _window, cx| this.close_host_form(cx)))
                    .child("取消")
                    .into_any_element()
            })
    }

    /// 主机管理页：头部 + 分组侧栏 + 搜索工具栏 + 卡片网格。
    fn hosts_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .h(px(56.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(icons::icon("shell/server.svg", 20.0, theme.text))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("主机管理"),
                    )
                    .child(div().flex_1())
                    .children(self.notice.as_ref().map(|notice| {
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child(notice.clone())
                            .into_any_element()
                    }))
                    .child(
                        div()
                            .id("privacy")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_3()
                            .h(px(32.0))
                            .rounded_md()
                            .cursor_pointer()
                            .border_1()
                            .border_color(theme.border)
                            .bg(if self.privacy { theme.accent_soft } else { theme.surface })
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.privacy = !this.privacy;
                                cx.notify();
                            }))
                            .child(icons::icon("shell/eye-off.svg", 14.0, theme.muted))
                            .child("隐私模式"),
                    )
                    .child(
                        div()
                            .id("new-host")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_3()
                            .h(px(32.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme.accent)
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.surface)
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.open_host_form(cx)),
                            )
                            .child(icons::icon("shell/plus.svg", 14.0, theme.surface))
                            .child("新建主机"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.group_sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.hosts_toolbar(cx))
                            .child(self.host_cards(cx)),
                    ),
            )
    }

    fn group_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let groups = self.group_counts();
        let mut rows = div().flex().flex_col().gap_1().w_full();
        rows = rows.child(
            div()
                .id("group-all")
                .flex()
                .items_center()
                .gap_2()
                .h(px(30.0))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .bg(if self.group.is_empty() {
                    theme.accent_soft
                } else {
                    theme.canvas
                })
                .hover(move |style| style.bg(theme.accent_soft))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.group.clear();
                    cx.notify();
                }))
                .child(icons::icon("shell/layout-grid.svg", 14.0, theme.muted))
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_SIZE))
                        .text_color(theme.text)
                        .child("全部主机"),
                ),
        );
        for (name, count) in groups {
            let selected = self.group == name;
            let value = name.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("group-{name}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.0))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if selected {
                        theme.accent_soft
                    } else {
                        theme.canvas
                    })
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.group = value.clone();
                        cx.notify();
                    }))
                    .child(icons::icon("shell/folder.svg", 14.0, theme.muted))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .child(SharedString::from(name)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child(format!("{count}")),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .w(px(224.0))
            .h_full()
            .px_3()
            .py_3()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .pb_2()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.muted)
                    .child("分组导航"),
            )
            .child(rows)
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.muted)
                            .child("标签筛选"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child("尚无标签"),
                    ),
            )
    }

    fn hosts_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(52.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .w(px(224.0))
                    .h(px(30.0))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(icons::icon("shell/search.svg", 13.0, theme.muted))
                    .child(div().flex_1().min_w(px(0.0)).child(self.search.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .w(px(92.0))
                    .h(px(30.0))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child("SSH")
                    .child(icons::icon("shell/chevron-down.svg", 12.0, theme.muted)),
            )
            .child(
                div()
                    .id("reload-hosts")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(30.0))
                    .rounded_md()
                    .cursor_pointer()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.reload_hosts(cx)))
                    .child(icons::icon("shell/refresh-cw.svg", 14.0, theme.muted)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id("hosts-grid")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if self.list_mode {
                                theme.surface
                            } else {
                                theme.accent_soft
                            })
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.list_mode = false;
                                cx.notify();
                            }))
                            .child(icons::icon("shell/layout-grid.svg", 14.0, theme.muted)),
                    )
                    .child(
                        div()
                            .id("hosts-list")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(if self.list_mode {
                                theme.accent_soft
                            } else {
                                theme.surface
                            })
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.list_mode = true;
                                cx.notify();
                            }))
                            .child(icons::icon("shell/list.svg", 14.0, theme.muted)),
                    ),
            )
    }

    fn group_counts(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        counts.insert("默认分组".into(), 0);
        for host in &self.host_list {
            *counts.entry(group_label(&host.group)).or_insert(0) += 1;
        }
        counts.into_iter().collect()
    }
    fn host_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
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
                    .w(px(560.0))
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
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(icons::icon("shell/server.svg", 16.0, theme.muted))
                                    .child(
                                        div()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text)
                                            .child("新建主机"),
                                    ),
                            )
                            .child(
                                div()
                                    .id("close-host-form")
                                    .px_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent_soft))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.close_host_form(cx)
                                    }))
                                    .child(icons::icon("shell/x.svg", 13.0, theme.muted)),
                            ),
                    )
                    .child(self.form_row("名称", &self.form_name))
                    .child(self.form_row("主机", &self.form_host))
                    .child(self.form_row("端口", &self.form_port))
                    .child(self.form_row("用户名", &self.form_user))
                    .child(self.form_row("密码", &self.form_password))
                    .child(self.form_row("备注", &self.form_notes))
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
                                    .id("cancel-host")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.close_host_form(cx)
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
                                    .id("save-host")
                                    .px_3()
                                    .py_1()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .on_click(
                                        cx.listener(|this, _event, _window, cx| this.save_host(cx)),
                                    )
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

    fn form_row(&self, label: &str, editor: &Entity<SqlEditor>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(64.0))
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

    /// Card grid shown while no session is open, mirroring the Go layout.
    fn host_cards(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let needle = self.filter.to_lowercase();
        let matches: Vec<(usize, &ShellHost)> = self
            .host_list
            .iter()
            .enumerate()
            .filter(|(_, host)| {
                self.group.is_empty() || group_label(&host.group) == self.group
            })
            .filter(|(_, host)| {
                needle.is_empty()
                    || host.name.to_lowercase().contains(&needle)
                    || host.host.to_lowercase().contains(&needle)
                    || host.user.to_lowercase().contains(&needle)
            })
            .collect();

        if matches.is_empty() {
            return div()
                .id("host-cards")
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .overflow_y_scroll()
                .child(icons::icon("shell/server.svg", 44.0, theme.muted))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text)
                        .child("暂无主机"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme.muted)
                        .child("新建主机，或调整搜索与筛选条件"),
                );
        }

        div()
            .id("host-cards")
            .flex()
            .flex_wrap()
            .items_start()
            .gap_4()
            .p_4()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .children(
                matches
                    .into_iter()
                    .map(|(index, host)| self.host_card(index, host, cx)),
            )
    }

    fn host_card(
        &self,
        index: usize,
        host: &ShellHost,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = self.theme;
        let name = SharedString::from(host.name.clone());
        let address = SharedString::from(if self.privacy {
            "••••@••••••".to_string()
        } else {
            format!("{}@{}:{}", host.user, host.host, host.port)
        });
        let notes = SharedString::from(
            host.notes
                .lines()
                .next()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.to_string())
                .unwrap_or_else(|| "暂无备注".into()),
        );
        let tags = SharedString::from(if host.tags.trim().is_empty() {
            "无标签".to_string()
        } else {
            host.tags.clone()
        });
        let card = div()
            .id(("host-card", index))
            .flex()
            .flex_col()
            .p_4()
            .rounded_lg()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .hover(move |style| style.border_color(theme.accent));
        if self.list_mode {
            return card
                .w_full()
                .h(px(120.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(40.0))
                                .rounded_md()
                                .bg(theme.button)
                                .child(icons::icon("shell/terminal.svg", 20.0, theme.text)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .flex_1()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(theme.text)
                                                .child(name),
                                        )
                                        .child(protocol_badge(theme)),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(theme.muted)
                                        .child(address),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme.muted)
                                        .child(notes),
                                ),
                        )
                        .child(
                            div()
                                .id(("quick-connect", index))
                                .flex()
                                .items_center()
                                .justify_center()
                                .w(px(120.0))
                                .h(px(30.0))
                                .rounded_md()
                                .cursor_pointer()
                                .bg(theme.button)
                                .border_1()
                                .border_color(theme.border)
                                .hover(move |style| style.bg(theme.accent_soft))
                                .on_click(cx.listener(move |this, _event, window, cx| {
                                    this.connect_host_at(index, window, cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.text)
                                        .child("快速连接"),
                                ),
                        ),
                );
        }
        card.w(px(300.0))
            .h(px(260.0))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(44.0))
                            .rounded_md()
                            .bg(theme.button)
                            .child(icons::icon("shell/terminal.svg", 22.0, theme.text)),
                    )
                    .child(
                        div()
                            .id(("host-menu", index))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(24.0))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.notice = Some(SharedString::from("主机菜单尚未接入"));
                                cx.notify();
                            }))
                            .child(icons::icon("shell/ellipsis.svg", 16.0, theme.muted)),
                    ),
            )
            .child(div().h(px(14.0)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(name),
                    )
                    .child(protocol_badge(theme)),
            )
            .child(div().h(px(6.0)))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(address),
            )
            .child(div().h(px(8.0)))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child(notes),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child(tags),
            )
            .child(div().h(px(14.0)))
            .child(
                div()
                    .id(("quick-connect", index))
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(30.0))
                    .w_full()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.button)
                    .border_1()
                    .border_color(theme.border)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.connect_host_at(index, window, cx)
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("快速连接"),
                    ),
            )
    }
    fn terminal_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut area = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .bg(rgb(0x101317))
            .track_focus(&self.focus)
            .key_context("Terminal")
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(|this, event, _window, cx| this.on_key(event, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| {
                    this.focus.focus(window);
                    cx.notify();
                }),
            );

        if let Some((host, fingerprint)) = self.pending.clone() {
            area =
                area.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_4()
                        .m_4()
                        .rounded_md()
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(format!("尚未信任 {} 的主机密钥", host.name)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(SharedString::from(fingerprint.clone())),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .id("trust-once")
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(theme.accent_soft)
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.accent)
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.trust(false, cx)
                                        }))
                                        .child("仅本次信任"),
                                )
                                .child(
                                    div()
                                        .id("trust-remember")
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .cursor_pointer()
                                        .bg(theme.accent)
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .text_color(theme.surface)
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.trust(true, cx)
                                        }))
                                        .child("记住并连接"),
                                ),
                        ),
                );
            return area;
        }

        if let Some(error) = self.error.clone() {
            area = area.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(rgb(0xff8a80))
                    .child(error),
            );
        }

        if let Some(composing) = self.composing.preedit().map(str::to_string) {
            area = area.child(
                div()
                    .px_3()
                    .py_1()
                    .bg(rgb(0x1b2028))
                    .border_b_1()
                    .border_color(rgb(0x2a313b))
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(rgb(0xd19a66))
                    .child(SharedString::from(format!("输入中：{composing}"))),
            );
        }

        area.child(ScreenElement {
            page: cx.entity(),
            rows: GEOMETRY.rows as usize,
            scroll_offset: self.scroll_offset,
            focused: self.session.is_some(),
        })
    }
}

/// IME preedit state. Nothing here reaches the PTY: only a commit does.
#[derive(Debug, Default, PartialEq)]
struct Composition {
    text: Option<String>,
}

impl Composition {
    fn mark(&mut self, text: &str) {
        self.text = (!text.is_empty()).then(|| text.to_string());
    }

    fn clear(&mut self) -> Option<String> {
        self.text.take()
    }

    fn preedit(&self) -> Option<&str> {
        self.text.as_deref()
    }

    fn is_active(&self) -> bool {
        self.text.is_some()
    }

    fn utf16_len(&self) -> usize {
        self.text
            .as_ref()
            .map(|text| text.encode_utf16().count())
            .unwrap_or(0)
    }
}

fn encode_key(key: &str, control: bool) -> Option<Vec<u8>> {
    if control {
        if key.len() == 1 {
            let byte = key.as_bytes()[0].to_ascii_lowercase();
            if byte.is_ascii_alphabetic() {
                return Some(vec![byte - b'a' + 1]);
            }
        }
        if key == "[" {
            return Some(vec![0x1b]);
        }
    }
    let bytes: &[u8] = match key {
        "enter" => b"\r",
        "backspace" => b"\x7f",
        "tab" => b"\t",
        "escape" => b"\x1b",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "delete" => b"\x1b[3~",
        "insert" => b"\x1b[2~",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        // Printable text arrives through the input handler so an IME can
        // compose it first; sending it here would duplicate every character.
        _ => return None,
    };
    Some(bytes.to_vec())
}

fn spawn_worker(target: Target, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let cancel = CancelToken::new();
        type Opened = (SessionHandle, Box<dyn Session + Send>, Option<String>);
        let opened: Result<Opened, (String, Option<String>)> = match target {
            Target::Local => LocalSession::open(None, GEOMETRY)
                .map(|session| {
                    let handle = session.handle();
                    (handle, Box::new(session) as Box<dyn Session + Send>, None)
                })
                .map_err(|err| (err.to_string(), None)),
            Target::Remote { host, policy } => {
                SshClient::connect(&host, policy, GEOMETRY, &cancel, None)
                    .and_then(|outcome| match outcome.session.handle() {
                        Some(handle) => Ok((
                            handle,
                            Box::new(outcome.session) as Box<dyn Session + Send>,
                            Some(outcome.fingerprint),
                        )),
                        None => Err(TerminalError::Channel("终端会话不可写".into())),
                    })
                    .map_err(|err| match &err {
                        TerminalError::HostKeyUnknown { fingerprint } => {
                            (err.to_string(), Some(fingerprint.clone()))
                        }
                        other => (other.to_string(), None),
                    })
            }
        };

        match opened {
            Err((message, Some(fingerprint))) => {
                let _ = tx.send(Event::HostKeyUnknown {
                    fingerprint,
                    message,
                });
            }
            Err((message, None)) => {
                let _ = tx.send(Event::Failed(message));
            }
            Ok((handle, mut session, fingerprint)) => {
                if tx
                    .send(Event::Opened {
                        handle,
                        fingerprint,
                    })
                    .is_err()
                {
                    return;
                }
                let mut buffer = vec![0u8; 32 * 1024];
                loop {
                    match session.read(&mut buffer) {
                        Ok(0) => {
                            let _ = tx.send(Event::Closed("会话已结束".into()));
                            break;
                        }
                        Ok(count) => {
                            if tx.send(Event::Data(buffer[..count].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            let _ = tx.send(Event::Closed(err.to_string()));
                            break;
                        }
                    }
                }
            }
        }
    });
}

struct ScreenElement {
    page: Entity<TerminalPage>,
    rows: usize,
    scroll_offset: usize,
    focused: bool,
}

struct Prepaint {
    lines: Vec<ShapedLine>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for ScreenElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ScreenElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * self.rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let mut font = style.font();
        font.family = "Menlo".into();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let mut lines = Vec::with_capacity(self.rows);
        let mut cursor = None;
        let screen = self.page.read(cx).screen.clone();
        for row in 0..self.rows {
            let (text, runs) = screen.row(row);
            let runs: Vec<TextRun> = runs
                .into_iter()
                .map(|(range, cell)| TextRun {
                    len: range.len(),
                    font: font.clone(),
                    color: cell_color(cell.fg),
                    background_color: (cell.bg != Color::Default).then(|| cell_color(cell.bg)),
                    underline: None,
                    strikethrough: None,
                })
                .collect();
            let shaped =
                window
                    .text_system()
                    .shape_line(SharedString::from(text), font_size, &runs, None);
            if self.focused && self.scroll_offset == 0 {
                let (row_index, col) = screen.cursor();
                if row_index == row {
                    let x = shaped.x_for_index(byte_index(&screen, row, col));
                    cursor = Some(fill(
                        Bounds::new(
                            point(
                                bounds.left() + x,
                                bounds.top() + window.line_height() * row as f32,
                            ),
                            size(px(8.0), window.line_height()),
                        ),
                        rgb(0xd0d7de),
                    ));
                }
            }
            lines.push(shaped);
        }
        Prepaint { lines, cursor }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.page.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.page.clone()),
            cx,
        );
        let line_height = window.line_height();
        for (row, line) in prepaint.lines.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * row as f32);
            let _ = line.paint(origin, line_height, window, cx);
        }
        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }
    }
}

fn byte_index(screen: &Screen, row: usize, col: usize) -> usize {
    let (text, _) = screen.row(row);
    text.char_indices()
        .nth(col)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn cell_color(color: Color) -> gpui::Hsla {
    match color {
        Color::Default => rgb(0xd0d7de).into(),
        Color::Indexed(index) => rgb(palette(index)).into(),
        Color::Rgb(r, g, b) => rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into(),
    }
}

fn palette(index: u8) -> u32 {
    const ANSI: [u32; 16] = [
        0x1b1f23, 0xe06c75, 0x98c379, 0xd19a66, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf, 0x5c6370,
        0xe06c75, 0x98c379, 0xd19a66, 0x61afef, 0xc678dd, 0x56b6c2, 0xffffff,
    ];
    if index < 16 {
        return ANSI[index as usize];
    }
    if index >= 232 {
        let level = 8 + (index as u32 - 232) * 10;
        return (level << 16) | (level << 8) | level;
    }
    let index = index as u32 - 16;
    let steps = [0u32, 95, 135, 175, 215, 255];
    let r = steps[(index / 36) as usize];
    let g = steps[((index % 36) / 6) as usize];
    let b = steps[(index % 6) as usize];
    (r << 16) | (g << 8) | b
}

impl Focusable for TerminalPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for TerminalPage {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.composing.preedit().unwrap_or_default().to_string();
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let start = range_utf16.start.min(utf16.len());
        let end = range_utf16.end.min(utf16.len());
        actual_range.replace(start..end);
        String::from_utf16(&utf16[start..end]).ok()
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.composing.utf16_len();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.composing
            .is_active()
            .then(|| 0..self.composing.utf16_len())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.composing.clear().is_some() {
            cx.notify();
        }
    }

    /// A commit, not a keystroke: this is the only path that reaches the PTY.
    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composing.clear();
        if !new_text.is_empty() {
            self.send(new_text.as_bytes(), cx);
        }
        cx.notify();
    }

    /// Composition updates stay local until the IME commits.
    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composing.mark(new_text);
        cx.notify();
    }

    /// Anchors the candidate window on the terminal cursor cell.
    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (row, col) = self.screen.cursor();
        let cell = bounds.size.width / self.screen.cols() as f32;
        Some(Bounds::new(
            point(
                bounds.left() + cell * col as f32,
                bounds.top() + window.line_height() * row as f32,
            ),
            size(cell, window.line_height()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

impl Render for TerminalPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .size_full()
            .bg(theme.canvas)
            .child(self.rail(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(if self.form_open || self.session.is_some() {
                        self.header(cx).into_any_element()
                    } else {
                        self.hosts_page(cx).into_any_element()
                    })
                    .child(
                        if self.form_open {
                            self.host_form(cx).into_any_element()
                        } else if self.session.is_some() {
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_h(px(0.0))
                                .child(self.terminal_area(cx))
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        },
                    ),
            )
    }
}

fn group_label(group: &str) -> String {
    if group.trim().is_empty() {
        "默认分组".into()
    } else {
        group.trim().to_string()
    }
}

fn protocol_badge(theme: Theme) -> impl IntoElement {
    div()
        .px_1()
        .rounded_sm()
        .bg(theme.accent_soft)
        .text_size(px(10.0))
        .text_color(theme.accent)
        .child("SSH")
}
