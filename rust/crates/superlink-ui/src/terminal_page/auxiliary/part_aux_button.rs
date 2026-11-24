use super::*;

impl TerminalPage {
    pub(super) fn aux_button(
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
    pub(in crate::terminal_page) fn auxiliary_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(40.))
            .px_3()
            .flex_shrink_0()
            .border_b_1()
            .border_color(theme.border)
            .text_size(px(12.))
            .text_color(theme.text)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::BOLD)
                    .child(match self.auxiliary.mode {
                        Mode::Files => "SFTP 文件",
                        Mode::Monitor => "服务器监控",
                        _ => "终端设置",
                    }),
            )
            .when(self.transport.is_some(), |bar| {
                bar.child(self.aux_button(
                    "panel-files",
                    "文件",
                    |this, cx| {
                        this.auxiliary.mode = Mode::Files;
                        this.list_files(cx);
                    },
                    cx,
                ))
            })
            .child(self.aux_button(
                "panel-monitor",
                "监控",
                |this, cx| {
                    this.auxiliary.mode = Mode::Monitor;
                    this.refresh_monitor(cx);
                },
                cx,
            ))
            .child(
                div()
                    .id("collapse-shell-panel")
                    .cursor_pointer()
                    .p_1()
                    .child(icons::icon("shell/panel-left-close.svg", 14., theme.muted))
                    .on_click(cx.listener(|this, _, _, cx| this.hide_panel(cx))),
            )
            .into_any_element()
    }
    pub(in crate::terminal_page) fn auxiliary_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.auxiliary.mode {
            Mode::Files => self.files_panel(cx),
            Mode::Monitor => self.monitor_panel(cx),
            Mode::Settings => div()
                .flex()
                .flex_col()
                .gap_3()
                .p_5()
                .text_color(self.theme.text)
                .child("终端设置")
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_3()
                        .child("默认本地壳")
                        .children(
                            if cfg!(windows) {
                                vec!["", "powershell.exe", "cmd.exe"]
                            } else {
                                vec!["", "/bin/zsh", "/bin/bash"]
                            }
                            .into_iter()
                            .map(|program| {
                                let active = self
                                    .settings
                                    .shell_default_program()
                                    .as_deref()
                                    .unwrap_or("")
                                    == program;
                                div()
                                    .id(SharedString::from(format!("default-shell-{program}")))
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if active {
                                        self.theme.accent_soft
                                    } else {
                                        self.theme.button
                                    })
                                    .child(if program.is_empty() {
                                        "系统默认"
                                    } else {
                                        program
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.settings.set(
                                            "shell.defaultProgram",
                                            serde_json::json!(program),
                                        );
                                        cx.notify();
                                    }))
                            }),
                        ),
                )
                .child(
                    div().flex().flex_wrap().gap_3().child("字号").children(
                        [10, 12, 13, 14, 15, 16, 18, 20, 24]
                            .into_iter()
                            .map(|size| {
                                div()
                                    .id(("terminal-font-size", size as usize))
                                    .cursor_pointer()
                                    .px_2()
                                    .child(size.to_string())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.settings
                                            .set("shell.fontSize", serde_json::json!(size));
                                        cx.notify();
                                    }))
                            }),
                    ),
                )
                .child(self.aux_button(
                    "toggle-reconnect",
                    if self.settings.shell_reconnect_enabled() {
                        "自动重连：开"
                    } else {
                        "自动重连：关"
                    },
                    |this, cx| {
                        this.settings.set(
                            "shell.autoReconnect",
                            serde_json::json!(!this.settings.shell_reconnect_enabled()),
                        );
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.aux_button(
                    "terminal-clear",
                    "清空终端",
                    |this, cx| {
                        this.screen = Screen::new(this.screen.cols(), this.screen.rows());
                        this.scroll_offset = 0;
                        cx.notify();
                    },
                    cx,
                ))
                .child(self.aux_button(
                    "terminal-copy-screen",
                    "复制屏幕",
                    |this, cx| {
                        let text = (0..this.screen.rows())
                            .map(|i| {
                                this.screen
                                    .visible_row(i, this.scroll_offset)
                                    .0
                                    .trim_end()
                                    .to_string()
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                    },
                    cx,
                ))
                .into_any_element(),
            Mode::Terminal => div().into_any_element(),
        }
    }
    pub(super) fn remote_path(&self, cx: &Context<Self>) -> String {
        self.auxiliary.path.read(cx).text().trim().to_string()
    }
    pub(super) fn selected_path(&self, cx: &Context<Self>) -> Option<String> {
        let name = self.auxiliary.selected.as_ref()?;
        let _ = cx;
        Some(superlink_terminal::files::remote_join(
            &self.auxiliary.loaded_path,
            name,
        ))
    }
    pub(super) fn file_operation(
        &mut self,
        operation: impl FnOnce(&SftpClient, &CancelToken) -> Result<FileResult, TerminalError>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.auxiliary.busy {
            return;
        }
        let Some(client) = self.transport.clone() else {
            self.auxiliary.message = "请先连接远程主机".into();
            cx.notify();
            return;
        };
        self.auxiliary.busy = true;
        self.auxiliary.file_job += 1;
        let job = self.auxiliary.file_job;
        self.auxiliary.cancel = CancelToken::new();
        let cancel = self.auxiliary.cancel.clone();
        let generation = self.generation;
        let task = cx.background_executor().spawn(async move {
            let sftp = client.open_sftp()?;
            let result = operation(&sftp, &cancel);
            let _ = sftp.close();
            result
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || this.auxiliary.file_job != job {
                    return;
                }
                this.auxiliary.busy = false;
                match result {
                    Ok(FileResult::List(path, mut files)) => {
                        files.sort_by(|a, b| {
                            b.directory
                                .cmp(&a.directory)
                                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                                .then_with(|| a.name.cmp(&b.name))
                        });
                        this.auxiliary.loaded_path = path;
                        this.auxiliary.files = files;
                        this.auxiliary.page = 0;
                        this.auxiliary.selected = None;
                        this.auxiliary.message = "目录已加载".into();
                    }
                    Ok(FileResult::Done(message)) => {
                        this.auxiliary.message = message;
                        this.list_files(cx);
                    }
                    Err(error) => this.auxiliary.message = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(125))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    if this.generation != generation
                        || this.auxiliary.file_job != job
                        || !this.auxiliary.busy
                    {
                        return false;
                    }
                    cx.notify();
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    pub(in crate::terminal_page) fn list_files(&mut self, cx: &mut Context<Self>) {
        let path = self.remote_path(cx);
        self.file_operation(
            move |sftp, cancel| {
                sftp.list(&path, cancel)
                    .map(|files| FileResult::List(path, files))
            },
            cx,
        );
    }
}
