use super::*;
#[derive(Default)]
pub(super) struct UpdateUi {
    pub(super) cancel: Option<superlink_app::QueryCancel>,
    release: Option<superlink_app::updates::Release>,
    pub(super) prepared: Option<superlink_app::updates::Prepared>,
    pub(super) message: SharedString,
}
impl Drop for UpdateUi {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            std::thread::spawn(move || cancel.cancel());
        }
    }
}
impl SuperLinkApp {
    pub(super) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.updates.cancel.is_some() {
            return;
        }
        let cancel = superlink_app::QueryCancel::new();
        self.updates.cancel = Some(cancel.clone());
        self.updates.release = None;
        self.updates.message = "正在检测签名发布…".into();
        let task = cx
            .background_executor()
            .spawn(async move { superlink_app::updates::check(&cancel) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                this.updates.cancel = None;
                match outcome {
                    Ok(Some(release)) if superlink_app::updates::is_newer(&release) => {
                        this.updates.message =
                            format!("发现新版本 {}", release.manifest.version).into();
                        this.updates.release = Some(release);
                    }
                    Ok(Some(_)) => this.updates.message = "当前已是最新稳定版本".into(),
                    Ok(None) => this.updates.message = "尚未发布可用的稳定版本".into(),
                    Err(error) => this.updates.message = error.into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn download_update(&mut self, cx: &mut Context<Self>) {
        let Some(release) = self.updates.release.clone() else {
            return;
        };
        if self.updates.cancel.is_some() {
            return;
        }
        let cancel = superlink_app::QueryCancel::new();
        self.updates.cancel = Some(cancel.clone());
        self.updates.message = "正在下载并校验更新包…".into();
        let task = cx.background_executor().spawn(async move {
            let directory = superlink_app::data_root().join("updates");
            let path = superlink_app::updates::download(&release, &directory, &cancel)?;
            match superlink_app::updates::prepare(&path, &release, &directory) {
                Ok(request) => Ok((
                    Some(request),
                    "更新已准备，点击退出并安装后生效".to_string(),
                )),
                Err(error) => {
                    Ok::<_, String>((None, format!("{error}；已下载到 {}", path.display())))
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| {
                this.updates.cancel = None;
                match outcome {
                    Ok((request, message)) => {
                        this.updates.prepared = request;
                        this.updates.message = message.into();
                    }
                    Err(e) => this.updates.message = e.into(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn update_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(12.))
            .text_color(self.theme.text)
            .child(self.updates.message.clone());
        let mut actions = div().flex().gap_3();
        if self.updates.cancel.is_some() {
            actions = actions.child(
                div()
                    .id("cancel-update")
                    .cursor_pointer()
                    .child("取消更新任务")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(cancel) = this.updates.cancel.as_ref().cloned() {
                            cx.background_executor()
                                .spawn(async move {
                                    cancel.cancel();
                                })
                                .detach();
                        }
                    })),
            );
        } else {
            actions = actions.child(
                div()
                    .id("check-updates")
                    .cursor_pointer()
                    .text_color(self.theme.accent)
                    .child("检测更新")
                    .on_click(cx.listener(|this, _, _, cx| this.check_updates(cx))),
            );
            if self.updates.prepared.is_some() {
                actions = actions.child(
                    div()
                        .id("install-update")
                        .cursor_pointer()
                        .child("退出并安装更新")
                        .on_click(cx.listener(|this, _, _, cx| this.request_quit(cx))),
                );
            } else if self.updates.release.is_some() {
                actions = actions.child(
                    div()
                        .id("download-update")
                        .cursor_pointer()
                        .child("下载并准备更新")
                        .on_click(cx.listener(|this, _, _, cx| this.download_update(cx))),
                );
            }
        }
        panel = panel.child(actions);
        panel.into_any_element()
    }
}
