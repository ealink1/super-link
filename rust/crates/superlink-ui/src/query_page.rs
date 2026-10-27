//! SQL workspace: query tabs, toolbar, statement input and a result table.

use std::sync::Arc;

use gpui::{Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px};
use serde_json::Value;
use superlink_app::{Credentials, DraftService, ProfileService, QueryService};
use superlink_domain::{Draft, Profile, ResultSet};

use crate::editor::SqlEditor;
use crate::icons;
use crate::theme::Theme;

const DISPLAY_ROWS: usize = 300;
const COLUMN_WIDTH: f32 = 160.0;
const ROW_NUMBER_WIDTH: f32 = 52.0;
const DEFAULT_LIMIT: u32 = 5_000;

struct QueryTab {
    id: String,
    title: String,
    text: String,
}

pub struct QueryPage {
    profiles: Arc<ProfileService>,
    queries: Arc<QueryService>,
    drafts: Arc<DraftService>,
    theme: Theme,
    editor: Entity<SqlEditor>,
    profile: Option<Profile>,
    tabs: Vec<QueryTab>,
    active: usize,
    result: Option<ResultSet>,
    error: Option<SharedString>,
    status: SharedString,
    running: bool,
}

impl QueryPage {
    pub fn new(
        profiles: Arc<ProfileService>,
        queries: Arc<QueryService>,
        drafts: Arc<DraftService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            profiles,
            queries,
            drafts,
            theme,
            editor: cx.new(SqlEditor::new),
            profile: None,
            tabs: Vec::new(),
            active: 0,
            result: None,
            error: None,
            status: "选择连接后执行查询".into(),
            running: false,
        }
    }

    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn select_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        let supported = self.queries.supports(profile.sql_dialect());
        self.tabs = self.load_tabs(&profile);
        self.active = 0;
        let text = self
            .tabs
            .first()
            .map(|tab| tab.text.clone())
            .unwrap_or_default();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        self.profile = Some(profile);
        self.result = None;
        self.error = None;
        self.status = if supported {
            "就绪 · 结果按行数上限截断".into()
        } else {
            "该数据源尚未提供 Rust 驱动 agent".into()
        };
        cx.notify();
    }

    fn load_tabs(&self, profile: &Profile) -> Vec<QueryTab> {
        let mut tabs: Vec<QueryTab> = self
            .drafts
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|draft| draft.profile_id == profile.id && !draft.closed)
            .map(|draft| QueryTab {
                id: draft.id,
                title: draft.title,
                text: draft.text,
            })
            .collect();
        if tabs.is_empty() {
            tabs.push(Self::new_tab(profile, 1));
        }
        tabs
    }

    fn new_tab(profile: &Profile, index: usize) -> QueryTab {
        QueryTab {
            id: format!("query-tab-{}-{index}", profile.id),
            title: format!("查询 {index}"),
            text: String::new(),
        }
    }

    fn stash(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_string();
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.text = text;
        }
    }

    fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.active || index >= self.tabs.len() {
            return;
        }
        self.save_active(cx);
        self.active = index;
        let text = self.tabs[index].text.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        cx.notify();
    }

    fn add_tab(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            return;
        };
        self.save_active(cx);
        let tab = Self::new_tab(&profile, self.tabs.len() + 1);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.editor
            .update(cx, |editor, cx| editor.set_text(String::new(), cx));
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.save_active(cx);
        if self.tabs.len() <= 1 {
            if let Some(tab) = self.tabs.get_mut(0) {
                tab.text.clear();
            }
            self.editor
                .update(cx, |editor, cx| editor.set_text(String::new(), cx));
            cx.notify();
            return;
        }
        let tab = self.tabs.remove(index);
        let _ = self.drafts.delete(&tab.id);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        let text = self.tabs[self.active].text.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, cx));
        cx.notify();
    }

    fn save_active(&mut self, cx: &mut Context<Self>) {
        self.stash(cx);
        let Some(profile) = self.profile.clone() else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let draft = Draft {
            id: tab.id.clone(),
            profile_id: profile.id,
            title: tab.title.clone(),
            text: tab.text.clone(),
            ..Default::default()
        };
        let _ = self.drafts.save(&draft);
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        let Some(profile) = self.profile.clone() else {
            self.error = Some("请先在左侧选择一个连接".into());
            cx.notify();
            return;
        };
        self.save_active(cx);
        let sql = self.editor.read(cx).text().to_string();
        if sql.trim().is_empty() {
            self.error = Some("请输入 SQL".into());
            cx.notify();
            return;
        }
        let credentials = self
            .profiles
            .credentials(&profile)
            .ok()
            .flatten()
            .unwrap_or_else(Credentials::default);
        let queries = self.queries.clone();
        self.running = true;
        self.error = None;
        self.status = "执行中…".into();
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { queries.execute(&profile, &credentials, &sql, DEFAULT_LIMIT) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.running = false;
                match outcome {
                    Ok(result) => {
                        this.status = format!(
                            "{} 行 · {} 列{} · {:.1} ms",
                            result.rows.len(),
                            result.columns.len(),
                            if result.truncated {
                                " · 已截断"
                            } else {
                                ""
                            },
                            result.duration_ns as f64 / 1_000_000.0
                        )
                        .into();
                        this.result = Some(result);
                    }
                    Err(err) => {
                        this.status = "执行失败".into();
                        this.error = Some(err.to_string().into());
                    }
                }
                this.editor
                    .update(cx, |editor, cx| editor.set_enabled(true, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
        self.editor
            .update(cx, |editor, cx| editor.set_enabled(false, cx));
    }

    fn tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut strip = div()
            .flex()
            .items_center()
            .h(px(34.0))
            .px_2()
            .gap_1()
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border);
        for (index, tab) in self.tabs.iter().enumerate() {
            let active = index == self.active;
            let title = SharedString::from(tab.title.clone());
            strip = strip.child(
                div()
                    .id(("tab", index))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(26.0))
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if active { theme.surface } else { theme.canvas })
                    .border_1()
                    .border_color(if active { theme.border } else { theme.canvas })
                    .on_click(
                        cx.listener(move |this, _event, _window, cx| this.switch_tab(index, cx)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if active { theme.text } else { theme.muted })
                            .child(title),
                    )
                    .child(
                        div()
                            .id(("close-tab", index))
                            .px_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.accent_soft))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.close_tab(index, cx)
                            }))
                            .child(icons::icon("shell/x.svg", 11.0, theme.muted)),
                    ),
            );
        }
        strip.child(
            div()
                .id("new-tab")
                .px_2()
                .h(px(26.0))
                .flex()
                .items_center()
                .rounded_md()
                .cursor_pointer()
                .hover(move |style| style.bg(theme.accent_soft))
                .on_click(cx.listener(|this, _event, _window, cx| this.add_tab(cx)))
                .child(icons::icon("shell/plus.svg", 13.0, theme.muted)),
        )
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let profile_label = self
            .profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| "未选择连接".to_string());
        let profile_kind = self
            .profile
            .as_ref()
            .map(|profile| profile.sql_dialect().to_string())
            .unwrap_or_default();
        let database = self
            .profile
            .as_ref()
            .map(|profile| {
                if profile.config.database.is_empty() {
                    "数据库 / Namespace".to_string()
                } else {
                    profile.config.database.clone()
                }
            })
            .unwrap_or_else(|| "数据库 / Namespace".to_string());

        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(42.0))
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("new-query")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| this.add_tab(cx)))
                    .child(icons::icon("superlink/new-query.svg", 14.0, theme.muted))
                    .child(div().text_xs().text_color(theme.muted).child("新建查询")),
            )
            .child(
                div()
                    .id("save-query")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.accent_soft))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.save_active(cx);
                        this.status = "已保存到草稿".into();
                        cx.notify();
                    }))
                    .child(icons::icon("shell/save.svg", 14.0, theme.muted))
                    .child(div().text_xs().text_color(theme.muted).child("保存")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(icons::icon(
                        icons::datasource_icon(profile_kind.as_str()),
                        14.0,
                        theme.muted,
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text)
                            .child(SharedString::from(profile_label)),
                    ),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted)
                    .child(SharedString::from(database)),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted)
                    .child(format!("{DEFAULT_LIMIT}")),
            )
            .child(
                div()
                    .id("run-query")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if self.running {
                        theme.border
                    } else {
                        theme.accent
                    })
                    .on_click(cx.listener(|this, _event, _window, cx| this.run(cx)))
                    .child(icons::icon("superlink/send.svg", 13.0, theme.surface))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.surface)
                            .child(if self.running {
                                "执行中"
                            } else {
                                "运行 ⌘R"
                            }),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted)
                    .child(self.status.clone()),
            )
    }

    fn editor_panel(&self) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .px_3()
            .py_2()
            .gap_1()
            .h(px(220.0))
            .border_b_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .child(self.editor.clone())
    }

    fn result_panel(&self) -> impl IntoElement {
        let theme = self.theme;
        let panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .h(px(32.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().text_xs().text_color(theme.muted).child("日志"))
                    .child(div().text_xs().text_color(theme.muted).child("参数"))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.accent)
                            .border_b_2()
                            .border_color(theme.accent)
                            .child("结果"),
                    ),
            );

        if let Some(error) = self.error.clone() {
            return panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(gpui::rgb(0xb42318))
                    .child(error),
            );
        }
        let Some(result) = self.result.as_ref() else {
            return panel.child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(theme.muted)
                    .child("执行查询后显示结果"),
            );
        };

        let header = div()
            .flex()
            .items_center()
            .h(px(28.0))
            .bg(theme.canvas)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(px(ROW_NUMBER_WIDTH))
                    .px_2()
                    .text_xs()
                    .text_color(theme.muted)
                    .child("#"),
            )
            .children(result.columns.iter().map(|column| {
                div()
                    .w(px(COLUMN_WIDTH))
                    .px_2()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.muted)
                    .child(SharedString::from(column.name.clone()))
            }));

        let rows = result
            .rows
            .iter()
            .take(DISPLAY_ROWS)
            .enumerate()
            .map(|(index, row)| {
                div()
                    .flex()
                    .items_center()
                    .h(px(24.0))
                    .bg(if index % 2 == 0 {
                        theme.surface
                    } else {
                        theme.canvas
                    })
                    .child(
                        div()
                            .w(px(ROW_NUMBER_WIDTH))
                            .px_2()
                            .text_xs()
                            .text_color(theme.muted)
                            .child(format!("{}", index + 1)),
                    )
                    .children(row.iter().map(|cell| {
                        div()
                            .w(px(COLUMN_WIDTH))
                            .px_2()
                            .text_xs()
                            .text_color(theme.text)
                            .child(cell_text(cell))
                    }))
            });

        panel.child(header).child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .children(rows),
        )
    }
}

fn cell_text(value: &Value) -> SharedString {
    match value {
        Value::Null => "NULL".into(),
        Value::String(text) => text.clone().into(),
        Value::Bool(flag) => flag.to_string().into(),
        Value::Number(number) => number.to_string().into(),
        other => other.to_string().into(),
    }
}

impl Render for QueryPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.surface)
            .child(self.tab_strip(cx))
            .child(self.toolbar(cx))
            .child(self.editor_panel())
            .child(self.result_panel())
    }
}
