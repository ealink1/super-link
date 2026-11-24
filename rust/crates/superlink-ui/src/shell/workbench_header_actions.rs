use super::*;

impl SuperLinkApp {
    pub(super) fn workbench_header_actions(
        &self,
        index: usize,
        name: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let bench = self.workbench.as_ref().unwrap();
        let mark = bench.sort_mark(name);
        let filtered = bench.filters.iter().any(|f| f.column == name);
        let filter_name = name.to_owned();
        let sort_name = name.to_owned();
        div()
            .flex()
            .items_start()
            .gap(px(1.))
            .pt(px(6.))
            .w(px(23.))
            .h_full()
            .flex_shrink_0()
            .child(
                div()
                    .id(("wb-column-filter", index))
                    .w(px(12.))
                    .h(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.accent_soft))
                    .child(icons::icon(
                        "superlink/filter.svg",
                        9.,
                        if filtered {
                            theme.accent
                        } else {
                            theme.muted.opacity(0.7)
                        },
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(b) = this.workbench.as_mut() {
                            b.tools_open = true;
                            b.draft.column = filter_name.clone();
                        }
                        this.wb_column
                            .update(cx, |editor, cx| editor.set_text(filter_name.clone(), cx));
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id(("wb-column-sort", index))
                    .w(px(9.))
                    .h(px(14.))
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(move |s| s.bg(theme.accent_soft))
                    .child(
                        gpui::canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                for (down, active) in [(false, mark == " ↑"), (true, mark == " ↓")]
                                {
                                    let x = bounds.left() + px(2.);
                                    let y = bounds.top() + px(if down { 8. } else { 3. });
                                    let mut path = gpui::PathBuilder::fill();
                                    path.move_to(point(x, y + px(if down { 0. } else { 3. })));
                                    path.line_to(point(
                                        x + px(5.),
                                        y + px(if down { 0. } else { 3. }),
                                    ));
                                    path.line_to(point(
                                        x + px(2.5),
                                        y + px(if down { 3. } else { 0. }),
                                    ));
                                    path.close();
                                    if let Ok(path) = path.build() {
                                        window.paint_path(
                                            path,
                                            if active {
                                                theme.accent
                                            } else {
                                                theme.muted.opacity(0.55)
                                            },
                                        );
                                    }
                                }
                            },
                        )
                        .size_full(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.workbench_sort(sort_name.clone(), cx);
                    })),
            )
            .into_any_element()
    }
}
