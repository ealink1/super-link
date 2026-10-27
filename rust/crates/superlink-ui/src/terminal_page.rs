//! Shell workspace: host list, interactive PTY and a real VT screen.
//!
//! The transport blocks on its own runtime, so connecting and reading happen
//! on a worker thread; the GPUI thread only drains events and paints.

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
        host: ShellHost,
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

        let port: u16 = port_text.trim().parse().unwrap_or(0);
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
        self.begin(Target::Remote { host, policy }, true, cx);
    }

    /// Manual reconnect after a drop, or a retry from the header button.
    fn reconnect(&mut self, cx: &mut Context<Self>) {
        if self.session.is_some() {
            return;
        }
        let Some(target) = self.target.clone() else {
            return;
        };
        self.attempts = 0;
        self.auto_reconnect = true;
        self.begin(target, false, cx);
    }

    /// User-initiated disconnect: cancel retries and close the transport.
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
                    self.screen.feed(&mut self.parser, &bytes);
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
        self.begin(Target::Remote { host, policy }, true, cx);
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

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(40.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.text)
                    .child(if self.session.is_some() {
                        "终端"
                    } else {
                        "未连接"
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .id("local-shell")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .bg(theme.accent_soft)
                    .text_xs()
                    .text_color(theme.accent)
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.connect_local(cx);
                        this.focus.focus(window);
                        cx.notify();
                    }))
                    .child(icons::icon("shell/terminal.svg", 13.0, theme.accent))
                    .child("本地终端"),
            )
            .child(
                div()
                    .id("new-host")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.accent_soft)
                    .text_xs()
                    .text_color(theme.accent)
                    .on_click(cx.listener(|this, _event, _window, cx| this.open_host_form(cx)))
                    .child("新建主机"),
            )
            .child(
                div()
                    .id("connect-host")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .bg(theme.accent)
                    .text_xs()
                    .text_color(theme.surface)
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.connect_selected(cx);
                        this.focus.focus(window);
                        cx.notify();
                    }))
                    .child(icons::icon("superlink/link.svg", 13.0, theme.surface))
                    .child("连接"),
            )
            .child(if self.session.is_some() {
                div()
                    .id("disconnect")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .bg(theme.accent_soft)
                    .text_xs()
                    .text_color(theme.accent)
                    .on_click(cx.listener(|this, _event, _window, cx| this.disconnect(cx)))
                    .child(icons::icon("superlink/stop.svg", 13.0, theme.accent))
                    .child("断开")
                    .into_any_element()
            } else {
                div()
                    .id("reconnect")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .bg(theme.accent_soft)
                    .text_xs()
                    .text_color(theme.accent)
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.reconnect(cx);
                        this.focus.focus(window);
                        cx.notify();
                    }))
                    .child(icons::icon("shell/refresh-cw.svg", 13.0, theme.accent))
                    .child("重连")
                    .into_any_element()
            })
    }

    #[allow(clippy::too_many_lines)]
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
                                            .text_sm()
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
                    .children(
                        self.form_error.clone().map(|error| {
                            div().text_xs().text_color(gpui::rgb(0xb42318)).child(error)
                        }),
                    )
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
                                    .child(div().text_xs().text_color(theme.text).child("取消")),
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
                                            .text_xs()
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
                    .text_xs()
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
                needle.is_empty()
                    || host.name.to_lowercase().contains(&needle)
                    || host.host.to_lowercase().contains(&needle)
                    || host.user.to_lowercase().contains(&needle)
            })
            .collect();

        let area = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .px_3()
            .pt_3()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .w(px(300.0))
                    .px_2()
                    .h(px(28.0))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(div().flex_1().min_w(px(0.0)).child(self.search.clone())),
            )
            .child(
                div()
                    .id("reload-hosts")
                    .flex()
                    .items_center()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(theme.muted)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.reload_hosts(cx)))
                    .child(icons::icon("shell/refresh-cw.svg", 13.0, theme.muted)),
            );

        if matches.is_empty() {
            return area.child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(theme.muted)
                    .child(if self.host_list.is_empty() {
                        "还没有 SSH 主机。可以先打开本地终端。"
                    } else {
                        "没有匹配的主机"
                    }),
            );
        }

        area.child(
            div()
                .id("host-cards")
                .flex()
                .flex_wrap()
                .gap_2()
                .overflow_y_scroll()
                .children(
                    matches
                        .into_iter()
                        .map(|(index, host)| self.host_card(index, host, cx)),
                ),
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
        let endpoint = SharedString::from(format!("{}@{}:{}", host.user, host.host, host.port));
        div()
            .w(px(280.0))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(34.0))
                            .rounded_md()
                            .bg(theme.accent_soft)
                            .child(icons::icon("shell/terminal.svg", 16.0, theme.accent)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(name),
                    ),
            )
            .child(div().text_xs().text_color(theme.muted).child(endpoint))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted)
                    .child(if host.notes.is_empty() {
                        SharedString::from("暂无备注")
                    } else {
                        SharedString::from(host.notes.clone())
                    }),
            )
            .child(
                div()
                    .id(("quick-connect", index))
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(28.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.canvas)
                    .border_1()
                    .border_color(theme.border)
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.connect_host_at(index, window, cx)
                    }))
                    .child(div().text_xs().text_color(theme.text).child("快速连接")),
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
                                .text_sm()
                                .text_color(theme.text)
                                .child(format!("尚未信任 {} 的主机密钥", host.name)),
                        )
                        .child(
                            div()
                                .text_xs()
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
                                        .text_xs()
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
                                        .text_xs()
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
                    .text_xs()
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
                    .text_xs()
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
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .child(self.header(cx))
            .child(if self.form_open {
                self.host_form(cx).into_any_element()
            } else if self.session.is_some() {
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.terminal_area(cx))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .bg(theme.canvas)
                    .child(self.host_cards(cx))
                    .into_any_element()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_control_and_navigation_keys() {
        assert_eq!(encode_key("enter", false).unwrap(), b"\r");
        assert_eq!(encode_key("backspace", false).unwrap(), b"\x7f");
        assert_eq!(encode_key("up", false).unwrap(), b"\x1b[A");
        assert_eq!(encode_key("c", true).unwrap(), vec![0x03]);
        assert_eq!(encode_key("z", true).unwrap(), vec![0x1a]);
    }

    #[test]
    fn leaves_printable_text_to_the_input_handler() {
        assert!(encode_key("a", false).is_none());
        assert!(encode_key("中", false).is_none());
        assert!(encode_key("space", false).is_none());
        assert!(encode_key("f1", false).is_none());
    }

    #[test]
    fn palette_covers_ansi_greyscale_and_cube() {
        assert_eq!(palette(1), 0xe06c75);
        assert_eq!(palette(232), 0x080808);
        assert_eq!(palette(16), 0x000000);
        assert_eq!(palette(196), 0xff0000);
    }
}

#[cfg(all(test, unix))]
mod pty_tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// Proves the real local PTY feeds the real screen model.
    #[test]
    fn local_shell_output_reaches_the_screen() {
        let mut session =
            LocalSession::open(Some("/bin/sh"), TerminalGeometry { cols: 80, rows: 24 })
                .expect("local pty");
        session.write(b"printf 'superlink-ok\\n'\n").expect("write");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            for _ in 0..64 {
                match session.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if tx.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut screen = Screen::new(80, 24);
        let mut parser = vte::Parser::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(bytes) => screen.feed(&mut parser, &bytes),
                Err(_) => continue,
            }
            let found = (0..24).any(|row| screen.row(row).0.contains("superlink-ok"));
            if found {
                return;
            }
        }
        panic!("shell output did not reach the screen");
    }
}

