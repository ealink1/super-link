use super::*;
use widgets::{bytes, chart, ratio};

impl TerminalPage {
    pub(super) fn monitor_overview(&self, metrics: &Metrics, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let state = &self.auxiliary.monitor;
        let d = &metrics.details;
        let cpu = cpu_total(&state.rates);
        let (down, up) = host_totals(&state.rates);
        let (disk_used, disk_total) = d
            .partitions
            .iter()
            .find(|p| p.mount == "/")
            .map(|p| (p.used, p.total))
            .unwrap_or((0, 0));
        let (name, address, user) = match &self.target {
            Some(Target::Remote { host, .. }) => {
                (host.name.clone(), host.host.clone(), host.user.clone())
            }
            _ => ("本地终端".into(), String::new(), String::new()),
        };
        let cpu_value = if state.rates.ready {
            format!("{cpu:.1}%")
        } else {
            "采样中".into()
        };
        let cards = [
            compact_card(
                "CPU",
                cpu_value,
                state
                    .rates
                    .cpus
                    .iter()
                    .find(|c| c.name == "cpu")
                    .map(|c| format!("User: {:.1}% / Sys: {:.1}%", c.user, c.system))
                    .unwrap_or_else(|| {
                        format!("{} 核 / 负载 {}", state.rates.cpus.len(), metrics.load)
                    }),
                cpu / 100.,
                0x1799a0,
                theme,
            ),
            compact_card(
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
                ratio(metrics.memory_used, metrics.memory_total),
                0xdf74c5,
                theme,
            ),
            compact_card(
                "磁盘",
                format!("{:.0}%", ratio(disk_used, disk_total) * 100.),
                format!("{} / {}", bytes(disk_used), bytes(disk_total)),
                ratio(disk_used, disk_total),
                0xe68e13,
                theme,
            ),
            compact_card(
                "网络",
                format!("↓ {}/s", bytes(down as u64)),
                format!("↑ {}/s", bytes(up as u64)),
                0.,
                0x4da331,
                theme,
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(icons::icon(
                        "shell/server.svg",
                        18.,
                        gpui::rgb(0x4da331).into(),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().font_weight(FontWeight::BOLD).child(name))
                                    .when(!address.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .px_1()
                                                .rounded_md()
                                                .bg(theme.button)
                                                .text_color(gpui::rgb(0x1799a0))
                                                .text_size(px(10.))
                                                .child(address),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(10.))
                                    .when(!user.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .px_1()
                                                .rounded_md()
                                                .bg(theme.button)
                                                .text_color(gpui::rgb(0xdf74c5))
                                                .child(user),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_color(theme.muted)
                                            .child(format!("{} {}", metrics.system, d.kernel)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap_1()
                            .text_size(px(10.))
                            .text_color(theme.muted)
                            .child("运行时间")
                            .child(
                                div()
                                    .font_family("monospace")
                                    .text_size(px(13.))
                                    .text_color(gpui::rgb(0x4da331))
                                    .child(metrics.uptime.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(cards.into_iter().enumerate().map(|(index, card)| {
                        div()
                            .id(("monitor-overview-card", index))
                            .w(relative(0.485))
                            .cursor_pointer()
                            .child(card)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.auxiliary.monitor.view = [1, 3, 4, 5][index];
                                cx.notify();
                            }))
                    })),
            )
            .child(chart(&state.history, false, theme))
            .child(self.monitor_partitions(metrics))
            .child(self.monitor_interfaces(metrics))
            .into_any_element()
    }
}

fn compact_card(
    title: &str,
    value: String,
    caption: String,
    progress: f64,
    tint: u32,
    theme: Theme,
) -> AnyElement {
    let color = gpui::rgb(tint);
    if title == "网络" {
        return div()
            .flex()
            .flex_col()
            .justify_between()
            .w_full()
            .h(px(88.))
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(theme.muted)
                    .child(icons::icon("shell/monitor-net.svg", 14., color.into()))
                    .child("网络"),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap_1()
                    .font_family("monospace")
                    .text_size(px(11.))
                    .child(div().text_color(color).child(value))
                    .child(div().text_color(gpui::rgb(0x337aff)).child(caption)),
            )
            .child(div().h(px(1.)))
            .into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .justify_between()
        .w_full()
        .h(px(88.))
        .text_size(px(10.))
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_color(color)
                .child(
                    div()
                        .text_color(theme.muted)
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(icons::icon(
                            match title {
                                "CPU" => "shell/monitor-cpu.svg",
                                "内存" => "shell/monitor-mem.svg",
                                "磁盘" => "shell/monitor-disk.svg",
                                _ => "shell/monitor-net.svg",
                            },
                            14.,
                            color.into(),
                        ))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .font_family("monospace")
                        .text_size(px(if title == "网络" { 12. } else { 20. }))
                        .whitespace_nowrap()
                        .child(value),
                ),
        )
        .child(
            div()
                .h(px(5.))
                .w_full()
                .rounded_full()
                .bg(theme.button)
                .child(
                    div()
                        .h_full()
                        .w(relative(progress.clamp(0., 1.) as f32))
                        .bg(color),
                ),
        )
        .child(
            div().text_size(px(10.)).text_color(theme.muted).child(
                div().flex().justify_between().gap_1().children(
                    caption
                        .split(" / ")
                        .map(|part| div().child(part.to_string())),
                ),
            ),
        )
        .into_any_element()
}

impl TerminalPage {
    pub(super) fn monitor_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let directory = if self.auxiliary.loaded_path.is_empty() {
            "—"
        } else {
            &self.auxiliary.loaded_path
        };
        let dirs = self.auxiliary.files.iter().filter(|f| f.directory).count();
        let files = self.auxiliary.files.len() - dirs;
        let total: u64 = self
            .auxiliary
            .files
            .iter()
            .filter(|f| !f.directory)
            .map(|f| f.size.max(0) as u64)
            .sum();
        div()
            .flex()
            .gap_1()
            .p_2()
            .flex_shrink_0()
            .children(
                [
                    ("PATH", directory.to_string(), "shell/monitor-path.svg"),
                    ("DIR", dirs.to_string(), "shell/monitor-dir.svg"),
                    ("FILE", files.to_string(), "shell/monitor-file.svg"),
                    ("TOTAL", bytes(total), "shell/monitor-disk.svg"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (title, value, icon))| {
                    div()
                        .id(("monitor-file-stat", index))
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(px(0.))
                        .h(px(46.))
                        .p_1()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.button)
                        .cursor_pointer()
                        .text_size(px(10.))
                        .text_color(theme.muted)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(icons::icon(icon, 12., theme.muted))
                                .child(title),
                        )
                        .child(div().overflow_hidden().child(value))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_panel(super::super::Mode::Files, cx);
                        }))
                }),
            )
            .into_any_element()
    }
}
