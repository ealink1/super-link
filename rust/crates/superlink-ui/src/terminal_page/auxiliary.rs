mod file_view;
mod monitor;
mod part_aux_button;
mod part_files_panel;
use super::*;
use gpui::AnyElement;
use superlink_terminal::{LocalMonitor, Metrics, RemoteFile, SftpClient, TransferOptions};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Terminal,
    Files,
    Monitor,
    Settings,
}
pub(super) struct Auxiliary {
    pub(in crate::terminal_page) monitor: monitor::State,
    pub mode: Mode,
    pub(in crate::terminal_page) collapsed: Option<Mode>,
    pub cancel: CancelToken,
    path: Entity<SqlEditor>,
    name: Entity<SqlEditor>,
    pub(in crate::terminal_page) files: Vec<RemoteFile>,
    pub(in crate::terminal_page) selected: Option<String>,
    pub(in crate::terminal_page) busy: bool,
    message: String,
    pub(in crate::terminal_page) metrics: Option<Metrics>,
    local: Option<Arc<std::sync::Mutex<LocalMonitor>>>,
    progress: Arc<std::sync::atomic::AtomicU64>,
    remove_confirm: bool,
    page: usize,
    filter: Entity<SqlEditor>,
    filter_text: String,
    show_hidden: bool,
    loaded_path: String,
    file_job: u64,
    pub(in crate::terminal_page) monitor_sampling: bool,
    monitor_job: u64,
}
impl Auxiliary {
    pub fn new(cx: &mut Context<TerminalPage>) -> Self {
        let path = cx.new(SqlEditor::new);
        path.update(cx, |e, cx| e.set_text(".", cx));
        let name = cx.new(SqlEditor::new);
        name.update(cx, |e, cx| e.set_placeholder("新名称 / 八进制权限", cx));
        let filter = cx.new(SqlEditor::new);
        let weak = cx.entity().downgrade();
        filter.update(cx, |e, cx| {
            e.set_placeholder("搜索当前目录…", cx);
            e.set_on_change(
                move |text, app| {
                    let _ = weak.update(app, |page, cx| {
                        page.auxiliary.filter_text = text.to_string();
                        page.auxiliary.page = 0;
                        page.auxiliary.selected = None;
                        cx.notify();
                    });
                },
                cx,
            );
        });
        Self {
            filter,
            filter_text: String::new(),
            show_hidden: true,
            loaded_path: String::new(),
            file_job: 0,
            monitor_sampling: false,
            monitor_job: 0,
            monitor: monitor::State::new(cx),
            mode: Mode::Terminal,
            collapsed: None,
            cancel: CancelToken::new(),
            path,
            name,
            files: Vec::new(),
            selected: None,
            busy: false,
            message: String::new(),
            metrics: None,
            local: None,
            progress: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            remove_confirm: false,
            page: 0,
        }
    }
}
enum FileResult {
    List(String, Vec<RemoteFile>),
    Done(String),
}
