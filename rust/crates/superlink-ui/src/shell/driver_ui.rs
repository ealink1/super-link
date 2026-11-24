use super::*;
pub(super) struct DriverUi {
    kind: String,
    checksum: Entity<SqlEditor>,
    release: Option<superlink_app::updates::Release>,
    pub(super) cancel: Option<superlink_app::QueryCancel>,
    message: SharedString,
}
impl DriverUi {
    pub fn new(cx: &mut Context<SuperLinkApp>) -> Self {
        let checksum = cx.new(SqlEditor::new);
        checksum.update(cx, |editor, cx| {
            editor.set_placeholder("官方发布文件的 SHA256 校验和", cx)
        });
        Self {
            kind: "postgres".into(),
            checksum,
            release: None,
            cancel: None,
            message: "选择数据源后，从签名发布下载或从本地文件安装驱动。".into(),
        }
    }
}
impl SuperLinkApp {
    pub(super) fn drivers_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let installed = self.queries.installed_drivers();
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .text_color(theme.text)
            .text_size(px(12.))
            .child(div().font_weight(FontWeight::BOLD).child("驱动管理"))
            .child(
                div()
                    .id("driver-type-list")
                    .max_h(px(120.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(superlink_domain::catalog().iter().map(|descriptor| {
                        let kind = descriptor.key.to_string();
                        let active = kind == self.drivers.kind;
                        let available = installed
                            .iter()
                            .any(|d| d.kind == kind && d.problem.is_none());
                        div()
                            .id(SharedString::from(format!("install-driver-{kind}")))
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(if active {
                                theme.accent_soft
                            } else {
                                theme.canvas
                            })
                            .child(format!(
                                "{}{}",
                                descriptor.name,
                                if available { " ✓" } else { "" }
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.drivers.cancel.is_none() {
                                    this.drivers.kind = kind.clone();
                                    cx.notify();
                                }
                            }))
                    })),
            )
            .child(self.drivers.message.clone());
        if let Some(driver) = installed.iter().find(|d| d.kind == self.drivers.kind) {
            panel = panel.child(driver.problem.clone().unwrap_or_else(|| {
                if driver.verified {
                    "已校验并可用".into()
                } else {
                    "已发现，首次使用前校验".into()
                }
            }));
        }
        if self.drivers.cancel.is_some() {
            return panel
                .child(
                    div()
                        .id("stop-driver-install")
                        .cursor_pointer()
                        .child("取消驱动任务")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(cancel) = this.drivers.cancel.clone() {
                                cx.background_executor()
                                    .spawn(async move {
                                        cancel.cancel();
                                    })
                                    .detach();
                            }
                        })),
                )
                .into_any_element();
        }
        panel = panel
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .id("check-driver-release")
                            .cursor_pointer()
                            .text_color(theme.accent)
                            .child("检查驱动发布")
                            .on_click(cx.listener(|this, _, _, cx| this.check_driver_release(cx))),
                    )
                    .children(self.drivers.release.as_ref().map(|_| {
                        div()
                            .id("download-selected-driver")
                            .cursor_pointer()
                            .text_color(theme.accent)
                            .child("下载并安装所选驱动")
                            .on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.install_selected_driver(None, cx)
                                }),
                            )
                    })),
            )
            .child(self.drivers.checksum.clone())
            .child(
                div()
                    .id("choose-local-driver")
                    .cursor_pointer()
                    .child("从本地驱动文件安装…")
                    .on_click(cx.listener(|_this, _, _, cx| {
                        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: Some("选择与当前数据源对应的驱动代理文件".into()),
                        });
                        cx.spawn(async move |this, cx| {
                            if let Ok(Ok(Some(paths))) = picker.await
                                && let Some(path) = paths.into_iter().next()
                            {
                                let _ = this.update(cx, |this, cx| {
                                    this.install_selected_driver(Some(path), cx)
                                });
                            }
                        })
                        .detach();
                    })),
            );
        panel.into_any_element()
    }
    fn check_driver_release(&mut self, cx: &mut Context<Self>) {
        if self.drivers.cancel.is_some() {
            return;
        }
        let cancel = superlink_app::QueryCancel::new();
        self.drivers.cancel = Some(cancel.clone());
        self.drivers.message = "正在检查签名驱动发布…".into();
        let task = cx
            .background_executor()
            .spawn(async move { superlink_app::updates::check_drivers(&cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.drivers.cancel = None;
                match result {
                    Ok(Some(release)) => {
                        this.drivers.message =
                            format!("可用驱动发布 {}", release.manifest.version).into();
                        this.drivers.release = Some(release);
                    }
                    Ok(None) => this.drivers.message = "尚无可用的驱动发布".into(),
                    Err(error) => this.drivers.message = error.into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn install_selected_driver(
        &mut self,
        path: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.drivers.cancel.is_some() {
            return;
        }
        let kind = self.drivers.kind.clone();
        let checksum = self.drivers.checksum.read(cx).text().trim().to_string();
        let release = self.drivers.release.clone();
        let queries = self.queries.clone();
        let cancel = superlink_app::QueryCancel::new();
        self.drivers.cancel = Some(cancel.clone());
        self.drivers.message = "正在校验并安装驱动…".into();
        let task = cx.background_executor().spawn(async move {
            match path {
                Some(path) => queries.install_local_driver(&path, &kind, &checksum, None, &cancel),
                None => queries.install_release_driver(
                    &release.ok_or("请先检查签名驱动发布")?,
                    &kind,
                    &cancel,
                ),
            }
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                this.drivers.cancel = None;
                this.drivers.message = match outcome {
                    Ok(()) => "驱动安装完成".into(),
                    Err(error) => format!("驱动安装未完成：{error}").into(),
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
