use super::*;

impl SuperLinkApp {
    pub(super) fn groups_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let summaries = superlink_app::groups::summarize(&self.profiles, &self.declared_groups);

        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child("管理连接分组"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("close-groups")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(
                                cx.listener(|this, _event, _window, cx| this.close_groups(cx)),
                            )
                            .child(
                                div()
                                    .text_size(px(crate::theme::TEXT_SIZE))
                                    .text_color(theme.muted)
                                    .child("关闭"),
                            ),
                    ),
            );

        if let Some(note) = &self.group_note {
            panel = panel.child(
                div()
                    .mx_3()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(crate::theme::TEXT_SIZE))
                    .text_color(theme.muted)
                    .child(note.clone()),
            );
        }

        // 新建分组
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w(px(200.0))
                        .h(px(crate::theme::CONTROL_MIN))
                        .px_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.surface)
                        .child(self.group_new.clone()),
                )
                .child(
                    div()
                        .id("create-group")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(theme.accent)
                        .on_click(cx.listener(|this, _event, _window, cx| this.create_group(cx)))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.surface)
                                .child("新建分组"),
                        ),
                ),
        );

        // 分组列表
        let mut list = div()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .pt_2()
            .flex_1()
            .min_h(px(0.0));
        for group in &summaries {
            let name = group.name.clone();
            let rename_target = name.clone();
            let is_default = name == superlink_app::groups::DEFAULT_GROUP;
            let renaming = self
                .group_rename
                .as_ref()
                .map(|(from, _)| from == &name)
                .unwrap_or(false);
            let mut block = div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(theme.border)
                .bg(theme.canvas);
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text)
                            .child(SharedString::from(format!(
                                "{} · {}",
                                group.name, group.count
                            ))),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(SharedString::from(format!("rename-group-{name}")))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.accent)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.start_rename_group(rename_target.clone(), cx)
                            }))
                            .child("重命名"),
                    )
                    .children((!is_default).then(|| {
                        let delete_name = group.name.clone();
                        div()
                            .id(SharedString::from(format!("delete-group-{name}")))
                            .text_size(px(crate::theme::TEXT_SIZE))
                            .text_color(theme.muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.delete_group(delete_name.clone(), cx)
                            }))
                            .child("删除")
                    })),
            );

            if renaming && let Some((_, editor)) = &self.group_rename {
                let commit_name = group.name.clone();
                block = block.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .w(px(180.0))
                                .h(px(crate::theme::CONTROL_MIN))
                                .px_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(editor.clone()),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("confirm-rename-{name}")))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(theme.accent)
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.commit_rename_group(commit_name.clone(), cx)
                                }))
                                .child(
                                    div()
                                        .text_size(px(crate::theme::TEXT_SIZE))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.surface)
                                        .child("确认改名"),
                                ),
                        ),
                );
            }

            // 该分组的连接
            for profile in self.profiles.iter().filter(|profile| {
                superlink_app::groups::display_group(&profile.group) == group.name
            }) {
                let id = profile.id.clone();
                let profile_name = profile.name.clone();
                block = block.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl(px(12.0))
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.text)
                                .child(SharedString::from(profile_name)),
                        )
                        .child(
                            div()
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.muted)
                                .child(SharedString::from(profile.sql_dialect().to_uppercase())),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(SharedString::from(format!("move-{id}")))
                                .text_size(px(crate::theme::TEXT_SIZE))
                                .text_color(theme.accent)
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.move_profile_next(id.clone(), cx)
                                }))
                                .child("移到下一组"),
                        ),
                );
            }
            list = list.child(block);
        }
        panel.child(list).into_any_element()
    }
}
