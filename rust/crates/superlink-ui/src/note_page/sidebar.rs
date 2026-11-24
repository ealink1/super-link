use super::*;
impl NotePage {
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
            .w(gpui::relative(0.28))
            .min_w(px(232.0))
            .max_w(px(280.0))
            .flex_shrink_0()
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
                                        this.groups_open = !this.groups_open;
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
                                this.import_markdown(cx);
                            }))
                            .child(icons::icon("shell/upload.svg", 14.0, theme.muted))
                            .child("导入"),
                    ),
            )
    }

    pub(super) fn group_list(&self) -> Vec<(String, String)> {
        let mut groups = vec![(String::new(), "默认分组".to_string())];
        for group in &self.notebook.groups {
            groups.push((group.id.clone(), group.name.clone()));
        }
        groups
    }

    pub(super) fn note_row(&self, note: &Note, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
}
