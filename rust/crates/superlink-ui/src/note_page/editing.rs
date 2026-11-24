use super::*;

impl NotePage {
    pub(super) fn bind_note(&mut self, cx: &mut Context<Self>) {
        let note = self.current().cloned().unwrap_or_default();
        let enabled = !note.deleted && self.selected.is_some();
        self.title_editor.update(cx, |e, cx| {
            e.set_text(note.title, cx);
            e.set_enabled(enabled, cx);
        });
        self.tags_editor.update(cx, |e, cx| {
            e.set_text(note.tags, cx);
            e.set_enabled(enabled, cx);
        });
        self.body_editor.update(cx, |e, cx| {
            e.set_text(note.body, cx);
            e.set_enabled(enabled, cx);
        });
    }

    pub(super) fn update_note(&mut self, field: &str, text: String, cx: &mut Context<Self>) {
        let Some(note) = self.current_mut() else {
            return;
        };
        if note.deleted {
            return;
        }
        match field {
            "title" => note.title = text,
            "tags" => note.tags = text,
            _ => note.body = text,
        }
        note.updated_at = Some(chrono::Utc::now());
        self.changed(cx);
    }

    pub(super) fn action(
        &self,
        id: &'static str,
        label: &'static str,
        handler: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        div()
            .id(id)
            .cursor_pointer()
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(12.0))
            .text_color(theme.text)
            .hover(move |s| s.bg(theme.accent_soft))
            .on_click(cx.listener(move |this, _, _, cx| handler(this, cx)))
            .child(label)
            .into_any_element()
    }

    pub(super) fn content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let Some(note) = self.current() else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted)
                .child("选择或新建一篇笔记")
                .into_any_element();
        };
        let preview = div()
            .id("note-preview")
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p_6()
            .text_size(px(14.))
            .text_color(theme.text)
            .children(markdown_blocks(&trimmed(&note.body), theme))
            .into_any_element();
        let group = self.group_name(&note.group_id);
        let formats = self.body_editor.read(cx).formatting_state();
        let toolbar = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .children(
                [
                    ("note-bold", "B", "**", "**"),
                    ("note-italic", "I", "*", "*"),
                    ("note-strike", "S", "~~", "~~"),
                    ("note-code", "<>", "`", "`"),
                    ("note-h1", "H1", "# ", ""),
                    ("note-h2", "H2", "## ", ""),
                    ("note-h3", "H3", "### ", ""),
                    ("note-list", "列表", "- ", ""),
                    ("note-ordered", "编号", "1. ", ""),
                    ("note-quote", "引用", "> ", ""),
                    ("note-link", "链接", "[", "](https://)"),
                    ("note-codeblock", "代码块", "\n```\n", "\n```\n"),
                ]
                .into_iter()
                .map(|(id, label, prefix, suffix)| {
                    let active = formats.active(prefix);
                    let disabled = formats.disabled(prefix, suffix);
                    div()
                        .rounded_md()
                        .bg(if active {
                            theme.accent_soft
                        } else {
                            theme.surface
                        })
                        .when(disabled, |item| item.opacity(0.45))
                        .child(self.action(
                            id,
                            label,
                            move |this, cx| {
                                if !disabled {
                                    this.body_editor
                                        .update(cx, |e, cx| e.format(prefix, suffix, cx));
                                }
                            },
                            cx,
                        ))
                        .into_any_element()
                }),
            )
            .child(self.action(
                "note-undo",
                "撤销",
                |this, cx| this.body_editor.update(cx, |e, cx| e.undo(cx)),
                cx,
            ))
            .child(self.action(
                "note-redo",
                "重做",
                |this, cx| this.body_editor.update(cx, |e, cx| e.redo(cx)),
                cx,
            ))
            .child(div().flex_1())
            .children(
                [
                    ("note-mode-edit", "编辑", EditorMode::Edit),
                    ("note-mode-source", "源码", EditorMode::Source),
                    ("note-mode-preview", "预览", EditorMode::Preview),
                    ("note-mode-split", "分屏", EditorMode::Split),
                ]
                .into_iter()
                .map(|(id, label, mode)| {
                    let active = self.editor_mode == mode;
                    div()
                        .bg(if active {
                            theme.accent_soft
                        } else {
                            theme.surface
                        })
                        .rounded_md()
                        .child(self.action(
                            id,
                            label,
                            move |this, cx| {
                                this.editor_mode = mode;
                                this.body_editor.update(cx, |e, cx| {
                                    e.set_rich_markdown(mode == EditorMode::Edit, cx)
                                });
                                cx.notify();
                            },
                            cx,
                        ))
                }),
            );
        let body = match self.editor_mode {
            EditorMode::Preview => preview,
            EditorMode::Split => div()
                .flex()
                .flex_1()
                .min_h(px(0.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .border_r_1()
                        .border_color(theme.border)
                        .child(self.body_editor.clone()),
                )
                .child(preview)
                .into_any_element(),
            _ => div()
                .flex_1()
                .min_h(px(0.0))
                .px_6()
                .py_4()
                .child(self.body_editor.clone())
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .child(
                div()
                    .px_6()
                    .py_4()
                    .text_size(px(28.0))
                    .font_weight(FontWeight::BOLD)
                    .child(self.title_editor.clone()),
            )
            .child(
                div()
                    .px_6()
                    .pb_2()
                    .text_size(px(12.0))
                    .child(self.tags_editor.clone()),
            )
            .child(
                div()
                    .px_6()
                    .pb_2()
                    .text_size(px(12.0))
                    .text_color(theme.muted)
                    .child(group),
            )
            .child(toolbar)
            .children(note.deleted.then(|| {
                div()
                    .flex()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .child("此笔记在回收站中")
                    .child(self.action(
                        "note-restore",
                        "恢复",
                        |this, cx| this.set_deleted(false, cx),
                        cx,
                    ))
            }))
            .child(body)
            .into_any_element()
    }
}
