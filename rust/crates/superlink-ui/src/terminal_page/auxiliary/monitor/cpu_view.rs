use super::*;
use widgets::chart;

impl TerminalPage {
    pub(super) fn cpu_details(&self, metrics: &Metrics) -> AnyElement {
        let theme = self.theme;
        let state = &self.auxiliary.monitor;
        let rates = &state.rates;
        let cores = rates
            .cpus
            .iter()
            .filter(|c| c.name != "cpu")
            .collect::<Vec<_>>();
        let aggregate = rates.cpus.iter().find(|c| c.name == "cpu");
        let average = |f: fn(&superlink_terminal::monitor::rates::CpuRate) -> f64| {
            aggregate.map(f).unwrap_or_else(|| {
                cores.iter().map(|c| f(c)).sum::<f64>() / cores.len().max(1) as f64
            })
        };
        let load = metrics.load.split_whitespace().collect::<Vec<_>>();
        let values = [
            (
                "总负载",
                format!("{:.1}%", cpu_total(rates)),
                format!("核心数: {}", cores.len()),
            ),
            (
                "负载均值 (1M)",
                load.first().copied().unwrap_or("—").to_string(),
                load.iter()
                    .skip(1)
                    .take(2)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" / "),
            ),
            (
                "CPU 温度",
                if metrics.details.temperature > 0. {
                    format!("{:.1}°C", metrics.details.temperature)
                } else {
                    "N/A".into()
                },
                String::new(),
            ),
            (
                "用户态 (USER)",
                if self.transport.is_none() {
                    "N/A".into()
                } else {
                    format!("{:.1}%", average(|c| c.user))
                },
                String::new(),
            ),
            (
                "内核态 (SYS)",
                if self.transport.is_none() {
                    "N/A".into()
                } else {
                    format!("{:.1}%", average(|c| c.system))
                },
                String::new(),
            ),
            (
                "IO 等待",
                if self.transport.is_none() {
                    "N/A".into()
                } else {
                    format!("{:.1}%", average(|c| c.wait))
                },
                String::new(),
            ),
        ];
        div()
            .flex()
            .flex_col()
            .gap_3()
            .flex_shrink_0()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(values.into_iter().enumerate().map(
                        |(i, (title, value, caption))| {
                            let tint = [
                                theme.text,
                                gpui::rgb(0x4da331).into(),
                                gpui::rgb(0x4da331).into(),
                                gpui::rgb(0x1799a0).into(),
                                gpui::rgb(0xdf74c5).into(),
                                gpui::rgb(0xe68e13).into(),
                            ][i];
                            div()
                                .flex()
                                .flex_col()
                                .w(relative(0.31))
                                .flex_grow()
                                .min_w(px(88.))
                                .gap_1()
                                .p_3()
                                .h(px(88.))
                                .rounded_lg()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface)
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .line_height(px(14.))
                                        .text_color(theme.muted)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_size(px(20.))
                                        .line_height(px(24.))
                                        .font_family("monospace")
                                        .text_color(tint)
                                        .whitespace_nowrap()
                                        .child(value),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .line_height(px(14.))
                                        .text_color(theme.muted)
                                        .child(caption),
                                )
                        },
                    )),
            )
            .child(chart(&state.history, false, theme))
            .child(
                div()
                    .text_color(theme.muted)
                    .text_size(px(11.))
                    .child("核心详情"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(cores.into_iter().take(256).map(|c| {
                        // Four compact tiles per row, with a minimum that also works in
                        // the narrowest supported monitor sidebar.
                        div()
                            .flex()
                            .flex_col()
                            .w(relative(0.22))
                            .min_w(px(58.))
                            .flex_grow()
                            .p_2()
                            .gap_1()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.surface)
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .gap_1()
                                    .text_size(px(10.))
                                    .text_color(theme.muted)
                                    .child(c.name.clone())
                                    .child(format!("{:.0}%", c.used)),
                            )
                            .child(
                                div().h(px(4.)).rounded_full().bg(theme.button).child(
                                    div()
                                        .h_full()
                                        .rounded_full()
                                        .w(relative((c.used / 100.).clamp(0., 1.) as f32))
                                        .bg(gpui::rgb(0x1799a0)),
                                ),
                            )
                    })),
            )
            .into_any_element()
    }
}
