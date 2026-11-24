use super::*;

impl SqlTextEditor {
    pub(super) fn gutter(&self) -> f32 {
        if self.plain { 0.0 } else { GUTTER }
    }
    pub fn set_rich_markdown(&mut self, rich: bool, cx: &mut Context<Self>) {
        self.rich_markdown = rich;
        cx.notify();
    }
    pub(super) fn rebuild(&mut self) {
        self.source_revision = self.source_revision.wrapping_add(1);
        self.cached_starts = std::sync::Arc::new(line_starts(&self.content));
        self.cached_tokens = std::sync::Arc::new(if self.plain {
            vec![superlink_sql::Token {
                text: self.content.to_string(),
                kind: superlink_sql::TokenKind::Plain,
            }]
        } else {
            tokenize(&self.content)
        });
    }

    pub fn set_plain(&mut self, plain: bool, cx: &mut Context<Self>) {
        self.plain = plain;
        self.completion = None;
        self.rebuild();
        cx.notify();
    }

    pub(super) fn record_edit(&mut self) {
        self.redo.clear();
        self.undo
            .push((self.content.clone(), self.selected_range.clone()));
        while self.undo.len() > 100 || self.undo.iter().map(|s| s.0.len()).sum::<usize>() > 8 << 20
        {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if let Some((text, range)) = self.undo.pop() {
            self.redo
                .push((self.content.clone(), self.selected_range.clone()));
            self.content = text;
            self.selected_range = range;
            self.marked_range = None;
            self.rebuild();
            self.ensure_caret_visible();
            self.emit_change(cx);
            cx.notify();
        }
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if let Some((text, range)) = self.redo.pop() {
            self.undo
                .push((self.content.clone(), self.selected_range.clone()));
            self.content = text;
            self.selected_range = range;
            self.marked_range = None;
            self.rebuild();
            self.ensure_caret_visible();
            self.emit_change(cx);
            cx.notify();
        }
    }

    pub(crate) fn formatting_state(&self) -> formatting::FormattingState {
        formatting::state(&self.content, self.selected_range.clone(), self.enabled)
    }

    pub fn format(&mut self, prefix: &str, suffix: &str, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        if prefix.contains("```") {
            let edit = formatting::toggle_fence(&self.content, self.selected_range.clone());
            self.record_edit();
            self.content = format!(
                "{}{}{}",
                &self.content[..edit.range.start],
                edit.replacement,
                &self.content[edit.range.end..]
            )
            .into();
            self.selected_range = edit.selected;
            self.rebuild();
            self.ensure_caret_visible();
            self.emit_change(cx);
            cx.notify();
            return;
        }
        let mut range = self.selected_range.clone();
        if suffix.is_empty() {
            range.start = self.content[..range.start]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            range.end = self.content[range.end..]
                .find('\n')
                .map(|i| range.end + i)
                .unwrap_or(self.content.len());
        }
        let selected = self.content[range.clone()].to_string();
        let (replacement, selection) = if suffix.is_empty() {
            let replacement = formatting::toggle_block(&selected, prefix);
            let end = range.start + replacement.len();
            (replacement, range.start..end)
        } else if range.start >= prefix.len()
            && self.content[..range.start].ends_with(prefix)
            && self.content[range.end..].starts_with(suffix)
        {
            range.start -= prefix.len();
            range.end += suffix.len();
            let end = range.start + selected.len();
            (selected, range.start..end)
        } else {
            let start = range.start + prefix.len();
            let end = start + selected.len();
            (format!("{prefix}{selected}{suffix}"), start..end)
        };
        self.record_edit();
        self.content = format!(
            "{}{}{}",
            &self.content[..range.start],
            replacement,
            &self.content[range.end..]
        )
        .into();
        self.selected_range = selection;
        self.rebuild();
        self.ensure_caret_visible();
        self.emit_change(cx);
        cx.notify();
    }
}

pub(super) fn utf16_byte(text: &str, offset: usize) -> usize {
    let mut count = 0;
    for (byte, ch) in text.char_indices() {
        if count >= offset {
            return byte;
        }
        count += ch.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marked_selection_offsets_are_relative_to_composition() {
        assert_eq!(utf16_byte("中😀文", 1), 3);
        assert_eq!(utf16_byte("中😀文", 3), 7);
        assert_eq!(utf16_byte("中😀文", 4), 10);
    }
}
