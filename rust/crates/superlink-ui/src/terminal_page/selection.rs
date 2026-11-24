use super::*;
impl TerminalPage {
    fn selection_position(&self, position: gpui::Point<Pixels>) -> Option<(usize, usize)> {
        let bounds = self.screen_bounds?;
        let height = bounds.size.height / self.screen.rows() as f32;
        let row = ((position.y - bounds.top()) / height).floor().max(0.0) as usize;
        let row = row.min(self.screen.rows().saturating_sub(1));
        let line = self.screen_lines.get(row)?;
        let column = line
            .index_for_x(position.x - bounds.left())
            .unwrap_or(line.len());
        Some((row, column))
    }
    pub(super) fn start_selection(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if let Some(position) = self.selection_position(position) {
            self.selection = Some((position, position));
            self.selecting_text = true;
            cx.notify();
        }
    }
    pub(super) fn extend_selection(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if let (Some(position), Some((anchor, _))) =
            (self.selection_position(position), self.selection)
        {
            self.selection = Some((anchor, position));
            cx.notify();
        }
    }
    pub(super) fn copy_selection(&self, cx: &mut Context<Self>) {
        let Some((a, b)) = self.selection else {
            return;
        };
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        if from == to {
            return;
        }
        let mut selected = Vec::new();
        for row in from.0..=to.0 {
            let (text, _) = self.screen.visible_row(row, self.scroll_offset);
            let start = if row == from.0 {
                from.1.min(text.len())
            } else {
                0
            };
            let end = if row == to.0 {
                to.1.min(text.len())
            } else {
                text.len()
            };
            if text.is_char_boundary(start) && text.is_char_boundary(end) && start <= end {
                selected.push(text[start..end].trim_end().to_string());
            }
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(selected.join("\n")));
    }
    pub(super) fn selection_quads(
        &self,
        lines: &[ShapedLine],
        bounds: Bounds<Pixels>,
        height: Pixels,
    ) -> Vec<PaintQuad> {
        let Some((a, b)) = self.selection else {
            return Vec::new();
        };
        let (from, to) = if a <= b { (a, b) } else { (b, a) };
        lines
            .iter()
            .enumerate()
            .filter_map(|(row, line)| {
                if row < from.0 || row > to.0 {
                    return None;
                }
                let start = if row == from.0 {
                    from.1.min(line.len())
                } else {
                    0
                };
                let end = if row == to.0 {
                    to.1.min(line.len())
                } else {
                    line.len()
                };
                let left = line.x_for_index(start);
                let right = line.x_for_index(end);
                if right <= left {
                    return None;
                }
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + left, bounds.top() + height * row as f32),
                        size(right - left, height),
                    ),
                    gpui::rgba(0x5b8ec966),
                ))
            })
            .collect()
    }
}
