//! Note workspace: Fyne-parity sidebar, group tree, and a Markdown reading view.
//!
//! The notebook snapshot is encrypted; this view reads it through the
//! application service and renders bounded Markdown blocks. Editing needs a
//! multi-line editor and stays on the follow-up list.

use std::collections::HashSet;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px,
};
use superlink_app::NotebookService;
use superlink_domain::{Note, Notebook};

use crate::editor::SqlEditor;
use crate::icons;
use crate::theme::Theme;

const PREVIEW_BYTES: usize = 8 << 10;
const SIDEBAR_WIDTH: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteView {
    Read,
    Trash,
}

pub struct NotePage {
    service: Arc<NotebookService>,
    theme: Theme,
    notebook: Notebook,
    search: Entity<SqlEditor>,
    query: String,
    selected: Option<SharedString>,
    view: NoteView,
    /// Empty string means "全部笔记".
    active_group: String,
    collapsed: HashSet<String>,
    notice: Option<SharedString>,
    error: Option<SharedString>,
}

impl NotePage {
    pub fn new(service: Arc<NotebookService>, theme: Theme, cx: &mut Context<Self>) -> Self {
        let (notebook, error) = match service.load() {
            Ok(notebook) => (notebook, None),
            Err(err) => (
                Notebook::default(),
                Some(SharedString::from(err.to_string())),
            ),
        };
        let page = cx.entity().downgrade();
        let search = cx.new(SqlEditor::new);
        search.update(cx, |editor, cx| {
            editor.set_placeholder("搜索笔记…", cx);
            editor.set_on_change(
                move |text, app| {
                    let page = page.clone();
                    let _ = page.update(app, |this, cx| {
                        this.query = text.to_string();
                        cx.notify();
                    });
                },
                cx,
            );
        });
        let selected = notebook
            .notes
            .iter()
            .find(|note| !note.deleted)
            .map(|note| SharedString::from(note.id.clone()));
        Self {
            service,
            theme,
            notebook,
            search,
            query: String::new(),
            selected,
            view: NoteView::Read,
            active_group: String::new(),
            collapsed: HashSet::new(),
            notice: None,
            error,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        match self.service.save(&self.notebook) {
            Ok(saved) => {
                self.notebook = saved;
                self.error = None;
                true
            }
            Err(err) => {
                self.error = Some(SharedString::from(err.to_string()));
                cx.notify();
                false
            }
        }
    }

    fn matches(&self, note: &Note) -> bool {
        let needle = self.query.to_lowercase();
        note.deleted == (self.view == NoteView::Trash)
            && (self.active_group.is_empty() || note.group_id == self.active_group)
            && (needle.is_empty()
                || note.title.to_lowercase().contains(&needle)
                || note.body.to_lowercase().contains(&needle))
    }

    fn group_name(&self, id: &str) -> String {
        if id.is_empty() {
            return "默认分组".into();
        }
        self.notebook
            .groups
            .iter()
            .find(|group| group.id == id)
            .map(|group| group.name.clone())
            .unwrap_or_else(|| "默认分组".into())
    }

    fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selected = Some(SharedString::from(id.to_string()));
        self.notice = None;
        cx.notify();
    }

    fn new_note(&mut self, cx: &mut Context<Self>) {
        let id = uuid::Uuid::new_v4().to_string();
        self.notebook.notes.push(Note {
            id: id.clone(),
            title: "无标题笔记".into(),
            group_id: self.active_group.clone(),
            updated_at: Some(chrono::Utc::now()),
            ..Note::default()
        });
        self.view = NoteView::Read;
        self.query.clear();
        self.search.update(cx, |editor, cx| editor.set_text("", cx));
        self.selected = Some(SharedString::from(id));
        if self.save(cx) {
            self.notice = Some(SharedString::from("已新建笔记"));
        }
        cx.notify();
    }

    fn toggle_trash(&mut self, cx: &mut Context<Self>) {
        self.view = if self.view == NoteView::Trash {
            NoteView::Read
        } else {
            NoteView::Trash
        };
        let first = self
            .notebook
            .notes
            .iter()
            .find(|note| note.deleted == (self.view == NoteView::Trash))
            .map(|note| note.id.clone());
        self.selected = first.map(SharedString::from);
        cx.notify();
    }

