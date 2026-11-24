use super::*;
use gpui::{Hsla, PathBuilder, canvas, point};
pub(super) fn bytes(value: u64) -> String {
    for (size, unit) in [
        (1u64 << 40, "TiB"),
        (1 << 30, "GiB"),
        (1 << 20, "MiB"),
        (1 << 10, "KiB"),
    ] {
        if value >= size {
            return format!("{:.1} {unit}", value as f64 / size as f64);
        }
    }
    format!("{value} B")
}
pub(super) fn ratio(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.
    } else {
        (used as f64 / total as f64).clamp(0., 1.)
    }
}
pub(super) fn card(
    title: &str,
    value: String,
    caption: String,
    progress: Option<f64>,
    theme: Theme,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        // Flex wrapping uses this minimum before distributing the row's space.
        // A zero minimum squeezes every CPU core into the same narrow row.
        .min_w(px(160.))
        .gap_2()
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme.muted)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(px(14.))
                .whitespace_nowrap()
                .font_weight(gpui::FontWeight::BOLD)
                .child(value),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme.muted)
                .child(caption),
        )
        .children(progress.map(|progress| {
            div()
                .w_full()
                .h(px(5.))
                .rounded_full()
                .bg(theme.button)
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(progress.clamp(0., 1.) as f32))
                        .rounded_full()
                        .bg(theme.accent),
                )
        }))
        .into_any_element()
}
pub(super) fn section(
    title: &str,
    children: impl IntoIterator<Item = AnyElement>,
    theme: Theme,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(0.))
        .flex_shrink_0()
        .gap_2()
        .child(
            div()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(theme.muted)
                .child(title.to_string()),
        )
        .child(div().flex().flex_wrap().gap_3().children(children))
        .into_any_element()
}
pub(super) fn chart(history: &VecDeque<Sample>, network: bool, theme: Theme) -> AnyElement {
    let now = Instant::now();
    let first = history
        .iter()
        .map(|p| {
            (
                (1. - now.saturating_duration_since(p.at).as_secs_f32() / 60.).clamp(0., 1.),
                if network { p.down } else { p.cpu },
            )
        })
        .collect::<Vec<_>>();
    let second = history
        .iter()
        .map(|p| {
            (
                (1. - now.saturating_duration_since(p.at).as_secs_f32() / 60.).clamp(0., 1.),
                if network { p.up } else { 0. },
            )
        })
        .collect::<Vec<_>>();
    let scale = if network {
        first
            .iter()
            .chain(&second)
            .map(|(_, value)| *value)
            .fold(1., f64::max)
    } else {
        100.
    };
    let draw = move |values: &[(f32, f64)],
                     color: Hsla,
                     bounds: gpui::Bounds<gpui::Pixels>,
                     window: &mut Window| {
        if values.len() < 2 {
            return;
        }
        let mut area = PathBuilder::fill();
        area.move_to(point(
            bounds.left() + bounds.size.width * values[0].0,
            bounds.bottom(),
        ));
        for (position, value) in values {
            area.line_to(point(
                bounds.left() + bounds.size.width * *position,
                bounds.bottom() - bounds.size.height * (value / scale).clamp(0., 1.) as f32,
            ));
        }
        area.line_to(point(
            bounds.left() + bounds.size.width * values.last().unwrap().0,
            bounds.bottom(),
        ));
        area.close();
        if let Ok(area) = area.build() {
            let mut shade = color;
            shade.a = 0.08;
            window.paint_path(area, shade);
        }
        let mut path = PathBuilder::stroke(px(1.5));
        for (i, (position, value)) in values.iter().enumerate() {
            let x = bounds.left() + bounds.size.width * *position;
            let y = bounds.bottom() - bounds.size.height * (value / scale).clamp(0., 1.) as f32;
            if i == 0 {
                path.move_to(point(x, y));
            } else {
                path.line_to(point(x, y));
            }
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    };
    div()
        .flex()
        .flex_col()
        .gap_2()
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
                .text_size(px(11.))
                .text_color(theme.muted)
                .child(icons::icon(
                    "shell/monitor-trend.svg",
                    14.,
                    gpui::rgb(0x1799a0).into(),
                ))
                .child(if network {
                    "流量历史 · 最近 60 秒"
                } else {
                    "CPU 负载趋势"
                }),
        )
        .child(
            canvas(
                move |_, _, _| {},
                move |bounds, _, window, _| {
                    for fraction in [0., 0.5, 1.] {
                        let y = bounds.top() + bounds.size.height * fraction;
                        let mut grid = PathBuilder::stroke(px(0.5));
                        let mut x = bounds.left();
                        while x < bounds.right() {
                            grid.move_to(point(x, y));
                            grid.line_to(point((x + px(2.)).min(bounds.right()), y));
                            x += px(6.);
                        }
                        if let Ok(grid) = grid.build() {
                            window.paint_path(grid, theme.border);
                        }
                    }
                    draw(&first, gpui::rgb(0x1799a0).into(), bounds, window);
                    if network {
                        draw(&second, theme.accent, bounds, window);
                    }
                },
            )
            .w_full()
            .h(px(72.)),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .text_size(px(10.))
                .text_color(theme.muted)
                .children([60, 40, 20, 0].map(|seconds| {
                    div().child(
                        (chrono::Local::now() - chrono::Duration::seconds(seconds))
                            .format("%H:%M:%S")
                            .to_string(),
                    )
                })),
        )
        .when(network, |chart| {
            chart.child(
                div()
                    .text_size(px(11.))
                    .text_color(theme.muted)
                    .child(format!(
                        "绿色下载 / 蓝色上传 · 峰值 {}/s",
                        bytes(scale as u64)
                    )),
            )
        })
        .into_any_element()
}
