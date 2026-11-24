use super::*;
use widgets::bytes;

fn heading(title: &'static str, icon: &'static str, color: gpui::Hsla, theme: Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(11.))
        .text_color(theme.muted)
        .child(icons::icon(icon, 13., color))
        .child(title)
        .into_any_element()
}
impl TerminalPage {
    pub(super) fn monitor_partitions(&self, metrics: &Metrics) -> AnyElement {
        let theme = self.theme;
        let orange = gpui::rgb(0xe68e13);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_shrink_0()
            .child(heading(
                "磁盘分区",
                "shell/monitor-disk.svg",
                orange.into(),
                theme,
            ))
            .children(metrics.details.partitions.iter().take(100).map(|p| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .gap_2()
                            .text_size(px(11.))
                            .text_color(theme.muted)
                            .child(p.mount.clone())
                            .child(format!("{:.0}%", p.percent)),
                    )
                    .child(
                        div().h(px(4.)).rounded_full().bg(theme.button).child(
                            div()
                                .h_full()
                                .rounded_full()
                                .w(relative((p.percent / 100.).clamp(0., 1.) as f32))
                                .bg(orange),
                        ),
                    )
            }))
            .into_any_element()
    }
    pub(super) fn monitor_interfaces(&self, metrics: &Metrics) -> AnyElement {
        let theme = self.theme;
        let green = gpui::rgb(0x4da331);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_shrink_0()
            .child(heading(
                "网络接口",
                "shell/monitor-net.svg",
                green.into(),
                theme,
            ))
            .children(metrics.details.networks.iter().take(100).map(|n| {
                let rate = self
                    .auxiliary
                    .monitor
                    .rates
                    .networks
                    .iter()
                    .find(|r| r.name == n.name);
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .text_size(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .text_color(theme.muted)
                            .child(n.name.clone()),
                    )
                    .child(
                        div().text_color(green).child(
                            rate.map(|r| format!("↓ {}/s", bytes(r.received as u64)))
                                .unwrap_or_else(|| "↓ —".into()),
                        ),
                    )
                    .child(
                        div().text_color(gpui::rgb(0x337aff)).child(
                            rate.map(|r| format!("↑ {}/s", bytes(r.sent as u64)))
                                .unwrap_or_else(|| "↑ —".into()),
                        ),
                    )
            }))
            .into_any_element()
    }
}