    fn set_deleted(&mut self, deleted: bool, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        if let Some(note) = self
            .notebook
            .notes
            .iter_mut()
            .find(|note| note.id.as_str() == id.as_str())
        {
            note.deleted = deleted;
            note.updated_at = Some(chrono::Utc::now());
        }
        self.save(cx);
        self.selected = None;
        cx.notify();
    }
}

impl NotePage {
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let total = self
            .notebook
            .notes
            .iter()
            .filter(|note| !note.deleted)
            .count();
        let visible: Vec<&Note> = self
            .notebook
            .notes
            .iter()
            .filter(|n| self.matches(n))
            .collect();

        let mut list = div()
            .id("note-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll();

        for (group_id, name) in self.group_list() {
            let key = if group_id.is_empty() {
                "default".to_string()
            } else {
                group_id.clone()
            };
            let collapsed = self.collapsed.contains(&key);
            let active = self.active_group == group_id;
            let count = visible.iter().filter(|n| n.group_id == group_id).count();
            let click_id = group_id.clone();
            let toggle_key = key.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("note-group-{key}")))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.0))
                    .px_3()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if this.active_group != click_id {
                            this.active_group = click_id.clone();
                        } else if this.collapsed.remove(&toggle_key) {
                        } else {
                            this.collapsed.insert(toggle_key.clone());
                        }
                        cx.notify();
                    }))
                    .child(icons::icon(
                        if collapsed {
                            "shell/chevron-right.svg"
                        } else {
                            "shell/chevron-down.svg"
                        },
                        13.0,
                        theme.muted,
                    ))
                    .child(icons::icon("shell/folder.svg", 15.0, theme.text))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.accent } else { theme.text })
                            .child(SharedString::from(name)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child(format!("{count}")),
                    ),
            );
            if collapsed {
                continue;
            }
            for note in visible.iter().filter(|note| note.group_id == group_id) {
                list = list.child(self.note_row(note, cx));
            }
        }

        div()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .px_3()
                    .pt_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(34.0))
                            .px_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .child(icons::icon("shell/search.svg", 14.0, theme.muted))
                            .child(div().flex_1().min_w(px(0.0)).child(self.search.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .mt_3()
                            .child(
                                div()
                                    .id("note-new")
                                    .flex()
                                    .flex_1()
                                    .items_center()
                                    .justify_center()
                                    .h(px(36.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .bg(theme.accent)
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.surface)
                                    .on_click(
                                        cx.listener(|this, _event, _window, cx| this.new_note(cx)),
                                    )
                                    .child("新建笔记"),
                            )
                            .child(
                                div()
                                    .id("note-folder")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(36.0))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.surface)
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.notice =
                                            Some(SharedString::from("笔记分组管理尚未接入"));
                                        cx.notify();
                                    }))
                                    .child(icons::icon("shell/folder.svg", 15.0, theme.muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(40.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme.muted)
                                    .child("笔记分组"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.muted)
                                    .child(format!("{total} 篇")),
                            ),
                    ),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(44.0))
                    .px_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .id("note-trash")
                            .flex()
                            .items_center()
                            .gap_1()
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(if self.view == NoteView::Trash {
                                theme.accent
                            } else {
                                theme.muted
                            })
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.toggle_trash(cx)),
                            )
                            .child(icons::icon("shell/trash-2.svg", 14.0, theme.muted))
                            .child("回收站"),
                    )
                    .child(
                        div()
                            .id("note-import")
                            .flex()
                            .items_center()
                            .gap_1()
                            .cursor_pointer()
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.notice = Some(SharedString::from("笔记导入尚未接入"));
                                cx.notify();
                            }))
                            .child(icons::icon("shell/upload.svg", 14.0, theme.muted))
                            .child("导入"),
                    ),
            )
    }

    fn group_list(&self) -> Vec<(String, String)> {
        let mut groups = vec![(String::new(), "默认分组".to_string())];
        for group in &self.notebook.groups {
            groups.push((group.id.clone(), group.name.clone()));
        }
        groups
    }

    fn note_row(&self, note: &Note, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = self.theme;
        let active = self.selected.as_ref().map(|id| id.as_str()) == Some(note.id.as_str());
        let id = note.id.clone();
        div()
            .id(SharedString::from(format!("note-{}", note.id)))
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .cursor_pointer()
            .bg(if active {
                theme.accent_soft
            } else {
                theme.sidebar
            })
            .hover(move |style| style.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _event, _window, cx| this.select(&id, cx)))
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(SharedString::from(if note.title.is_empty() {
                        "未命名笔记".to_string()
                    } else {
                        note.title.clone()
                    })),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(SharedString::from(snippet(&note.body, 48))),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.muted)
                    .child(SharedString::from(stamp(note))),
            )
    }

    fn content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let note = self.selected.as_ref().and_then(|id| {
            self.notebook
                .notes
                .iter()
                .find(|note| note.id.as_str() == id.as_str())
        });
        let Some(note) = note else {
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .text_size(px(crate::theme::HEADING_SIZE))
                .text_color(theme.muted)
                .child(icons::icon("shell/notebook-pen.svg", 40.0, theme.muted))
                .child(if self.view == NoteView::Trash {
                    "回收站为空；移到回收站的笔记可以恢复"
                } else {
                    "选择或新建一篇笔记"
                })
                .into_any_element();
        };

        let title = SharedString::from(if note.title.is_empty() {
            "未命名笔记".to_string()
        } else {
            note.title.clone()
        });
        let body = trimmed(&note.body);
        let deleted = note.deleted;

        let mut page = div()
            .id("note-content")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .overflow_y_scroll()
            .px_12()
            .py_8()
            .child(
                div()
                    .text_size(px(28.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text)
                    .child(title),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(52.0))
                    .mt_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .text_size(px(13.0))
                            .text_color(theme.muted)
                            .child(format_button("note-bold", "B", true))
                            .child(format_button("note-italic", "I", true))
                            .child(format_button("note-strike", "S", true))
                            .child(format_button("note-code", "<>", false))
                            .child(format_button("note-h1", "H1", false))
                            .child(format_button("note-h2", "H2", false))
                            .child(format_button("note-h3", "H3", false))
                            .child(format_button("note-h4", "H4", false))
                            .child(icons::icon("shell/list.svg", 15.0, theme.muted))
                            .child(icons::icon("shell/list-ordered.svg", 15.0, theme.muted))
                            .child(icons::icon("shell/quote.svg", 15.0, theme.muted))
                            .child(icons::icon("shell/undo-2.svg", 14.0, theme.muted))
                            .child(icons::icon("shell/redo-2.svg", 14.0, theme.muted)),
                    )
                    .child(
                        div()
                            .id("note-edit")
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .h(px(32.0))
                            .rounded_md()
                            .cursor_pointer()
                            .bg(theme.button)
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.text)
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.notice =
                                    Some(SharedString::from("多行编辑器尚未接入，当前为阅读模式"));
                                cx.notify();
                            }))
                            .child("编辑")
                            .child(icons::icon("shell/chevron-down.svg", 13.0, theme.muted)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .pt_5()
                    .text_size(px(14.0))
                    .text_color(theme.text)
                    .children(markdown_blocks(&body, theme)),
            );

        if deleted {
            page =
                page.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .mt_4()
                        .child(
                            div()
                                .id("note-restore")
                                .px_3()
                                .h(px(32.0))
                                .flex()
                                .items_center()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(theme.accent)
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.surface)
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.set_deleted(false, cx)
                                }))
                                .child("恢复"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.muted)
                                .child("恢复后回到原分组"),
                        ),
                );
        }
        page.into_any_element()
    }
}

