use super::*;

impl QueryPage {
    pub(super) fn profile_options(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .id("query-profile-menu")
            .flex()
            .flex_wrap()
            .gap_2()
            .p_2()
            .bg(theme.canvas)
            .children(self.profile_choices.clone().into_iter().map(|profile| {
                let title = profile.name.clone();
                div()
                    .id(SharedString::from(format!("choose-profile-{}", profile.id)))
                    .cursor_pointer()
                    .px_3()
                    .py_1()
                    .bg(theme.button)
                    .rounded_md()
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_profile(profile.clone(), cx)),
                    )
                    .child(SharedString::from(title))
            }))
            .into_any_element()
    }

    pub(super) fn open_scope(&mut self, cx: &mut Context<Self>) {
        self.scope_open = !self.scope_open;
        self.scope_input.update(cx, |e, cx| {
            e.set_text(self.scope.clone(), cx);
            e.set_placeholder("数据库 / Namespace", cx);
        });
        self.schema_input.update(cx, |e, cx| {
            e.set_text(self.schema.clone(), cx);
            e.set_placeholder("Schema（可选）", cx);
        });
        if self.scope_open
            && let Some(profile) = self.profile.clone()
        {
            let queries = self.queries.clone();
            let profiles = self.profiles.clone();
            let id = profile.id.clone();
            let revision = profile.revision;
            let task = cx.background_executor().spawn(async move {
                let credentials = profiles
                    .credentials(&profile)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                queries
                    .databases(&profile, &credentials)
                    .map_err(|e| e.to_string())
            });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.profile.as_ref().map(|p| (&p.id, p.revision)) != Some((&id, revision)) {
                        return;
                    }
                    if let Ok(items) = result {
                        this.scope_options = items;
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub(super) fn scope_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(theme.canvas)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().w(px(220.0)).child(self.scope_input.clone()))
                    .child(div().w(px(160.0)).child(self.schema_input.clone()))
                    .child(
                        div()
                            .id("apply-query-scope")
                            .cursor_pointer()
                            .px_3()
                            .text_color(theme.accent)
                            .child("应用")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_query(cx);
                                this.scope = this.scope_input.read(cx).text().trim().into();
                                this.schema = this.schema_input.read(cx).text().trim().into();
                                this.scope_open = false;
                                this.save_active(cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(self.scope_options.iter().map(|name| {
                        let name = name.clone();
                        div()
                            .id(SharedString::from(format!("query-db-{name}")))
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .bg(theme.button)
                            .child(SharedString::from(name.clone()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.stop_query(cx);
                                this.scope = name.clone();
                                this.scope_open = false;
                                this.save_active(cx);
                                cx.notify();
                            }))
                    })),
            )
            .into_any_element()
    }

    fn result_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap_1()
            .children(
                (0..self.other_results.len() + usize::from(self.result.is_some())).map(|index| {
                    div()
                        .id(("result-set", index))
                        .px_2()
                        .cursor_pointer()
                        .text_color(if index == self.selected_result {
                            self.theme.accent
                        } else {
                            self.theme.muted
                        })
                        .child(format!("结果 {}", index + 1))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if index != this.selected_result
                                && let Some(current) = this.result.take()
                            {
                                this.other_results.insert(this.selected_result, current);
                                this.result = Some(this.other_results.remove(index));
                                this.selected_result = index;
                                this.display_page = 0;
                                cx.notify();
                            }
                        }))
                }),
            )
            .into_any_element()
    }

    pub(super) fn result_pager(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = self.result.as_ref().map(|r| r.rows.len()).unwrap_or(0);
        let total = count.div_ceil(DISPLAY_ROWS).max(1);
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .h(px(28.0))
            .text_size(px(12.0))
            .text_color(self.theme.muted)
            .child(self.result_choices(cx))
            .child(self.export_choices(cx))
            .child(
                div()
                    .id("result-prev")
                    .cursor_pointer()
                    .child("上一页")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.display_page = this.display_page.saturating_sub(1);
                        cx.notify();
                    })),
            )
            .child(format!("{} / {total} · {count} 行", self.display_page + 1))
            .child(
                div()
                    .id("result-next")
                    .cursor_pointer()
                    .child("下一页")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.display_page = (this.display_page + 1).min(total - 1);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("copy-result")
                    .cursor_pointer()
                    .child("复制结果")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(result) = &this.result {
                            let mut lines = vec![
                                result
                                    .columns
                                    .iter()
                                    .map(|c| c.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join("\t"),
                            ];
                            lines.extend(result.rows.iter().map(|row| {
                                row.iter()
                                    .map(|v| cell_text(v).to_string().replace(['\t', '\n'], " "))
                                    .collect::<Vec<_>>()
                                    .join("\t")
                            }));
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                lines.join("\n"),
                            ));
                        }
                    })),
            )
            .into_any_element()
    }
}
