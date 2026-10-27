//! Shell workspace: host list, interactive PTY and a real VT screen.
//!
//! The transport blocks on its own runtime, so connecting and reading happen
//! on a worker thread; the GPUI thread only drains events and paints.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    App, Bounds, Context, CursorStyle, Element, ElementId, Entity, FocusHandle, Focusable,
    GlobalElementId, InspectorElementId, KeyDownEvent, LayoutId, MouseButton, PaintQuad, Pixels,
    Render, SharedString, ShapedLine, Style, TextRun, Window, div, fill, point, prelude::*, px,
    relative, rgb, size,
};
use superlink_app::HostService;
use superlink_domain::ShellHost;
use superlink_terminal::{
    CancelToken, HostKeyPolicy, LocalSession, SshClient, TerminalGeometry, TerminalError,
};

use superlink_terminal::Session;

use crate::theme::Theme;
use crate::vt::{Color, Screen};

const GEOMETRY: TerminalGeometry = TerminalGeometry { cols: 100, rows: 28 };
const POLL_INTERVAL: Duration = Duration::from_millis(16);

type SessionRef = Arc<Mutex<Box<dyn Session + Send>>>;

enum Event {
    Opened { session: SessionRef, fingerprint: Option<String> },
    Data(Vec<u8>),
    Closed(String),
    Failed(String),
    HostKeyUnknown { fingerprint: String, message: String },
}

#[derive(Clone)]
enum Target {
    Local,
    Remote { host: ShellHost, policy: HostKeyPolicy },
}

pub struct TerminalPage {
    hosts: Arc<HostService>,
    theme: Theme,
    host_list: Vec<ShellHost>,
    selected: usize,
    status: SharedString,
    session: Option<SessionRef>,
    pending: Option<(ShellHost, String)>,
    screen: Screen,
    parser: vte::Parser,
    focus: FocusHandle,
    running: Arc<AtomicBool>,
    scroll_offset: usize,
    error: Option<SharedString>,
}

impl TerminalPage {
    pub fn new(hosts: Arc<HostService>, theme: Theme, cx: &mut Context<Self>) -> Self {
        let host_list = hosts.list().unwrap_or_default();
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
            error: None,
        }
    }

    fn reload_hosts(&mut self, cx: &mut Context<Self>) {
        self.host_list = self.hosts.list().unwrap_or_default();
        if self.selected >= self.host_list.len() {
            self.selected = self.host_list.len().saturating_sub(1);
        }
        cx.notify();
    }

    fn connect_local(&mut self, cx: &mut Context<Self>) {
        self.start(Target::Local, cx);
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
        self.start(Target::Remote { host, policy }, cx);
    }

    fn start(&mut self, target: Target, cx: &mut Context<Self>) {
        if self.session.is_some() {
            return;
        }
        self.error = None;
        self.pending = None;
        self.status = "连接中…".into();
        self.screen = Screen::new(GEOMETRY.cols as usize, GEOMETRY.rows as usize);
        self.scroll_offset = 0;
        cx.notify();

        let (tx, rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        self.running = running.clone();
        spawn_worker(target, tx);
        self.poll(rx, running, cx);
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
                Event::Opened { session, fingerprint } => {
                    self.session = Some(session);
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
                    self.status = SharedString::from(message);
                }
                Event::Failed(message) => {
                    self.session = None;
                    self.running.store(false, Ordering::SeqCst);
                    self.status = "连接失败".into();
                    self.error = Some(message.into());
                }
                Event::HostKeyUnknown { fingerprint, message } => {
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
        self.start(Target::Remote { host, policy }, cx);
    }

    fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let result = session
            .lock()
            .map_err(|_| "会话不可用".to_string())
            .and_then(|mut guard| guard.write(bytes).map_err(|err| err.to_string()));
        if let Err(message) = result {
            self.error = Some(message.into());
            cx.notify();
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
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
                if let Some(session) = self.session.clone() {
                    if let Ok(mut guard) = session.lock() {
                        let _ = guard.write(&[0x03]);
                    }
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
                    .child(if self.session.is_some() { "终端" } else { "未连接" }),
            )
            .child(div().flex_1())
            .child(div().text_xs().text_color(theme.muted).child(self.status.clone()))
            .child(
                div()
                    .id("local-shell")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.accent_soft)
                    .text_xs()
                    .text_color(theme.accent)
                    .on_click(cx.listener(|this, _event, _window, cx| this.connect_local(cx)))
                    .child("本地终端"),
            )
            .child(
                div()
                    .id("connect-host")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(theme.accent)
                    .text_xs()
                    .text_color(theme.surface)
                    .on_click(cx.listener(|this, _event, _window, cx| this.connect_selected(cx)))
                    .child("连接"),
            )
    }

    fn host_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut column = div()
            .flex()
            .flex_col()
            .w(px(220.0))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .h(px(40.0))
                    .child(div().text_xs().text_color(theme.muted).child("主机"))
                    .child(
                        div()
                            .id("reload-hosts")
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.muted)
                            .on_click(cx.listener(|this, _event, _window, cx| this.reload_hosts(cx)))
                            .child(format!("{}", self.host_list.len())),
                    ),
            );
        if self.host_list.is_empty() {
            column = column.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(theme.muted)
                    .child("还没有 SSH 主机。可以先打开本地终端。"),
            );
        }
        for (index, host) in self.host_list.iter().enumerate() {
            let active = index == self.selected;
            let label = format!("{}@{}", host.user, host.host);
            column = column.child(
                div()
                    .id(("host", index))
                    .mx_2()
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.accent_soft } else { theme.sidebar })
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.selected = index;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_sm()
                            .text_color(if active { theme.accent } else { theme.text })
                            .child(SharedString::from(host.name.clone())),
                    )
                    .child(div().text_xs().text_color(theme.muted).child(label)),
            );
        }
        column
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
            area = area.child(
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

        area.child(ScreenElement {
            page: cx.entity(),
            rows: GEOMETRY.rows as usize,
            scroll_offset: self.scroll_offset,
            focused: self.session.is_some(),
        })
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
        "space" => b" ",
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
        _ => {
            if key.chars().count() == 1 {
                return Some(key.as_bytes().to_vec());
            }
            return None;
        }
    };
    Some(bytes.to_vec())
}

