use super::*;

impl QueryPage {
    pub(super) fn export_choices(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut menu = div().flex().gap_2();
        if self.export_operation.is_some() {
            return menu
                .child(
                    div()
                        .id("cancel-export")
                        .cursor_pointer()
                        .child("取消导出")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(cancel) = this.export_operation.take() {
                                cx.background_executor()
                                    .spawn(async move {
                                        cancel.cancel();
                                    })
                                    .detach();
                            }
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        if self.export_open {
            for extension in ["xlsx", "csv", "json", "md", "html"] {
                menu = menu.child(
                    div()
                        .id(SharedString::from(format!("export-{extension}")))
                        .cursor_pointer()
                        .child(extension.to_uppercase())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choose_export(extension, false, cx)
                        })),
                );
            }
            menu = menu.child(
                div()
                    .id("stream-export")
                    .cursor_pointer()
                    .child("完整导出 CSV")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_export("csv", true, cx))),
            );
        }
        menu.into_any_element()
    }

    fn choose_export(&mut self, extension: &'static str, stream: bool, cx: &mut Context<Self>) {
        if self.export_operation.is_some() {
            return;
        }
        let Some(result) = self.result.clone().filter(|r| !r.columns.is_empty()) else {
            self.error = Some("没有可导出的结果".into());
            cx.notify();
            return;
        };
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let request = self
            .last_request
            .clone()
            .unwrap_or_else(|| superlink_domain::Execution {
                revision: profile.revision,
                text: self.last_sql.clone().unwrap_or_default(),
                scope: self.scope.clone(),
                schema: self.schema.clone(),
                ..Default::default()
            });
        if stream && (!request.args.is_empty() || !request.parameters.is_empty()) {
            self.error = Some("完整导出暂不支持绑定参数，请导出已加载结果".into());
            cx.notify();
            return;
        }
        let filename = format!("查询.{}", extension);
        let picker = cx.prompt_for_new_path(&superlink_app::data_root(), Some(&filename));
        self.export_open = false;
        let profiles = self.profiles.clone();
        let queries = self.queries.clone();
        cx.spawn(async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(path))) => path,
                _ => return,
            };
            let cancel = superlink_app::QueryCancel::new();
            let operation = cancel.clone();
            let _ = this.update(cx, |this, cx| {
                this.export_operation = Some(operation);
                this.status = "正在导出…".into();
                cx.notify();
            });
            let task = cx.background_executor().spawn(async move {
                let options =
                    superlink_app::transfer::export_options(&path, profile.sql_dialect(), "")?;
                let count = if stream {
                    let current = profiles.get(&profile.id).map_err(|e| e.to_string())?;
                    let credentials = profiles
                        .credentials(&current)
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default();
                    queries.export_query(
                        &current,
                        &credentials,
                        &request,
                        &path,
                        options,
                        &cancel,
                    )?
                } else {
                    superlink_app::transfer::export_result(&result, &path, options, &cancel)
                        .map_err(|e| e.to_string())?
                };
                Ok::<_, String>((count, path, result.truncated && !stream))
            });
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                this.export_operation = None;
                match outcome {
                    Ok((count, path, truncated)) => {
                        this.status = format!(
                            "已导出 {count} 行{} → {}",
                            if truncated {
                                "（查询结果已截断）"
                            } else {
                                ""
                            },
                            path.display()
                        )
                        .into()
                    }
                    Err(e) => this.error = Some(format!("导出未完成：{e}").into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
