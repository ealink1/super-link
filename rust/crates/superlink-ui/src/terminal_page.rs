//! Shell workspace: host list, interactive PTY and a real VT screen.
//!
//! The transport blocks on its own runtime, so connecting and reading happen
//! on a worker thread; the GPUI thread only drains events and paints.

mod connection_dialog;
mod events;
mod host_actions;
mod host_cards;
mod input;
mod keyboard;
mod part_host_form;
mod part_new;
mod part_rail_button;
mod screen_element;
mod session_chrome;
use screen_element::ScreenElement;
mod terminal_view;
use session_chrome::this_progress;

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

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

/// 侧栏开合用 smoothstep 缓动，起止都不突兀。
fn panel_ease(progress: f32) -> f32 {
    let progress = progress.clamp(0., 1.);
    progress * progress * (3. - 2. * progress)
}

enum Event {
    HostKeyChanged {
        expected: String,
        observed: String,
    },
    Progress(superlink_terminal::ConnectEvent),
    Opened {
        handle: SessionHandle,
        fingerprint: Option<String>,
        client: Option<Arc<SshClient>>,
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
    Local(Option<String>),
    Remote {
        host: Box<ShellHost>,
        policy: HostKeyPolicy,
    },
}

type OpenHandler = Box<dyn Fn(Option<ShellHost>, &mut App)>;

mod auxiliary;
mod management;
mod selection;

type BackHandler = Box<dyn Fn(&mut App)>;
pub struct TerminalPage {
    connect_dialog: bool,
    connect_log: Vec<superlink_terminal::ConnectEvent>,
    changed_key: Option<(String, String)>,
    needs_focus: bool,
    connecting: bool,
    connect_steps: [Option<superlink_terminal::ConnectState>; 4],
    host_menu: Option<usize>,
    tag: String,
    form_remember: bool,
    form_key_mode: bool,
    form_system: String,
    form_busy: bool,
    form_generation: u64,
    form_cancel: CancelToken,
    probe_pending: Option<(String, u16, String, String)>,
    open_handler: Option<OpenHandler>,
    back_handler: Option<BackHandler>,
    connect_cancel: CancelToken,
    transport: Option<Arc<SshClient>>,
    auxiliary: auxiliary::Auxiliary,
    editing_host: Option<ShellHost>,
    confirm_host_delete: bool,
    screen_bounds: Option<Bounds<Pixels>>,
    screen_lines: Vec<ShapedLine>,
    write_queue: Option<SyncSender<Vec<u8>>>,
    selection: Option<((usize, usize), (usize, usize))>,
    selecting_text: bool,
    settings: Arc<superlink_app::SettingsService>,
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
    form_group: Entity<SqlEditor>,
    form_tags: Entity<SqlEditor>,
    form_key_path: Entity<SqlEditor>,
    form_passphrase: Entity<SqlEditor>,
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
    notice: Option<SharedString>,
    /// 侧栏滑动动画：起点、目标与起始时刻，0 收起、1 展开。
    panel_from: f32,
    panel_target: f32,
    panel_start: Option<Instant>,
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

fn spawn_worker(
    target: Target,
    tx: SyncSender<Event>,
    cancel: CancelToken,
    hosts: Arc<HostService>,
) {
    std::thread::spawn(move || {
        type Opened = (
            SessionHandle,
            Box<dyn Session + Send>,
            Option<String>,
            Option<Arc<SshClient>>,
        );
        let opened: Result<Opened, TerminalError> = match target {
            Target::Local(program) => {
                LocalSession::open(program.as_deref(), GEOMETRY).map(|session| {
                    let handle = session.handle();
                    (
                        handle,
                        Box::new(session) as Box<dyn Session + Send>,
                        None,
                        None,
                    )
                })
            }
            Target::Remote { mut host, policy } => {
                match hosts.credentials(&host) {
                    Ok(Some(credentials)) => {
                        host.password = credentials.password;
                        host.passphrase = credentials.passphrase;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = tx.send(Event::Failed(error.to_string()));
                        return;
                    }
                }
                let mut progress = |event| {
                    let _ = tx.try_send(Event::Progress(event));
                };
                SshClient::connect(&host, policy, GEOMETRY, &cancel, Some(&mut progress)).and_then(
                    |outcome| match outcome.session.handle() {
                        Some(handle) => Ok((
                            handle,
                            Box::new(outcome.session) as Box<dyn Session + Send>,
                            Some(outcome.fingerprint),
                            Some(Arc::new(outcome.client)),
                        )),
                        None => Err(TerminalError::Channel("终端会话不可写".into())),
                    },
                )
            }
        };

        match opened {
            Err(TerminalError::HostKeyUnknown { fingerprint }) => {
                let message = "首次连接需要核实 SSH 主机指纹".to_string();
                let _ = tx.send(Event::HostKeyUnknown {
                    fingerprint,
                    message,
                });
            }
            Err(TerminalError::HostKeyChanged { expected, observed }) => {
                let _ = tx.send(Event::HostKeyChanged { expected, observed });
            }
            Err(error) => {
                let _ = tx.send(Event::Failed(error.to_string()));
            }
            Ok((handle, mut session, fingerprint, client)) => {
                if cancel.is_cancelled() {
                    let _ = handle.close();
                    return;
                }
                if tx
                    .send(Event::Opened {
                        handle,
                        fingerprint,
                        client,
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_focus {
            self.focus.focus(window);
            self.needs_focus = false;
        }
        let theme = self.theme;
        let has_terminal = self.target.is_some();
        let panel_open = self.panel_progress();
        let animating = self.panel_start.is_some();
        if animating {
            // 让 GPUI 在下一个显示帧再重绘一次，进度由 elapsed 连续插值。
            window.request_animation_frame();
        }
        let content = if self.form_open {
            self.host_form(cx).into_any_element()
        } else if has_terminal {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.))
                .child(
                    div()
                        .flex()
                        .relative()
                        .flex_1()
                        .min_h(px(0.))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w(px(0.))
                                .child(self.terminal_area(animating, cx)),
                        )
                        .when(self.auxiliary.mode != auxiliary::Mode::Terminal, |body| {
                            body.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w(relative(0.32 * panel_open))
                                    .min_w(px(320. * panel_open))
                                    .max_w(px(420.))
                                    .min_h(px(0.))
                                    .overflow_hidden()
                                    .when(panel_open > 0.02, |panel| {
                                        panel.border_l_1().border_color(theme.border)
                                    })
                                    .when(
                                        self.auxiliary.mode != auxiliary::Mode::Monitor,
                                        |panel| panel.child(self.auxiliary_tabs(cx)),
                                    )
                                    .child(self.auxiliary_panel(cx)),
                            )
                        })
                        .children(self.auxiliary.collapsed.is_some().then(|| {
                            div()
                                .id("restore-shell-side-panel")
                                .absolute()
                                .right_0()
                                .top(relative(0.45))
                                .w(px(24.))
                                .h(px(64.))
                                .opacity(1. - panel_open)
                                .bg(theme.button)
                                .rounded_md()
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icons::icon("shell/panel-left-close.svg", 14., theme.muted))
                                .on_click(cx.listener(|this, _, _, cx| this.restore_panel(cx)))
                        })),
                )
                .child(self.session_footer(cx))
                .into_any_element()
        } else if self.auxiliary.mode == auxiliary::Mode::Settings {
            self.auxiliary_panel(cx)
        } else {
            self.hosts_page(cx).into_any_element()
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .child(content)
            .children(self.connect_dialog.then(|| self.connection_progress(cx)))
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
