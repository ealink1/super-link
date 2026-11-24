use super::*;

impl TerminalPage {
    pub(super) fn files_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let files = self.visible_files();
        let page = self
            .auxiliary
            .page
            .min(files.len().div_ceil(200).saturating_sub(1));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .p_3()
            .gap_2()
            .text_color(theme.text)
            .text_size(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(100.))
                            .child(self.auxiliary.filter.clone()),
                    )
                    .child(self.aux_button(
                        "sftp-hidden",
                        if self.auxiliary.show_hidden {
                            "隐藏点文件"
                        } else {
                            "显示点文件"
                        },
                        |this, cx| {
                            this.auxiliary.show_hidden = !this.auxiliary.show_hidden;
                            this.auxiliary.page = 0;
                            this.auxiliary.selected = None;
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(format!("{} 项", files.len())),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(div().flex_1().child(self.auxiliary.path.clone()))
                    .child(self.aux_button("sftp-go", "打开", |this, cx| this.list_files(cx), cx))
                    .child(self.aux_button(
                        "sftp-parent",
                        "上级",
                        |this, cx| {
                            let current = this.remote_path(cx);
                            let parent =
                                superlink_terminal::normalize_remote_path(&format!("{current}/.."));
                            this.auxiliary
                                .path
                                .update(cx, |e, cx| e.set_text(parent, cx));
                            this.list_files(cx);
                        },
                        cx,
                    ))
                    .child(self.aux_button(
                        "sftp-upload",
                        "上传",
                        |this, cx| this.upload_file(cx),
                        cx,
                    ))
                    .child(self.aux_button(
                        "sftp-download",
                        "下载",
                        |this, cx| this.download_file(cx),
                        cx,
                    ))
                    .child(self.aux_button(
                        "sftp-stop",
                        "停止传输",
                        |this, cx| {
                            this.auxiliary.cancel.cancel();
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(div().flex_1().child(self.auxiliary.name.clone()))
                    .child(self.aux_button(
                        "sftp-rename",
                        "重命名",
                        |this, cx| {
                            if let Some(source) = this.selected_path(cx) {
                                let name = this.auxiliary.name.read(cx).text().trim().to_string();
                                if name.is_empty()
                                    || name.contains('/')
                                    || matches!(name.as_str(), "." | "..")
                                {
                                    this.auxiliary.message = "请输入有效的文件名称".into();
                                    cx.notify();
                                    return;
                                }
                                let target = format!(
                                    "{}/{}",
                                    this.auxiliary.loaded_path.trim_end_matches('/'),
                                    name
                                );
                                this.file_operation(
                                    move |sftp, cancel| {
                                        sftp.rename_file(&source, &target, cancel)
                                            .map(|_| FileResult::Done("已重命名".into()))
                                    },
                                    cx,
                                );
                            }
                        },
                        cx,
                    ))
                    .child(self.aux_button(
                        "sftp-chmod",
                        "设置权限",
                        |this, cx| {
                            if let Some(path) = this.selected_path(cx) {
                                let mode = u32::from_str_radix(
                                    this.auxiliary.name.read(cx).text().trim(),
                                    8,
                                );
                                match mode {
                                    Ok(mode) => this.file_operation(
                                        move |sftp, cancel| {
                                            sftp.set_permissions(&path, mode, cancel)
                                                .map(|_| FileResult::Done("权限已更新".into()))
                                        },
                                        cx,
                                    ),
                                    Err(_) => {
                                        this.auxiliary.message =
                                            "请输入八进制权限，例如 644".into();
                                        cx.notify();
                                    }
                                }
                            }
                        },
                        cx,
                    ))
                    .child(self.aux_button(
                        "sftp-remove",
                        "删除",
                        |this, cx| {
                            this.auxiliary.remove_confirm = true;
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .children(self.auxiliary.remove_confirm.then(|| {
                div()
                    .flex()
                    .gap_2()
                    .child("确认删除选中的远程文件？")
                    .child(self.aux_button(
                        "sftp-remove-confirm",
                        "确认删除",
                        |this, cx| {
                            this.auxiliary.remove_confirm = false;
                            if let Some(path) = this.selected_path(cx) {
                                this.file_operation(
                                    move |sftp, cancel| {
                                        sftp.remove(&path, cancel)
                                            .map(|_| FileResult::Done("已删除".into()))
                                    },
                                    cx,
                                );
                            }
                        },
                        cx,
                    ))
                    .child(self.aux_button(
                        "sftp-remove-cancel",
                        "取消",
                        |this, cx| {
                            this.auxiliary.remove_confirm = false;
                            cx.notify();
                        },
                        cx,
                    ))
            }))
            .child(format!(
                "{}{} · {} 字节",
                if self.auxiliary.busy {
                    "处理中 · "
                } else {
                    ""
                },
                self.auxiliary.message,
                self.auxiliary.progress.load(Ordering::Relaxed)
            ))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.aux_button(
                        "sftp-prev",
                        "上一页",
                        |this, cx| {
                            this.auxiliary.page = this.auxiliary.page.saturating_sub(1);
                            cx.notify();
                        },
                        cx,
                    ))
                    .child(format!(
                        "{} / {}",
                        page + 1,
                        files.len().div_ceil(200).max(1)
                    ))
                    .child(self.aux_button(
                        "sftp-next",
                        "下一页",
                        |this, cx| {
                            this.auxiliary.page = (this.auxiliary.page + 1)
                                .min(this.visible_files().len().div_ceil(200).saturating_sub(1));
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .id("sftp-list")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .children(files.into_iter().skip(page * 200).take(200).map(|file| {
                        let name = file.name.clone();
                        let directory = file.directory;
                        let selected = self.auxiliary.selected.as_ref() == Some(&name);
                        div()
                            .id(SharedString::from(format!("remote-file-{name}")))
                            .flex()
                            .gap_3()
                            .px_2()
                            .py_1()
                            .bg(if selected {
                                theme.accent_soft
                            } else {
                                theme.surface
                            })
                            .cursor_pointer()
                            .child(if directory { "📁" } else { "  " })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .child(SharedString::from(name.clone())),
                            )
                            .child(div().w(px(64.)).child(file.size.to_string()))
                            .child(
                                div().w(px(112.)).text_size(px(10.)).child(
                                    file.modified
                                        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                                        .map(|d| {
                                            d.with_timezone(&chrono::Local)
                                                .format("%m-%d %H:%M")
                                                .to_string()
                                        })
                                        .unwrap_or_else(|| "—".into()),
                                ),
                            )
                            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                                if directory && event.click_count() == 2 {
                                    let path = format!(
                                        "{}/{}",
                                        this.auxiliary.loaded_path.trim_end_matches('/'),
                                        name
                                    );
                                    this.auxiliary.path.update(cx, |e, cx| e.set_text(path, cx));
                                    this.list_files(cx);
                                } else {
                                    this.auxiliary.remove_confirm = false;
                                    this.auxiliary.selected = Some(name.clone());
                                    cx.notify();
                                }
                            }))
                    })),
            )
            .into_any_element()
    }
    pub(super) fn upload_file(&mut self, cx: &mut Context<Self>) {
        if self.auxiliary.busy || self.transport.is_none() {
            return;
        }
        let generation = self.generation;
        let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("上传文件".into()),
        });
        let directory = self.remote_path(cx);
        cx.spawn(async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(mut p))) if !p.is_empty() => p.remove(0),
                _ => return,
            };
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let destination = format!("{}/{name}", directory.trim_end_matches('/'));
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let progress = this.auxiliary.progress.clone();
                progress.store(0, Ordering::Relaxed);
                this.file_operation(
                    move |sftp, cancel| {
                        let options = TransferOptions {
                            cancel: cancel.clone(),
                            ..Default::default()
                        }
                        .with_progress(move |n| {
                            progress.store(n, Ordering::Relaxed);
                        });
                        sftp.upload(&path, &destination, &options)
                            .map(|n| FileResult::Done(format!("已上传 {n} 字节")))
                    },
                    cx,
                );
            });
        })
        .detach();
    }
    pub(super) fn download_file(&mut self, cx: &mut Context<Self>) {
        if self.auxiliary.busy || self.transport.is_none() {
            return;
        }
        let generation = self.generation;
        let Some(source) = self.selected_path(cx) else {
            return;
        };
        let name = self.auxiliary.selected.clone().unwrap_or_default();
        let picker = cx.prompt_for_new_path(&superlink_app::data_root(), Some(&name));
        cx.spawn(async move |this, cx| {
            let destination = match picker.await {
                Ok(Ok(Some(p))) => p,
                _ => return,
            };
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let progress = this.auxiliary.progress.clone();
                progress.store(0, Ordering::Relaxed);
                this.file_operation(
                    move |sftp, cancel| {
                        let options = TransferOptions {
                            cancel: cancel.clone(),
                            ..Default::default()
                        }
                        .with_progress(move |n| {
                            progress.store(n, Ordering::Relaxed);
                        });
                        sftp.download(&source, &destination, &options)
                            .map(|n| FileResult::Done(format!("已下载 {n} 字节")))
                    },
                    cx,
                );
            });
        })
        .detach();
    }
    pub(in crate::terminal_page) fn refresh_monitor(&mut self, cx: &mut Context<Self>) {
        if matches!(self.target, Some(Target::Remote { .. })) && self.transport.is_none() {
            self.auxiliary.message = "远程会话尚未连接，无法读取主机监控".into();
            cx.notify();
            return;
        }
        if self.auxiliary.monitor_sampling {
            return;
        }
        self.auxiliary.monitor_sampling = true;
        self.auxiliary.monitor_job += 1;
        let job = self.auxiliary.monitor_job;
        let generation = self.generation;
        let interval = self.auxiliary.monitor.interval;
        let client = self.transport.clone();
        let local = self.auxiliary.local.clone();
        let cancel = self.auxiliary.monitor.cancel.clone();
        let task = cx.background_executor().spawn(async move {
            if let Some(client) = client {
                client.monitor(&cancel).map(|m| (m, None, None))
            } else {
                let monitor =
                    local.unwrap_or_else(|| Arc::new(std::sync::Mutex::new(LocalMonitor::new())));
                let snapshot = monitor.lock().unwrap_or_else(|e| e.into_inner()).snapshot();
                Ok((snapshot.metrics, Some(monitor), Some(snapshot.rates)))
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || this.auxiliary.monitor_job != job {
                    return;
                }
                this.auxiliary.monitor_sampling = false;
                match result {
                    Ok((metrics, local, rates)) => {
                        this.auxiliary.monitor.update(&metrics, rates);
                        this.auxiliary.metrics = Some(metrics);
                        this.auxiliary.local = local;
                    }
                    Err(e) => this.auxiliary.message = e.to_string(),
                }
                cx.notify();
            });
            cx.background_executor()
                .timer(Duration::from_secs(interval))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation
                    && this.auxiliary.monitor_job == job
                    && this.auxiliary.mode == Mode::Monitor
                    && !this.auxiliary.monitor.paused
                {
                    this.refresh_monitor(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }
}
