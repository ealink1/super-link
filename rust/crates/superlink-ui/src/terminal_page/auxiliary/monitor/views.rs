use super::*;
use widgets::{bytes, card, chart, ratio, section};
impl TerminalPage {
    pub(in crate::terminal_page) fn monitor_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let state = &self.auxiliary.monitor;
        let mut panel = div()
            .relative()
            .bg(theme.canvas)
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .text_size(px(12.))
            .text_color(theme.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .px_1()
                    .h(px(40.))
                    .text_size(px(10.))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| {
                            this.auxiliary.monitor.menu = !this.auxiliary.monitor.menu;
                            cx.notify();
                        }),
                    )
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(theme.border)
                    .children(
                        ["综合", "CPU", "GPU", "MEM", "DISK", "NET"]
                            .into_iter()
                            .enumerate()
                            .map(|(index, label)| {
                                div()
                                    .id(("monitor-tab", index))
                                    .cursor_pointer()
                                    .px_1()
                                    .py_1()
                                    .rounded_md()
                                    .bg(if state.view == index {
                                        theme.accent_soft
                                    } else {
                                        theme.canvas
                                    })
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(icons::icon(
                                        [
                                            "shell/monitor-overview.svg",
                                            "shell/monitor-cpu.svg",
                                            "shell/monitor-gpu.svg",
                                            "shell/monitor-mem.svg",
                                            "shell/monitor-disk.svg",
                                            "shell/monitor-net.svg",
                                        ][index],
                                        12.,
                                        if state.view == index {
                                            theme.accent
                                        } else {
                                            theme.muted
                                        },
                                    ))
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.auxiliary.monitor.view = index;
                                        cx.notify();
                                    }))
                            }),
                    )
                    .child(div().flex_1())
                    .child(self.aux_button(
                        "monitor-collapse",
                        "›",
                        |this, cx| this.hide_panel(cx),
                        cx,
                    )),
            );
        let mut content = div()
            .id("monitor-details")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .gap_2()
            .p_3()
            .children(
                (!self.auxiliary.message.is_empty())
                    .then(|| SharedString::from(self.auxiliary.message.clone())),
            )
            .children((!state.notice.is_empty()).then(|| SharedString::from(state.notice.clone())));
        let Some(metrics) = &self.auxiliary.metrics else {
            return panel
                .child(content.child(if self.auxiliary.monitor_sampling {
                    "正在读取监控…"
                } else {
                    "暂无监控数据，请连接后刷新"
                }))
                .into_any_element();
        };
        let d = &metrics.details;
        let local = self.transport.is_none();
        let (down, up) = host_totals(&state.rates);
        let memory = || {
            card(
                "内存",
                format!(
                    "{:.0}%",
                    ratio(metrics.memory_used, metrics.memory_total) * 100.
                ),
                format!(
                    "{} / {}",
                    bytes(metrics.memory_used),
                    bytes(metrics.memory_total)
                ),
                Some(ratio(metrics.memory_used, metrics.memory_total)),
                theme,
            )
        };
        let network = || {
            card(
                "主机吞吐量",
                if state.rates.ready {
                    format!("↓ {} /s", bytes(down as u64))
                } else {
                    "采样中".into()
                },
                format!("↑ {} /s · 不含回环和容器接口", bytes(up as u64)),
                None,
                theme,
            )
        };
        match state.view {
            0 => {
                content = content.child(self.monitor_overview(metrics, cx));
            }
            1 => {
                content = content
                    .child(self.cpu_details(metrics))
                    .child(self.monitor_processes(metrics, false, cx));
            }
            2 => {
                if d.gpus.is_empty() {
                    content = content
                        .child(card(
                            "GPU",
                            "未检测到 NVIDIA GPU".into(),
                            "服务器无 NVIDIA GPU，或 nvidia-smi 不可用".into(),
                            None,
                            theme,
                        ))
                        .child(self.aux_button(
                            "copy-gpu-probe",
                            "复制诊断命令 nvidia-smi",
                            |_, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    "nvidia-smi".into(),
                                ))
                            },
                            cx,
                        ));
                } else {
                    content = content.child(section(
                        "GPU",
                        d.gpus.iter().take(32).map(|g| {
                            card(
                                &g.name,
                                format!("{:.0}%", g.utilization),
                                format!(
                                    "显存 {} / {} · {:.0}°C",
                                    bytes(g.used),
                                    bytes(g.total),
                                    g.temperature
                                ),
                                Some(g.utilization / 100.),
                                theme,
                            )
                        }),
                        theme,
                    ));
                }
            }
            3 => {
                content = content
                    .child(memory())
                    .child(section(
                        "内存分布",
                        [
                            ("可用内存", d.memory_available),
                            ("页面缓存 Cached", d.cached),
                            ("缓冲区 Buffers", d.buffers),
                            ("空闲内存 Free", d.memory_free),
                        ]
                        .map(|(label, value)| {
                            let unsupported =
                                local && matches!(label, "页面缓存 Cached" | "缓冲区 Buffers");
                            card(
                                label,
                                if unsupported {
                                    "未采集".into()
                                } else {
                                    bytes(value)
                                },
                                String::new(),
                                None,
                                theme,
                            )
                        }),
                        theme,
                    ))
                    .child(card(
                        "Swap 交换分区",
                        format!("{} / {}", bytes(d.swap_used), bytes(d.swap_total)),
                        String::new(),
                        Some(ratio(d.swap_used, d.swap_total)),
                        theme,
                    ))
                    .child(self.monitor_processes(metrics, true, cx));
            }
            4 => {
                content = content
                    .child(self.monitor_partitions(metrics))
                    .child(section(
                        "磁盘 IO · 每秒读写操作",
                        state.rates.disks.iter().take(100).map(|d| {
                            card(
                                &d.name,
                                format!("读 {:.0} IOPS", d.reads),
                                format!("写 {:.0} IOPS", d.writes),
                                None,
                                theme,
                            )
                        }),
                        theme,
                    ))
                    .children((local && d.disks.is_empty()).then_some("本地探针未采集磁盘 IO"))
                    .child(self.monitor_scan(cx));
            }
            _ => {
                content = content
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(card(
                                "连接数",
                                if local {
                                    "未采集".into()
                                } else {
                                    d.connections.to_string()
                                },
                                if local {
                                    "连接统计由远程 Linux 探针提供".into()
                                } else {
                                    format!(
                                        "TCP {} · UDP {} · TIME_WAIT {}",
                                        d.tcp_connections, d.udp_connections, d.time_wait
                                    )
                                },
                                None,
                                theme,
                            ))
                            .child(card(
                                "文件句柄",
                                if local {
                                    "未采集".into()
                                } else {
                                    d.handles.to_string()
                                },
                                if local {
                                    "句柄统计由远程 Linux 探针提供".into()
                                } else {
                                    format!("上限 {}", d.handle_limit)
                                },
                                (!local).then(|| ratio(d.handles, d.handle_limit)),
                                theme,
                            ))
                            .child(network()),
                    )
                    .child(chart(&state.history, true, theme))
                    .child(self.monitor_interfaces(metrics))
                    .children(local.then_some("本地探针未采集监听端口与接口地址"))
                    .child(section(
                        "监听端口（最多 100 项）",
                        d.listeners.iter().take(100).enumerate().map(|(index, l)| {
                            let address = l.address.clone();
                            div()
                                .id(("listener", index))
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .bg(theme.surface)
                                .child(format!("{} · {}", l.protocol, l.address))
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        address.clone(),
                                    ))
                                }))
                                .into_any_element()
                        }),
                        theme,
                    ));
            }
        }
        panel = panel
            .child(content)
            .child(self.monitor_footer(cx))
            .children(state.menu.then(|| self.monitor_menu(cx)));
        panel.into_any_element()
    }
}