fn format_button(id: &'static str, label: &'static str, bold: bool) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(18.0))
        .child(
            div()
                .text_size(px(13.0))
                .when(bold, |style| style.font_weight(FontWeight::BOLD))
                .child(label),
        )
}

fn trimmed(body: &str) -> String {
    if body.len() > PREVIEW_BYTES {
        format!(
            "{}…",
            String::from_utf8_lossy(&body.as_bytes()[..PREVIEW_BYTES])
        )
    } else {
        body.to_string()
    }
}

/// Minimal block renderer: headings, bullets, quotes, code fences and
/// paragraphs. Inline markers are stripped so the text stays readable.
fn markdown_blocks(body: &str, theme: Theme) -> Vec<AnyElement> {
    const FENCE: &str = "\u{60}\u{60}\u{60}";
    let mut blocks = Vec::new();
    let mut code: Vec<String> = Vec::new();
    let mut fenced = false;
    for line in body.lines() {
        if line.trim_start().starts_with(FENCE) {
            if fenced {
                blocks.push(code_block(&code.join("\n"), theme));
                code.clear();
            }
            fenced = !fenced;
            continue;
        }
        if fenced {
            code.push(line.to_string());
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            blocks.push(div().h(px(6.0)).into_any_element());
        } else if let Some(rest) = trimmed.strip_prefix("#### ") {
            blocks.push(heading(rest, 15.0, theme));
        } else if let Some(rest) = trimmed.strip_prefix("### ") {
            blocks.push(heading(rest, 17.0, theme));
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            blocks.push(heading(rest, 21.0, theme));
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            blocks.push(heading(rest, 26.0, theme));
        } else if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            blocks.push(
                div()
                    .flex()
                    .gap_2()
                    .pl_2()
                    .text_size(px(14.0))
                    .text_color(theme.text)
                    .child("•")
                    .child(div().flex_1().child(plain(rest)))
                    .into_any_element(),
            );
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            blocks.push(
                div()
                    .pl_3()
                    .border_l_2()
                    .border_color(theme.border)
                    .text_size(px(14.0))
                    .text_color(theme.muted)
                    .child(plain(rest))
                    .into_any_element(),
            );
        } else {
            blocks.push(
                div()
                    .text_size(px(14.0))
                    .text_color(theme.text)
                    .child(plain(trimmed))
                    .into_any_element(),
            );
        }
    }
    if fenced && !code.is_empty() {
        blocks.push(code_block(&code.join("\n"), theme));
    }
    blocks
}

