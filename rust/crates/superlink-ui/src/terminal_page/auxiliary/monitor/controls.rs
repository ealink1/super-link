use super::*;
use widgets::{bytes, section};
impl TerminalPage {
    pub(super) fn monitor_processes(
        &self,
        metrics: &Metrics,
        memory: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let mut processes = metrics.details.processes.iter().collect::<Vec<_>>();
        processes.sort_by(|a, b| {
            if memory {
                b.rss.cmp(&a.rss)
            } else {
                b.cpu.total_cmp(&a.cpu)
            }
        });
        section(
            if memory {
                "进程内存占用排行"
            } else {
                "进程 CPU 占用排行"
            },
            processes.into_iter().take(10).map(|p| {
                let process = p.clone();
                let confirming = self.auxiliary.monitor.confirm.as_ref() == Some(p);
                let item = div()
                    .flex()
                    .flex_col()
                    .min_w(px(220.))
                    .flex_1()
                    .p_3()
                    .gap_2()
                    .rounded_md()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(format!("{} · {} · {}", p.pid, p.name, p.user))
                    .child(format!(
                        "CPU {:.1}% · {} ({:.1}%)",
                        p.cpu,
                        bytes(p.rss),
                        p.memory
                    ));
                if self.transport.is_none() {
                    return item.into_any_element();
                }
                item.child(
                    div()
                        .id(("stop-process", p.pid as usize))
                        .cursor_pointer()
                        .text_color(theme.danger)
                        .child(if confirming {
                            "确认发送 SIGTERM（可能中断服务）"
                        } else {
                            "停止进程"
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.stop_monitor_process(process.clone(), cx)
                        })),
                )
                .children(confirming.then(|| {
                    self.aux_button(
                        "cancel-process-stop",
                        "取消",
                        |this, cx| {
                            this.auxiliary.monitor.confirm = None;
                            cx.notify();
                        },
                        cx,
                    )
                }))
                .into_any_element()
            }),
            theme,
        )
    }
    pub(super) fn monitor_scan(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.transport.is_none() {
            return div()
                .child("大文件分析需要已连接的 SSH 会话")
                .into_any_element();
        }
        let state = &self.auxiliary.monitor;
        let theme = self.theme;
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .child("大文件分析 · 最长 30 秒")
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(state.paths.clone()),
            )
            .child(
                div()
                    .text_color(theme.muted)
                    .child("空格分隔的绝对目录；远程权限不足的目录会跳过"),
            );
        view = view.child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(
                    [
                        (100u64 << 20, "≥100 MiB"),
                        (500 << 20, "≥500 MiB"),
                        (1 << 30, "≥1 GiB"),
                    ]
                    .map(|(minimum, label)| {
                        div()
                            .id(("scan-minimum", minimum as usize))
                            .px_2()
                            .cursor_pointer()
                            .bg(if state.minimum == minimum {
                                theme.accent_soft
                            } else {
                                theme.button
                            })
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.auxiliary.monitor.minimum = minimum;
                                cx.notify();
                            }))
                    }),
                )
                .children([20, 50, 100].map(|count| {
                    div()
                        .id(("scan-count", count))
                        .px_2()
                        .cursor_pointer()
                        .bg(if state.count == count {
                            theme.accent_soft
                        } else {
                            theme.button
                        })
                        .child(format!("Top {count}"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.auxiliary.monitor.count = count;
                            cx.notify();
                        }))
                }))
                .child(self.aux_button(
                    "scan-large-files",
                    "扫描大文件",
                    |this, cx| this.scan_large_files(cx),
                    cx,
                ))
                .child(self.aux_button(
                    "cancel-large-files",
                    "取消扫描",
                    |this, cx| {
                        this.auxiliary.monitor.cancel.cancel();
                        cx.notify();
                    },
                    cx,
                )),
        );
        view.children(
            state
                .files
                .iter()
                .enumerate()
                .take(100)
                .map(|(index, file)| {
                    let directory = file.path.clone();
                    let copy = file.path.clone();
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(format!("{} · {}", file.path, bytes(file.size)))
                        .child(
                            div()
                                .id(("copy-large-file", index))
                                .cursor_pointer()
                                .text_color(theme.accent)
                                .child("复制路径")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        copy.clone(),
                                    ))
                                })),
                        )
                        .child(
                            div()
                                .id(("open-large-file", index))
                                .cursor_pointer()
                                .text_color(theme.accent)
                                .child("所在目录")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_monitor_directory(directory.clone(), cx)
                                })),
                        )
                        .into_any_element()
                }),
        )
        .into_any_element()
    }
}