fn spawn_worker(target: Target, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let cancel = CancelToken::new();
        let opened: Result<(SessionRef, Option<String>), (String, Option<String>)> = match target {
            Target::Local => LocalSession::open(None, GEOMETRY)
                .map(|session| (Arc::new(Mutex::new(Box::new(session) as Box<dyn Session + Send>)), None))
                .map_err(|err| (err.to_string(), None)),
            Target::Remote { host, policy } => SshClient::connect(&host, policy, GEOMETRY, &cancel, None)
                .map(|outcome| {
                    let client = outcome.client;
                    let session: SessionRef = Arc::new(Mutex::new(Box::new(outcome.session) as Box<dyn Session + Send>));
                    // Keep the transport alive for the lifetime of this thread.
                    let _client = client;
                    (session, Some(outcome.fingerprint))
                })
                .map_err(|err| match &err {
                    TerminalError::HostKeyUnknown { fingerprint } => {
                        (err.to_string(), Some(fingerprint.clone()))
                    }
                    other => (other.to_string(), None),
                }),
        };

        match opened {
            Err((message, Some(fingerprint))) => {
                let _ = tx.send(Event::HostKeyUnknown { fingerprint, message });
            }
            Err((message, None)) => {
                let _ = tx.send(Event::Failed(message));
            }
            Ok((session, fingerprint)) => {
                if tx.send(Event::Opened { session: session.clone(), fingerprint }).is_err() {
                    return;
                }
                let mut buffer = vec![0u8; 32 * 1024];
                loop {
                    let read = {
                        let mut guard = match session.lock() {
                            Ok(guard) => guard,
                            Err(_) => break,
                        };
                        guard.read(&mut buffer)
                    };
                    match read {
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
            let shaped = window
                .text_system()
                .shape_line(SharedString::from(text), font_size, &runs, None);
            if self.focused && self.scroll_offset == 0 {
                let (row_index, col) = screen.cursor();
                if row_index == row {
                    let x = shaped.x_for_index(byte_index(&screen, row, col));
                    cursor = Some(fill(
                        Bounds::new(
                            point(bounds.left() + x, bounds.top() + window.line_height() * row as f32),
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

impl Render for TerminalPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.canvas)
            .child(self.header(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.host_list(cx))
                    .child(self.terminal_area(cx)),
            )
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
    fn passes_printable_and_multibyte_text() {
        assert_eq!(encode_key("a", false).unwrap(), b"a");
        assert_eq!(encode_key("中", false).unwrap(), "中".as_bytes());
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
        let mut session = LocalSession::open(Some("/bin/sh"), TerminalGeometry { cols: 80, rows: 24 })
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