fn heading(text: &str, size: f32, theme: Theme) -> AnyElement {
    div()
        .text_size(px(size))
        .font_weight(FontWeight::BOLD)
        .text_color(theme.text)
        .mt_3()
        .child(plain(text))
        .into_any_element()
}

fn code_block(code: &str, theme: Theme) -> AnyElement {
    div()
        .p_3()
        .rounded_md()
        .bg(theme.button)
        .text_size(px(12.0))
        .text_color(theme.text)
        .child(SharedString::from(code.to_string()))
        .into_any_element()
}

/// Drops inline Markdown markers that the block renderer cannot style.
fn plain(text: &str) -> SharedString {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' | '_' => {}
            '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for next in chars.by_ref() {
                        if next == ')' {
                            break;
                        }
                    }
                }
            }
            other => out.push(other),
        }
    }
    SharedString::from(out)
}

fn snippet(body: &str, limit: usize) -> String {
    body.lines()
        .map(|line| line.trim().trim_start_matches(['#', '>', '-', '*']).trim())
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(limit).collect())
        .unwrap_or_default()
}

fn stamp(note: &Note) -> String {
    note.updated_at
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%m/%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

impl Render for NotePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let status = match (&self.error, &self.notice) {
            (Some(error), _) => error.clone(),
            (None, Some(notice)) => notice.clone(),
            (None, None) => SharedString::from("本地笔记已加载"),
        };
        let group = SharedString::from(self.group_name(&self.active_group));
        div()
            .flex()
            .size_full()
            .bg(theme.surface)
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(44.0))
                            .px_6()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(if self.error.is_some() {
                                        theme.danger
                                    } else {
                                        theme.muted
                                    })
                                    .child(status),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .px_3()
                                            .h(px(30.0))
                                            .rounded_md()
                                            .bg(theme.button)
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.text)
                                            .child(group)
                                            .child(icons::icon(
                                                "shell/chevron-down.svg",
                                                13.0,
                                                theme.muted,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .text_size(px(crate::theme::TEXT_SIZE))
                                            .text_color(theme.muted)
                                            .child("…")
                                            .child("更多"),
                                    ),
                            ),
                    )
                    .child(self.content(cx)),
            )
    }
}
