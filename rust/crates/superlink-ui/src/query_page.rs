//! SQL workspace: statement input, execution and a bounded result table.

use std::sync::Arc;

use gpui::{Context, Entity, FontWeight, Render, SharedString, Window, div, prelude::*, px};
use serde_json::Value;
use superlink_app::{Credentials, DraftService, ProfileService, QueryService};
use superlink_domain::{Draft, Profile, ResultSet};

use crate::editor::SqlEditor;
use crate::theme::Theme;

const DISPLAY_ROWS: usize = 200;

pub struct QueryPage {
    profiles: Arc<ProfileService>,
    queries: Arc<QueryService>,
    drafts: Arc<DraftService>,
    theme: Theme,
    editor: Entity<SqlEditor>,
    profile: Option<Profile>,
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
        if let Some(draft) = self.restore_draft(&profile.id) {
            self.editor
                .update(cx, |editor, cx| editor.set_text(draft.text, cx));
        }
        self.profile = Some(profile);
        self.result = None;
        self.error = None;
        self.status = if supported {
            "驱动已就绪，可以执行".into()
        } else {
            "该数据源尚未提供 Rust 驱动 agent".into()
        };
        cx.notify();
    }

    fn draft_id(profile_id: &str) -> String {
        format!("query-tab-{profile_id}")
    }

    fn restore_draft(&self, profile_id: &str) -> Option<Draft> {
        self.drafts
            .list()
            .ok()?
            .into_iter()
            .find(|draft| draft.id == Self::draft_id(profile_id))
    }

    /// Autosave the editor buffer before execution so a crash keeps the text.
    fn save_draft(&self, profile: &Profile, text: &str) {
        let draft = Draft {
            id: Self::draft_id(&profile.id),
            profile_id: profile.id.clone(),
            title: format!("{} · 查询", profile.name),
            text: text.to_string(),
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
        self.save_draft(&profile, &sql);
        let queries = self.queries.clone();
        self.running = true;
        self.error = None;
        self.status = "执行中…".into();
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { queries.execute(&profile, &credentials, &sql, 500) });
        cx.spawn(async move |this, cx| {
            let outcome = task.await;
            this.update(cx, |this, cx| {
                this.running = false;
                match outcome {
                    Ok(result) => {
                        this.status = format!(
                            "{} 行 · {} 列{}",
                            result.rows.len(),
                            result.columns.len(),
                            if result.truncated {
                                " · 已截断"
                            } else {
                                ""
                            }
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

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let title = self
            .profile
            .as_ref()
            .map(|profile| format!("{} · {}", profile.name, profile.sql_dialect()))
            .unwrap_or_else(|| "未选择连接".to_string());
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(40.0))
            .border_b_1()
            .border_color(theme.border)
            .child(div().text_sm().text_color(theme.text).child(title))
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .id("run-query")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(if self.running {
                        theme.border
                    } else {
                        theme.accent
                    })
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.surface)
                    .hover(move |style| style.opacity(0.9))
                    .on_click(cx.listener(|this, _event, _window, cx| this.run(cx)))
                    .child(if self.running {
                        "执行中"
                    } else {
                        "运行 ⌘R"
                    }),
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
            .border_b_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .child(self.editor.clone())
    }

    fn result_panel(&self) -> impl IntoElement {
        let theme = self.theme;
        let mut panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.surface);
        if let Some(error) = self.error.clone() {
            panel = panel.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(gpui::rgb(0xb42318))
                    .child(error),
            );
            return panel;
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
                    .child("结果会显示在这里"),
            );
        };

        let header = div()
            .flex()
            .items_center()
            .h(px(28.0))
            .border_b_1()
            .border_color(theme.border)
            .children(result.columns.iter().map(|column| {
                div()
                    .w(px(160.0))
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
                    .children(row.iter().map(|cell| {
                        div()
                            .w(px(160.0))
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
            .child(self.header(cx))
            .child(self.editor_panel())
            .child(self.result_panel())
    }
}
