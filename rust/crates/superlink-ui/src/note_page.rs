//! Encrypted notebook workspace. Edits are debounced and persisted off the UI thread.
use crate::{editor::SqlEditor, icons, multiline_editor::SqlTextEditor, theme::Theme};
use gpui::{
    AnyElement, Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px,
};
use std::collections::HashSet;
use std::sync::Arc;
use superlink_app::NotebookService;
use superlink_domain::{Note, Notebook};
mod editing;
mod files;
mod groups;
mod persistence;
mod preview;
mod sidebar;
use preview::{markdown_blocks, snippet, stamp, trimmed};
const PREVIEW_BYTES: usize = 256 << 10;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteView {
    Read,
    Trash,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorMode {
    Edit,
    Source,
    Preview,
    Split,
}

pub struct NotePage {
    service: Arc<NotebookService>,
    theme: Theme,
    notebook: Notebook,
    search: Entity<SqlEditor>,
    query: String,
    selected: Option<SharedString>,
    view: NoteView,
    active_group: String,
    collapsed: HashSet<String>,
    notice: Option<SharedString>,
    error: Option<SharedString>,
    body_editor: Entity<SqlTextEditor>,
    title_editor: Entity<SqlEditor>,
    tags_editor: Entity<SqlEditor>,
    editor_mode: EditorMode,
    dirty: bool,
    saving: bool,
    closing: bool,
    pending_save: Option<persistence::PendingSave>,
    loaded: bool,
    change: u64,
    groups_open: bool,
    group_editor: Entity<SqlEditor>,
    renaming_group: Option<String>,
    move_open: bool,
    delete_confirm: bool,
}

impl NotePage {
    pub fn new(service: Arc<NotebookService>, theme: Theme, cx: &mut Context<Self>) -> Self {
        let page = cx.entity().downgrade();
        let search = cx.new(SqlEditor::new);
        search.update(cx, |e, cx| {
            e.set_placeholder("搜索笔记…", cx);
            let page = page.clone();
            e.set_on_change(
                move |text, app| {
                    let page = page.clone();
                    let text = text.to_string();
                    app.defer(move |app| {
                        let _ = page.update(app, |this, cx| {
                            this.query = text;
                            cx.notify();
                        });
                    });
                },
                cx,
            );
        });
        let body_editor = cx.new(|cx| {
            let mut e = SqlTextEditor::new(cx);
            e.set_plain(true, cx);
            e.set_rich_markdown(true, cx);
            e.set_theme(theme, cx);
            e.set_placeholder("开始记录…", cx);
            e
        });
        let title_editor = cx.new(SqlEditor::new);
        let tags_editor = cx.new(SqlEditor::new);
        for (editor, field) in [
            (title_editor.clone(), "title"),
            (tags_editor.clone(), "tags"),
        ] {
            let page = page.clone();
            editor.update(cx, |e, cx| {
                e.set_placeholder(
                    if field == "title" {
                        "无标题笔记"
                    } else {
                        "标签，逗号分隔"
                    },
                    cx,
                );
                e.set_on_change(
                    move |text, app| {
                        let page = page.clone();
                        let text = text.to_string();
                        app.defer(move |app| {
                            let _ = page.update(app, |this, cx| this.update_note(field, text, cx));
                        });
                    },
                    cx,
                );
            });
        }
        body_editor.update(cx, |e, cx| {
            let page = page.clone();
            e.set_on_change(
                move |text, app| {
                    let page = page.clone();
                    let text = text.to_string();
                    app.defer(move |app| {
                        let _ = page.update(app, |this, cx| this.update_note("body", text, cx));
                    });
                },
                cx,
            );
        });
        let task_service = service.clone();
        let task = cx
            .background_executor()
            .spawn(async move { task_service.load() });
        cx.spawn(async move |this, cx| {
            let loaded = task.await;
            let _ = this.update(cx, |this, cx| {
                match loaded {
                    Ok(book) => {
                        this.loaded = true;
                        this.notebook = book;
                        this.selected = this
                            .notebook
                            .notes
                            .iter()
                            .find(|n| !n.deleted)
                            .map(|n| n.id.clone().into());
                        this.bind_note(cx);
                    }
                    Err(e) => this.error = Some(e.to_string().into()),
                }
                cx.notify();
            });
        })
        .detach();
        Self {
            service,
            theme,
            notebook: Notebook::default(),
            search,
            query: String::new(),
            selected: None,
            view: NoteView::Read,
            active_group: String::new(),
            collapsed: HashSet::new(),
            notice: None,
            error: None,
            body_editor,
            title_editor,
            tags_editor,
            editor_mode: EditorMode::Edit,
            dirty: false,
            saving: false,
            closing: false,
            pending_save: None,
            loaded: false,
            change: 0,
            groups_open: false,
            group_editor: cx.new(SqlEditor::new),
            renaming_group: None,
            move_open: false,
            delete_confirm: false,
        }
    }
    pub(crate) fn save_document(&mut self, cx: &mut Context<Self>) {
        self.save(cx);
    }
    pub(crate) fn resume_after_quit_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.closing = false;
        self.error = Some(error.into());
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        self.body_editor.update(cx, |e, cx| e.set_theme(theme, cx));
        cx.notify();
    }
    fn matches(&self, note: &Note) -> bool {
        let needle = self.query.to_lowercase();
        note.deleted == (self.view == NoteView::Trash)
            && (self.active_group.is_empty() || note.group_id == self.active_group)
            && (needle.is_empty()
                || format!("{} {} {}", note.title, note.body, note.tags)
                    .to_lowercase()
                    .contains(&needle))
    }
    fn group_name(&self, id: &str) -> String {
        self.notebook
            .groups
            .iter()
            .find(|g| g.id == id)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "默认分组".into())
    }
    fn select(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selected = Some(id.to_string().into());
        self.notice = None;
        self.delete_confirm = false;
        self.bind_note(cx);
        cx.notify();
    }
    fn new_note(&mut self, cx: &mut Context<Self>) {
        if !self.loaded {
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.notebook.notes.push(Note {
            id: id.clone(),
            title: "无标题笔记".into(),
            group_id: self.active_group.clone(),
            updated_at: Some(chrono::Utc::now()),
            ..Default::default()
        });
        self.selected = Some(id.into());
        self.view = NoteView::Read;
        self.query.clear();
        self.search.update(cx, |e, cx| e.set_text("", cx));
        self.bind_note(cx);
        self.changed(cx);
    }
    fn toggle_trash(&mut self, cx: &mut Context<Self>) {
        self.view = if self.view == NoteView::Trash {
            NoteView::Read
        } else {
            NoteView::Trash
        };
        self.active_group.clear();
        self.selected = self
            .notebook
            .notes
            .iter()
            .find(|n| n.deleted == (self.view == NoteView::Trash))
            .map(|n| n.id.clone().into());
        self.bind_note(cx);
        cx.notify();
    }
    fn set_deleted(&mut self, deleted: bool, cx: &mut Context<Self>) {
        if let Some(note) = self.current_mut() {
            note.deleted = deleted;
            note.updated_at = Some(chrono::Utc::now());
        }
        self.changed(cx);
        self.selected = None;
        self.bind_note(cx);
    }
    fn current(&self) -> Option<&Note> {
        self.selected
            .as_ref()
            .and_then(|id| self.notebook.notes.iter().find(|n| n.id == id.as_str()))
    }
    fn current_mut(&mut self) -> Option<&mut Note> {
        let id = self.selected.as_ref()?.to_string();
        self.notebook.notes.iter_mut().find(|n| n.id == id)
    }
}
impl Render for NotePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
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
                    .min_h(px(0.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .h(px(48.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .text_size(px(12.0))
                            .text_color(theme.muted)
                            .child(
                                self.error
                                    .clone()
                                    .or_else(|| self.notice.clone())
                                    .unwrap_or_else(|| {
                                        if !self.loaded {
                                            "正在读取本地笔记…".into()
                                        } else if self.saving {
                                            "正在保存…".into()
                                        } else if self.dirty {
                                            "有未保存修改".into()
                                        } else {
                                            "本地笔记已保存".into()
                                        }
                                    }),
                            )
                            .child(div().flex_1())
                            .child(self.action(
                                "note-save",
                                "保存",
                                |this, cx| {
                                    this.save(cx);
                                },
                                cx,
                            ))
                            .child(self.action(
                                "note-export",
                                "导出",
                                |this, cx| this.export_markdown(cx),
                                cx,
                            ))
                            .child(self.action(
                                "note-move",
                                "移动到分组",
                                |this, cx| {
                                    this.move_open = !this.move_open;
                                    cx.notify();
                                },
                                cx,
                            ))
                            .child(self.action(
                                "note-delete",
                                "删除",
                                |this, cx| {
                                    if this.view == NoteView::Trash {
                                        this.delete_confirm = true;
                                        cx.notify();
                                    } else {
                                        this.set_deleted(true, cx);
                                    }
                                },
                                cx,
                            )),
                    )
                    .children(self.groups_open.then(|| self.groups_panel(cx)))
                    .children(self.move_open.then(|| self.move_panel(cx)))
                    .children(self.delete_confirm.then(|| {
                        div()
                            .flex()
                            .gap_3()
                            .p_3()
                            .bg(theme.warning_bg)
                            .child("永久删除后无法恢复。")
                            .child(self.action(
                                "note-delete-confirm",
                                "永久删除",
                                |this, cx| {
                                    let id = this.selected.as_ref().map(|id| id.to_string());
                                    this.notebook.notes.retain(|n| Some(&n.id) != id.as_ref());
                                    this.selected = None;
                                    this.delete_confirm = false;
                                    this.changed(cx);
                                },
                                cx,
                            ))
                            .child(self.action(
                                "note-delete-cancel",
                                "取消",
                                |this, cx| {
                                    this.delete_confirm = false;
                                    cx.notify();
                                },
                                cx,
                            ))
                    }))
                    .child(self.content(cx)),
            )
    }
}
