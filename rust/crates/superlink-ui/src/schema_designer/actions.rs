use super::*;
impl SchemaDesigner {
    pub(super) fn add_field(&mut self, after: bool, cx: &mut Context<Self>) {
        if !self.editable() || self.rows.len() >= 4096 {
            return;
        }
        let draft = self.draft(cx);
        let mut n = 1;
        while draft
            .fields
            .iter()
            .any(|f| f.column.name == format!("field_{n}"))
        {
            n += 1;
        }
        let row = Self::row(
            DesignField {
                original: None,
                column: ColumnDefinition {
                    name: format!("field_{n}"),
                    data_type: if self.profile.sql_dialect() == "sqlite" {
                        "TEXT"
                    } else {
                        "varchar(255)"
                    }
                    .into(),
                    nullable: "YES".into(),
                    ..Default::default()
                },
            },
            cx,
        );
        let position = if after {
            self.rows
                .iter()
                .rposition(|r| self.selected.contains(&r.id))
                .map(|i| i + 1)
                .unwrap_or(self.rows.len())
        } else {
            self.rows.len()
        };
        self.selected.clear();
        self.selected.insert(row.id.clone());
        self.rows.insert(position, row);
        self.field_page = position / 100;
        self.changed(cx);
    }
    pub(super) fn delete_fields(&mut self, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        self.rows.retain(|r| !self.selected.contains(&r.id));
        self.field_page = self.field_page.min(self.rows.len().saturating_sub(1) / 100);
        self.selected.clear();
        self.changed(cx);
    }
    pub(super) fn move_fields(&mut self, down: bool, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let n = self.rows.len();
        if down {
            for i in (0..n.saturating_sub(1)).rev() {
                if self.selected.contains(&self.rows[i].id)
                    && !self.selected.contains(&self.rows[i + 1].id)
                {
                    self.rows.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..n {
                if self.selected.contains(&self.rows[i].id)
                    && !self.selected.contains(&self.rows[i - 1].id)
                {
                    self.rows.swap(i, i - 1);
                }
            }
        }
        self.changed(cx);
    }
    pub(super) fn drop_field(&mut self, target: &str, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        if let Some(source) = self.dragging.take() {
            let from = self.rows.iter().position(|r| r.id == source);
            let to = self.rows.iter().position(|r| r.id == target);
            if let (Some(from), Some(to)) = (from, to)
                && from != to
            {
                let row = self.rows.remove(from);
                self.rows.insert(to, row);
                self.changed(cx);
            }
        }
    }
    pub(super) fn copy_fields(&mut self, cx: &mut Context<Self>) {
        let fields = self.selected_fields(cx);
        if fields.is_empty() {
            self.note = Some("请先选择字段".into());
            cx.notify();
            return;
        }
        let value =
            serde_json::json!({"format":"SuperLink.schema-fields","version":1,"fields":fields});
        match serde_json::to_string_pretty(&value) {
            Ok(text) if text.len() <= 1 << 20 => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                self.note = Some("已复制所选字段定义".into());
            }
            _ => self.note = Some("字段定义超过 1 MiB，无法复制".into()),
        }
        cx.notify();
    }
    pub(super) fn selected_fields(&self, cx: &Context<Self>) -> Vec<ColumnDefinition> {
        self.rows
            .iter()
            .zip(self.draft(cx).fields)
            .filter(|(row, _)| self.selected.contains(&row.id))
            .map(|(_, field)| field.column)
            .collect()
    }
    pub(super) fn paste_fields(&mut self, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let outcome = decode_fields(&text);
        match outcome {
            Ok(fields) if fields.len() + self.rows.len() <= 4096 => {
                let mut names = self
                    .draft(cx)
                    .fields
                    .into_iter()
                    .map(|f| f.column.name.to_lowercase())
                    .collect::<BTreeSet<_>>();
                let position = self
                    .rows
                    .iter()
                    .rposition(|r| self.selected.contains(&r.id))
                    .map(|i| i + 1)
                    .unwrap_or(self.rows.len());
                self.selected.clear();
                for (offset, mut column) in fields.into_iter().enumerate() {
                    let base = column.name.clone();
                    let mut suffix = 1;
                    while !names.insert(column.name.to_lowercase()) {
                        column.name = format!("{base}_copy{suffix}");
                        suffix += 1;
                    }
                    let row = Self::row(
                        DesignField {
                            original: None,
                            column,
                        },
                        cx,
                    );
                    self.selected.insert(row.id.clone());
                    self.rows.insert(position + offset, row);
                }
                self.changed(cx);
            }
            Ok(_) => {
                self.note = Some("字段总数超过 4096".into());
                cx.notify();
            }
            Err(error) => {
                self.note = Some(error.into());
                cx.notify();
            }
        }
    }
    pub(super) fn toggle_flag(&mut self, id: &str, flag: usize, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        let mysql = superlink_sql::structure_dialect(self.profile.sql_dialect()) == "mysql";
        if let Some(row) = self.rows.iter_mut().find(|r| r.id == id) {
            if !row.base.writable() {
                return;
            }
            match flag {
                0 if mysql => row.unsigned = !row.unsigned,
                1 if mysql => {
                    row.primary = !row.primary;
                    if row.primary {
                        row.not_null = true;
                    }
                }
                2 if mysql => {
                    row.auto = !row.auto;
                    if row.auto {
                        row.primary = true;
                        row.not_null = true;
                        row.default.update(cx, |e, cx| e.set_text("", cx));
                    }
                }
                3 if !row.primary => row.not_null = !row.not_null,
                _ => return,
            };
            self.changed(cx);
        }
    }
    pub(super) fn choose_type(&mut self, id: &str, value: &str, cx: &mut Context<Self>) {
        if !self.editable() {
            return;
        }
        if let Some(row) = self.rows.iter().find(|r| r.id == id && r.base.writable()) {
            row.data_type
                .update(cx, |e, cx| e.set_text(value.to_owned(), cx));
            self.type_picker = None;
            self.changed(cx);
        }
    }
}
fn decode_fields(text: &str) -> Result<Vec<ColumnDefinition>, String> {
    if text.len() > 1 << 20 {
        return Err("剪贴板字段定义超过 1 MiB".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "剪贴板不是有效的字段定义")?;
    if value.get("format").and_then(|v| v.as_str()) != Some("SuperLink.schema-fields")
        || value.get("version").and_then(|v| v.as_u64()) != Some(1)
    {
        return Err("剪贴板格式不受支持，请先复制字段".into());
    }
    let fields: Vec<ColumnDefinition> =
        serde_json::from_value(value.get("fields").cloned().unwrap_or_default())
            .map_err(|_| "字段定义格式无效")?;
    if fields.is_empty() || fields.len() > 4096 {
        return Err("字段定义需要 1–4096 项".into());
    }
    Ok(fields)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_rejects_unversioned_oversized_or_arbitrary_sql() {
        assert!(decode_fields("ALTER TABLE users DROP id").is_err());
        assert!(decode_fields(r#"{"fields":[]}"#).is_err());
        assert!(decode_fields(&"x".repeat((1 << 20) + 1)).is_err());
        let value = serde_json::json!({"format":"SuperLink.schema-fields","version":1,"fields":[ColumnDefinition{name:"字段".into(),data_type:"TEXT".into(),nullable:"YES".into(),..Default::default()}]});
        assert_eq!(decode_fields(&value.to_string()).unwrap()[0].name, "字段");
    }
}
