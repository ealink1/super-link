//! Editable field designer. Drafts live in memory until a reviewed save.
use crate::{editor::SqlEditor, theme::Theme};
use gpui::{
    AnyElement, Context, Entity, EventEmitter, Render, SharedString, Window, div, prelude::*, px,
};
use std::{collections::BTreeSet, sync::Arc};
use superlink_app::{ProfileService, QueryCancel, QueryService};
use superlink_domain::Profile;
use superlink_sql::{
    ColumnDefinition,
    design::{DesignDraft, DesignField, DesignSnapshot},
};
mod actions;
mod grid;
mod io;
mod view;

#[derive(Clone)]
pub enum DesignEvent {
    Changed,
    Saved(String),
    SelectSection(crate::workbench::TableSection),
}
pub struct FieldRow {
    id: String,
    original: Option<String>,
    base: ColumnDefinition,
    name: Entity<SqlEditor>,
    data_type: Entity<SqlEditor>,
    default: Entity<SqlEditor>,
    comment: Entity<SqlEditor>,
    unsigned: bool,
    primary: bool,
    auto: bool,
    not_null: bool,
}
struct Plan {
    target: String,
    before: Option<DesignSnapshot>,
    draft: DesignDraft,
    statements: Vec<String>,
    copy: bool,
}
pub struct SchemaDesigner {
    profile: Profile,
    database: String,
    table: String,
    queries: Arc<QueryService>,
    profiles: Arc<ProfileService>,
    theme: Theme,
    baseline: Option<DesignSnapshot>,
    rows: Vec<FieldRow>,
    selected: BTreeSet<String>,
    table_comment: Entity<SqlEditor>,
    target: Entity<SqlEditor>,
    comment_open: bool,
    copy_open: bool,
    copy_new: bool,
    type_picker: Option<String>,
    type_anchor: gpui::Point<gpui::Pixels>,
    field_page: usize,
    dragging: Option<String>,
    dirty: bool,
    busy: bool,
    stale: bool,
    discard_confirm: bool,
    plan: Option<Plan>,
    generation: u64,
    operation: Option<QueryCancel>,
    note: Option<SharedString>,
}
impl EventEmitter<DesignEvent> for SchemaDesigner {}
impl SchemaDesigner {
    pub fn new(
        profile: Profile,
        database: String,
        table: String,
        queries: Arc<QueryService>,
        profiles: Arc<ProfileService>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let table_comment = Self::input("", "表备注", cx);
        let target = Self::input("", "目标表名", cx);
        let weak = cx.weak_entity();
        target.update(cx, |e, cx| {
            e.set_on_change(
                move |_, app| {
                    let _ = weak.update(app, |this, cx| {
                        this.plan = None;
                        cx.notify();
                    });
                },
                cx,
            )
        });
        cx.on_release(|this, cx| {
            if let Some(cancel) = this.operation.take() {
                cx.background_executor()
                    .spawn(async move {
                        cancel.cancel();
                    })
                    .detach();
            }
        })
        .detach();
        let mut this = Self {
            profile,
            database,
            table,
            queries,
            profiles,
            theme,
            baseline: None,
            rows: Vec::new(),
            selected: BTreeSet::new(),
            table_comment,
            target,
            comment_open: false,
            copy_open: false,
            copy_new: false,
            type_picker: None,
            type_anchor: gpui::point(px(0.), px(0.)),
            field_page: 0,
            dragging: None,
            dirty: false,
            busy: false,
            stale: false,
            discard_confirm: false,
            plan: None,
            generation: 0,
            operation: None,
            note: None,
        };
        this.reload(cx);
        this
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }
    pub fn mark_stale(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        self.generation = self.generation.wrapping_add(1);
        self.busy = false;
        self.plan = None;
        self.note = Some("连接配置已变化，草稿已保留；请关闭后重新打开设计器".into());
        if let Some(cancel) = self.operation.take() {
            cx.background_executor()
                .spawn(async move {
                    cancel.cancel();
                })
                .detach();
        }
        self.enable_inputs(cx);
        cx.notify();
    }
    fn input(text: &str, placeholder: &str, cx: &mut Context<Self>) -> Entity<SqlEditor> {
        let weak = cx.weak_entity();
        let editor = cx.new(SqlEditor::new);
        editor.update(cx, |editor, cx| {
            editor.set_text(text.to_string(), cx);
            editor.set_placeholder(placeholder.to_string(), cx);
            editor.set_on_change(
                move |_, app| {
                    let _ = weak.update(app, |this, cx| {
                        this.changed(cx);
                    });
                },
                cx,
            );
        });
        editor
    }
    fn row(field: DesignField, cx: &mut Context<Self>) -> FieldRow {
        let column = field.column;
        let unsigned = column.data_type.to_lowercase().ends_with(" unsigned");
        let data_type = if unsigned {
            column.data_type[..column.data_type.len() - 9]
                .trim()
                .to_owned()
        } else {
            column.data_type.clone()
        };
        FieldRow {
            id: uuid::Uuid::new_v4().to_string(),
            original: field.original,
            name: Self::input(&column.name, "字段名", cx),
            data_type: Self::input(&data_type, "类型", cx),
            default: Self::input(column.default.as_deref().unwrap_or(""), "无默认值", cx),
            comment: Self::input(&column.comment, "注释", cx),
            unsigned,
            primary: column.key == "PRI",
            auto: column.extra.to_lowercase().contains("auto_increment"),
            not_null: column.nullable == "NO",
            base: column,
        }
    }
    fn draft(&self, cx: &Context<Self>) -> DesignDraft {
        DesignDraft {
            fields: self
                .rows
                .iter()
                .map(|row| {
                    let mut column = row.base.clone();
                    column.name = row.name.read(cx).text().trim().into();
                    column.data_type = row.data_type.read(cx).text().trim().into();
                    if row.unsigned && !column.data_type.to_lowercase().ends_with(" unsigned") {
                        column.data_type.push_str(" unsigned");
                    }
                    column.key = if row.primary {
                        "PRI".into()
                    } else if row.base.key == "PRI" {
                        String::new()
                    } else {
                        row.base.key.clone()
                    };
                    column.nullable = if row.not_null || row.primary {
                        "NO"
                    } else {
                        "YES"
                    }
                    .into();
                    let original_auto = row.base.extra.to_lowercase().contains("auto_increment");
                    if row.auto != original_auto {
                        column.extra = row
                            .base
                            .extra
                            .split_whitespace()
                            .filter(|word| !word.eq_ignore_ascii_case("auto_increment"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if row.auto {
                            column.extra.push_str(" auto_increment");
                            column.extra = column.extra.trim().into();
                        }
                    }
                    let default = row.default.read(cx).text().to_string();
                    column.has_default = !default.is_empty();
                    column.default = (!default.is_empty()).then_some(default);
                    column.comment = row.comment.read(cx).text().into();
                    if !is_text_type(&column.data_type) {
                        column.charset.clear();
                        column.collation.clear();
                    }
                    DesignField {
                        original: row.original.clone(),
                        column,
                    }
                })
                .collect(),
            comment: self.table_comment.read(cx).text().into(),
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.plan = None;
        self.discard_confirm = false;
        self.note = None;
        cx.emit(DesignEvent::Changed);
        cx.notify();
    }
    fn editable(&self) -> bool {
        !self.busy && !self.stale && self.baseline.is_some()
    }
    fn enable_inputs(&mut self, cx: &mut Context<Self>) {
        let enabled = self.editable();
        for row in &self.rows {
            for editor in [&row.name, &row.data_type, &row.default, &row.comment] {
                editor.update(cx, |e, cx| {
                    e.set_enabled(enabled && row.base.writable(), cx)
                });
            }
        }
        self.table_comment
            .update(cx, |e, cx| e.set_enabled(enabled, cx));
        self.target.update(cx, |e, cx| e.set_enabled(enabled, cx));
    }
}
fn is_text_type(value: &str) -> bool {
    let base = value.split(['(', ' ']).next().unwrap_or("").to_lowercase();
    matches!(
        base.as_str(),
        "char" | "varchar" | "tinytext" | "text" | "mediumtext" | "longtext" | "enum" | "set"
    )
}
impl Render for SchemaDesigner {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.panel(cx)
    }
}
