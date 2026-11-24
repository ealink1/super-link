use super::*;

pub(super) struct ColumnTooltip {
    lines: Vec<String>,
    theme: crate::theme::Theme,
}

impl ColumnTooltip {
    pub(super) fn new(
        name: &str,
        column: Option<&superlink_drivers::ColumnEntry>,
        theme: crate::theme::Theme,
    ) -> Self {
        let mut lines = vec![name.to_owned()];
        if let Some(column) = column {
            lines.push(format!("类型：{}", column.data_type));
            lines.push(format!(
                "{} · {}",
                if column.primary_key {
                    "主键"
                } else {
                    "非主键"
                },
                if column.nullable {
                    "允许 NULL"
                } else {
                    "不允许 NULL"
                }
            ));
            if let Some(value) = &column.default_value {
                lines.push(format!("默认值：{value}"));
            }
            if !column.extra.is_empty() {
                lines.push(column.extra.clone());
            }
            if let Some(comment) = &column.comment
                && !comment.is_empty()
            {
                lines.push(format!("注释：{comment}"));
            }
        }
        Self { lines, theme }
    }
}

impl Render for ColumnTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .max_w(px(480.))
            .rounded_md()
            .border_1()
            .border_color(self.theme.border)
            .bg(self.theme.surface)
            .text_color(self.theme.text)
            .text_size(px(12.))
            .shadow_md()
            .children(self.lines.iter().cloned().map(|line| div().child(line)))
    }
}
