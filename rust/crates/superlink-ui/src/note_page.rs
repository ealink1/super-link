//! Note workspace: grouped document list and a bounded Markdown preview.
//!
//! Editing lands in the next stage; this view already reads the encrypted
//! notebook through the application service.

use std::sync::Arc;

use gpui::{Context, FontWeight, Render, SharedString, Window, div, prelude::*, px};
use superlink_app::NotebookService;
use superlink_domain::{Note, Notebook};

use crate::icons;
use crate::theme::Theme;

const PREVIEW_BYTES: usize = 8 << 10;

pub struct NotePage {
    service: Arc<NotebookService>,
    theme: Theme,
    notebook: Notebook,
    selected: usize,
    error: Option<SharedString>,
}

impl NotePage {
    pub fn new(service: Arc<NotebookService>, theme: Theme) -> Self {
        let (notebook, error) = match service.load() {
            Ok(notebook) => (notebook, None),
            Err(err) => (
                Notebook::default(),
                Some(SharedString::from(err.to_string())),
            ),
        };
        Self {
            service,
            theme,
            notebook,
            selected: 0,
            error,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match self.service.load() {
            Ok(notebook) => {
                self.notebook = notebook;
                self.error = None;
                if self.selected >= self.active_notes().len() {
                    self.selected = self.active_notes().len().saturating_sub(1);
                }
            }
            Err(err) => self.error = Some(SharedString::from(err.to_string())),
        }
        cx.notify();
    }

    fn active_notes(&self) -> Vec<&Note> {
        self.notebook
            .notes
            .iter()
            .filter(|note| !note.deleted)
            .collect()
    }

    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let notes = self.active_notes();
        let mut column = div()
            .flex()
            .flex_col()
            .w(px(260.0))
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
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.muted)
                            .child("笔记"),
                    )
                    .child(
                        div()
                            .id("reload-notes")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .cursor_pointer()
                            .text_color(theme.muted)
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(|this, _event, _window, cx| this.reload(cx)))
                            .child(icons::icon("shell/refresh-cw.svg", 13.0, theme.muted))
                            .child(format!("{}", notes.len())),
                    ),
            );

        if notes.is_empty() {
            column = column.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child("还没有笔记。编辑器接入后可以在这里新建。"),
            );
        }

        for (index, note) in notes.iter().enumerate() {
            let active = index == self.selected;
            let title = if note.title.is_empty() {
                SharedString::from("未命名笔记")
            } else {
                SharedString::from(note.title.clone())
            };
            column = column.child(
                div()
                    .id(("note", index))
                    .mx_2()
                    .px_2()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active {
                        theme.accent_soft
                    } else {
                        theme.sidebar
                    })
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.selected = index;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(if active { theme.accent } else { theme.text })
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .child(SharedString::from(snippet(&note.body, 60))),
                    ),
            );
        }
        column
    }

    fn preview(&self) -> impl IntoElement {
        let theme = self.theme;
        let notes = self.active_notes();
        let Some(note) = notes.get(self.selected) else {
            return div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .bg(theme.surface)
                .text_size(px(crate::theme::TEXT_SIZE))
                .text_color(theme.muted)
                .child("选择左侧笔记查看内容");
        };
        let body = note.body.as_bytes();
        let preview = if body.len() > PREVIEW_BYTES {
            format!("{}…", String::from_utf8_lossy(&body[..PREVIEW_BYTES]))
        } else {
            note.body.clone()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .p_4()
            .gap_2()
            .bg(theme.surface)
            .child(
                div()
                    .text_size(px(crate::theme::HEADING_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text)
                    .child(SharedString::from(note.title.clone())),
            )
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.text)
                    .child(SharedString::from(preview)),
            )
    }
}

fn snippet(body: &str, limit: usize) -> String {
    body.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.chars().take(limit).collect())
        .unwrap_or_default()
}

impl Render for NotePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .bg(theme.canvas);
        if let Some(error) = self.error.clone() {
            body = body.child(
                div()
                    .px_3()
                    .py_2()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.danger)
                    .child(error),
            );
        }
        div()
            .flex()
            .size_full()
            .child(self.list(cx))
            .child(body.child(self.preview()))
    }
}