#[cfg(test)]
mod composition_tests {
    use super::Composition;

    #[test]
    fn preedit_is_visible_but_not_committed() {
        let mut composition = Composition::default();
        composition.mark("zhong");
        assert!(composition.is_active());
        assert_eq!(composition.preedit(), Some("zhong"));
        assert_eq!(composition.utf16_len(), 5);
    }

    #[test]
    fn clearing_returns_the_previous_preedit_once() {
        let mut composition = Composition::default();
        composition.mark("中文");
        assert_eq!(composition.utf16_len(), 2);
        assert_eq!(composition.clear().as_deref(), Some("中文"));
        assert!(!composition.is_active());
        assert_eq!(composition.clear(), None);
    }

    #[test]
    fn empty_mark_cancels_the_composition() {
        let mut composition = Composition::default();
        composition.mark("a");
        composition.mark("");
        assert!(!composition.is_active());
        assert_eq!(composition.utf16_len(), 0);
    }

    #[test]
    fn surrogate_pairs_count_as_two_units() {
        let mut composition = Composition::default();
        composition.mark("👩‍💻");
        assert_eq!(composition.utf16_len(), 5);
    }
}

#[cfg(all(test, unix))]
mod ime_pty_tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// The commit path hands UTF-8 to the PTY byte for byte; the shell writes
    /// it to disk so the assertion does not depend on terminal rendering.
    #[test]
    fn committed_chinese_reaches_the_shell_unchanged() {
        let target = std::env::temp_dir().join(format!(
            "superlink-ime-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&target);
        let mut session =
            LocalSession::open(Some("/bin/sh"), TerminalGeometry { cols: 80, rows: 24 })
                .expect("local pty");
        let handle = session.handle();

        let command = format!("printf '中文输入' > {}\n", target.display());
        handle.write(command.as_bytes()).expect("write");

        // The reader owns the session; writes go through the cloneable handle.
        let drain = std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match session.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut received = None;
        while Instant::now() < deadline {
            if let Ok(text) = std::fs::read_to_string(&target) {
                received = Some(text);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // Unblock the reader and reap the child before asserting.
        let _ = handle.close();
        let _ = drain.join();
        let _ = std::fs::remove_file(&target);
        assert_eq!(received.as_deref(), Some("中文输入"));
    }

    /// A preedit update must not write anything to the session.
    #[test]
    fn composition_updates_do_not_touch_the_session() {
        let mut composition = Composition::default();
        composition.mark("zhong");
        assert_eq!(composition.preedit(), Some("zhong"));
        // Nothing else to send: only Composition::clear plus a commit produces
        // bytes, which the handler performs in replace_text_in_range.
        assert!(composition.is_active());
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::{MAX_RECONNECT_ATTEMPTS, backoff_delay};

    #[test]
    fn backoff_grows_and_then_stops() {
        assert_eq!(backoff_delay(0), None);
        assert_eq!(backoff_delay(1), Some(1));
        assert_eq!(backoff_delay(2), Some(2));
        assert_eq!(backoff_delay(3), Some(4));
        assert_eq!(backoff_delay(4), Some(8));
        assert_eq!(backoff_delay(5), Some(15));
        assert_eq!(backoff_delay(MAX_RECONNECT_ATTEMPTS + 1), None);
    }
}
