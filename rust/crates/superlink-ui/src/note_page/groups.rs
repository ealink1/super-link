use super::*;
use superlink_domain::NoteGroup;

impl NotePage {
    pub(super) fn groups_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(self.group_editor.clone()))
                    .child(self.action(
                        "note-group-save",
                        "保存分组",
                        |this, cx| this.save_group(cx),
                        cx,
                    ))
                    .child(self.action(
                        "note-group-close",
                        "关闭",
                        |this, cx| {
                            this.groups_open = false;
                            this.renaming_group = None;
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .children(self.notebook.groups.iter().map(|group| {
                let id = group.id.clone();
                let rename = id.clone();
                let name = group.name.clone();
                let label = name.clone();
                div()
                    .flex()
                    .gap_3()
                    .text_color(theme.text)
                    .text_size(px(12.0))
                    .child(SharedString::from(label))
                    .child(
                        div()
                            .id(SharedString::from(format!("note-group-rename-{id}")))
                            .cursor_pointer()
                            .child("重命名")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.renaming_group = Some(rename.clone());
                                this.group_editor
                                    .update(cx, |e, cx| e.set_text(name.clone(), cx));
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("note-group-remove-{id}")))
                            .cursor_pointer()
                            .child("删除分组（笔记移到默认）")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.notebook.groups.retain(|g| g.id != id);
                                for note in &mut this.notebook.notes {
                                    if note.group_id == id {
                                        note.group_id.clear();
                                    }
                                }
                                if this.active_group == id {
                                    this.active_group.clear();
                                }
                                this.changed(cx);
                            })),
                    )
            }))
            .into_any_element()
    }

    fn save_group(&mut self, cx: &mut Context<Self>) {
        let name = self.group_editor.read(cx).text().trim().to_string();
        let mut book = self.notebook.clone();
        if let Some(id) = self.renaming_group.as_ref() {
            if let Some(group) = book.groups.iter_mut().find(|g| &g.id == id) {
                group.name = name;
            }
        } else {
            book.groups.push(NoteGroup {
                id: uuid::Uuid::new_v4().to_string(),
                name,
            });
        }
        if let Err(error) = book.validate() {
            self.error = Some(error.to_string().into());
            cx.notify();
            return;
        }
        self.notebook = book;
        self.renaming_group = None;
        self.group_editor.update(cx, |e, cx| e.set_text("", cx));
        self.changed(cx);
    }

    pub(super) fn move_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .p_3()
            .bg(self.theme.canvas)
            .children(self.group_list().into_iter().map(|(id, name)| {
                div()
                    .id(SharedString::from(format!("note-move-{id}")))
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .child(SharedString::from(name))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(note) = this.current_mut() {
                            note.group_id = id.clone();
                            note.updated_at = Some(chrono::Utc::now());
                        }
                        this.move_open = false;
                        this.active_group.clear();
                        this.changed(cx);
                    }))
            }))
            .into_any_element()
    }
}
