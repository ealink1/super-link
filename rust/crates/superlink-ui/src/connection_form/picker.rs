//! 新建连接的第一步：按分类、搜索选择数据源类型。
//!
//! 对齐 Fyne 版 `datasource_picker`：左侧分类计数导航，右侧卡片网格，
//! 顶部搜索框与「1 选类型 › 2 配参数 › 3 保存」步骤条。

use gpui::{
    App, Context, Entity, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
};

use superlink_domain::{Family, catalog};

use crate::editor::SqlEditor;
use crate::theme::{TEXT_SIZE, Theme};

pub(super) const CATEGORIES: [&str; 9] = [
    "全部",
    "关系型数据库",
    "国产数据库",
    "NoSQL 数据库",
    "向量数据库",
    "时序数据库",
    "消息队列",
    "配置中心",
    "其他",
];

const DOMESTIC: [&str; 8] = [
    "goldendb",
    "dameng",
    "kingbase",
    "highgo",
    "vastbase",
    "opengauss",
    "gaussdb",
    "oceanbase",
];
const TIME_SERIES: [&str; 2] = ["tdengine", "iotdb"];

/// 与 Fyne 版相同的归类规则（国产库、时序库优先于通用族）。
pub(super) fn category_of(key: &str, family: Family) -> &'static str {
    if DOMESTIC.contains(&key) {
        return "国产数据库";
    }
    if TIME_SERIES.contains(&key) {
        return "时序数据库";
    }
    if key == "custom" {
        return "其他";
    }
    match family {
        Family::Sql => "关系型数据库",
        Family::Cache | Family::Document | Family::Search => "NoSQL 数据库",
        Family::Vector => "向量数据库",
        Family::Message => "消息队列",
        Family::Configuration => "配置中心",
    }
}

pub(super) struct SourcePicker {
    pub open: bool,
    category: &'static str,
    search: Entity<SqlEditor>,
    theme: Theme,
}

impl SourcePicker {
    pub fn new(theme: Theme, cx: &mut Context<SourcePicker>) -> Self {
        let search = cx.new(SqlEditor::new);
        search.update(cx, |editor, cx| {
            editor.set_placeholder("搜索数据源类型", cx);
        });
        // 输入变化时重绘结果网格。
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            open: false,
            category: "全部",
            search,
            theme,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<SourcePicker>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<SourcePicker>) {
        self.open = true;
        self.category = "全部";
        self.search
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        let handle = self.search.read(cx).focus_handle(cx);
        window.focus(&handle);
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<SourcePicker>) {
        self.open = false;
        cx.notify();
    }

    /// 当前搜索词与分类过滤出的目录条目。
    pub(super) fn matches(&self, cx: &App) -> Vec<&'static superlink_domain::Descriptor> {
        let needle = self.search.read(cx).text().trim().to_lowercase();
        catalog()
            .iter()
            .filter(|descriptor| {
                if self.category != "全部"
                    && category_of(descriptor.key, descriptor.family) != self.category
                {
                    return false;
                }
                needle.is_empty()
                    || descriptor.name.to_lowercase().contains(&needle)
                    || descriptor.key.contains(&needle)
            })
            .collect()
    }

    pub(super) fn count(&self, category: &str) -> usize {
        catalog()
            .iter()
            .filter(|descriptor| {
                category == "全部" || category_of(descriptor.key, descriptor.family) == category
            })
            .count()
    }

    pub(super) fn category(&self) -> &'static str {
        self.category
    }

    pub(super) fn set_category(&mut self, category: &'static str, cx: &mut Context<SourcePicker>) {
        self.category = category;
        cx.notify();
    }

    pub(super) fn search_editor(&self) -> Entity<SqlEditor> {
        self.search.clone()
    }
}

pub(super) fn category_label(key: &str) -> &'static str {
    match key {
        "全部" => "全部",
        other => CATEGORIES
            .iter()
            .find(|item| **item == other)
            .copied()
            .unwrap_or("全部"),
    }
}

/// 数据源卡片：图标、名称、key 与默认端口。
pub(super) fn source_card(
    descriptor: &'static superlink_domain::Descriptor,
    theme: Theme,
) -> impl IntoElement {
    let port = descriptor
        .port
        .filter(|port| *port > 0)
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    div()
        .flex()
        .items_center()
        .gap_2()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.canvas)
        .hover(move |style| style.bg(theme.accent_soft))
        .child(crate::icons::icon(
            crate::icons::datasource_icon(descriptor.key),
            22.0,
            theme.text,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(TEXT_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(descriptor.name),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme.muted)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(format!("{}{port}", descriptor.key)),
                ),
        )
}

/// 步骤条：当前步骤高亮。
pub(super) fn step_strip(active: usize, theme: Theme) -> impl IntoElement {
    let labels = ["1  选类型", "2  配参数", "3  保存"];
    let mut strip = div().flex().items_center().gap_1();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            strip = strip.child(div().text_size(px(11.0)).text_color(theme.muted).child("—"));
        }
        let on = index == active;
        strip = strip.child(
            div()
                .px_2()
                .py(px(2.0))
                .rounded_full()
                .text_size(px(11.0))
                .text_color(if on { theme.surface } else { theme.muted })
                .bg(if on { theme.accent } else { theme.canvas })
                .child(SharedString::from(*label)),
        );
    }
    strip
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 分类必须与 Fyne 版一致：国产库、时序库优先，其余按 data source family。
    #[test]
    fn categories_match_the_legacy_picker() {
        for key in [
            "goldendb",
            "dameng",
            "kingbase",
            "highgo",
            "vastbase",
            "opengauss",
            "gaussdb",
            "oceanbase",
        ] {
            let descriptor = superlink_domain::resolve(key).unwrap();
            assert_eq!(
                category_of(descriptor.key, descriptor.family),
                "国产数据库",
                "{key}"
            );
        }
        for key in ["tdengine", "iotdb"] {
            let descriptor = superlink_domain::resolve(key).unwrap();
            assert_eq!(
                category_of(descriptor.key, descriptor.family),
                "时序数据库",
                "{key}"
            );
        }
        let mysql = superlink_domain::resolve("mysql").unwrap();
        assert_eq!(category_of(mysql.key, mysql.family), "关系型数据库");
        let redis = superlink_domain::resolve("redis").unwrap();
        assert_eq!(category_of(redis.key, redis.family), "NoSQL 数据库");
        let milvus = superlink_domain::resolve("milvus").unwrap();
        assert_eq!(category_of(milvus.key, milvus.family), "向量数据库");
        let kafka = superlink_domain::resolve("kafka").unwrap();
        assert_eq!(category_of(kafka.key, kafka.family), "消息队列");
        let nacos = superlink_domain::resolve("nacos").unwrap();
        assert_eq!(category_of(nacos.key, nacos.family), "配置中心");
        assert_eq!(category_of("custom", Family::Sql), "其他");
    }

    #[test]
    fn every_catalog_entry_lands_in_a_known_category() {
        for descriptor in superlink_domain::catalog() {
            let category = category_of(descriptor.key, descriptor.family);
            assert!(CATEGORIES.contains(&category), "{:?}", descriptor.key);
        }
    }
}
