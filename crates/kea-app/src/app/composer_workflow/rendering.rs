use super::*;
use gpui_component::ActiveTheme as _;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    Disableable as _, Selectable as _, Sizable as _,
};

impl KeaView {
    /// Draft tab label: the automatic title, or a numbered name while empty.
    pub(in crate::app) fn draft_tab_label(&self, index: usize, cx: &App) -> String {
        self.composer_workflow
            .sections
            .get(index)
            .and_then(|editor| workflow::draft_title(&editor.read(cx).value()))
            .unwrap_or_else(|| format!("Draft {}", index + 1))
    }

    pub(in crate::app) fn render_composer_sections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = self.composer_workflow.sections.clone();
        let active = self.composer_workflow.active;
        let recommendations = if self.settings.composer_suggestions
            && self.editor.focus_handle(cx).is_focused(window)
            && self.composer_workflow.menu.is_none()
        {
            self.composer_recommendations(cx)
        } else {
            Vec::new()
        };
        let recommendation = recommendations.first();
        let split_plan = recommendation.and_then(|rec| {
            matches!(rec, Recommendation::SplitSeparator(_))
                .then(|| {
                    EditPlan::for_recommendation(
                        &self.editor.read(cx).value(),
                        self.editor.read(cx).cursor(),
                        rec,
                        None,
                    )
                })
                .flatten()
        });
        // The parent is a horizontal, items-start row. Flex growth fills its
        // width, not its height: without h_full the nested input collapses to
        // its 72 px minimum and scrolls several lines before the panel is full.
        let mut column = div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0();
        if sections.len() > 1 {
            column = column.child(self.render_draft_tabs(&sections, active, cx));
        }
        if self.composer_workflow.move_open && !self.draft_move_targets.is_empty() {
            column = column.child(self.render_draft_move_targets(cx));
        }
        // Only the active draft is shown, at full composer size. Inactive drafts
        // keep their entities, text, selection and undo history.
        column = column.child(
            Input::new(&sections[active])
                .flex_1()
                .h_full()
                .min_h(px(72.))
                .appearance(false)
                .bordered(false),
        );
        if let Some(rec) = recommendation {
            let target = self.editor.clone();
            let label = if split_plan.is_some() {
                "Split here".into()
            } else {
                format!(
                    "Suggestion: {} · {}",
                    rec.label(),
                    self.keymap.label(Action::ComposerActions)
                )
            };
            column = column.child(
                button("composer-recommendation", label)
                    .flex_shrink_0()
                    .text_xs()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(plan) = &split_plan {
                            this.accept_split_recommendation(&target, plan.clone(), window, cx);
                        } else {
                            this.open_composer_actions(window, cx);
                        }
                    })),
            );
        }
        column.into_any_element()
    }

    fn render_draft_tabs(
        &self,
        sections: &[Entity<InputState>],
        active: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut strip = div()
            .id("composer-sections")
            .flex()
            .flex_row()
            .items_center()
            .flex_shrink_0()
            .min_w_0()
            .gap_1()
            .pb_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .track_scroll(&self.composer_workflow.scroll)
            .overflow_x_scroll();
        for (index, editor) in sections.iter().enumerate() {
            let label = self.draft_tab_label(index, cx);
            let full = editor
                .read(cx)
                .value()
                .lines()
                .next()
                .unwrap_or("")
                .to_owned();
            let is_active = index == active;
            let mut tab = div()
                .id(("composer-section", editor.entity_id()))
                .flex()
                .flex_row()
                .items_center()
                .flex_shrink_0()
                .border_b_2()
                .border_color(if is_active {
                    cx.theme().primary
                } else {
                    cx.theme().transparent
                })
                .child(
                    Button::new(("focus-composer-section", editor.entity_id()))
                        .label(label)
                        .tooltip(if full.is_empty() {
                            "Empty draft. Submit sends only the active draft tab.".to_owned()
                        } else {
                            format!("{full}\nSubmit sends only the active draft tab.")
                        })
                        .ghost()
                        .small()
                        .selected(is_active)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_composer_section(index, cx);
                            this.composer_workflow.scroll.scroll_to_item(index);
                            this.focus_editor(window, cx);
                        })),
                );
            tab = tab.child(
                Button::new(("close-composer-section", editor.entity_id()))
                    .label("×")
                    .tooltip(
                        "Close this draft tab without sending. Undo draft tab change reopens it.",
                    )
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.close_composer_section(index, window, cx);
                    })),
            );
            strip = strip.child(tab);
        }
        // Actions for the active tab sit at a fixed place on the right, so the
        // tab titles do not shift when the active tab changes.
        let editor = &sections[active];
        strip
            .child(
                Button::new("add-composer-section")
                    .label("+")
                    .tooltip("Add an empty draft tab")
                    .ghost()
                    .xsmall()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.add_composer_section(window, cx);
                    })),
            )
            .child(div().flex_1().min_w(px(8.)))
            .child(
                Button::new(("merge-composer-section", editor.entity_id()))
                    .label("Merge ←")
                    .tooltip("Join the active draft to the previous draft tab with a newline")
                    .ghost()
                    .xsmall()
                    .disabled(active == 0)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_composer_section(active, cx);
                        this.focus_editor(window, cx);
                        this.merge_composer(window, cx);
                    })),
            )
            .into_any_element()
    }

    /// Shown in the composer header whenever another terminal tab exists.
    pub(in crate::app) fn render_draft_move_button(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.draft_move_targets.is_empty() {
            return div().into_any_element();
        }
        button("move-composer-section", "Move to ⇄")
            .when(self.composer_workflow.move_open, |button| {
                button.bg(cx.theme().accent)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.composer_workflow.move_open = !this.composer_workflow.move_open;
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_draft_move_targets(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .id("composer-move-targets")
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .py_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("Move this draft to:");
        for (target, label) in self.draft_move_targets.clone() {
            row = row.child(
                Button::new(("move-composer-target", target.0))
                    .label(label)
                    .tooltip("Moves the text only; nothing is sent")
                    .outline()
                    .xsmall()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_draft_move(target, cx);
                    })),
            );
        }
        row.into_any_element()
    }

    pub(in crate::app) fn render_composer_actions(
        &self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(menu) = &self.composer_workflow.menu else {
            return div().into_any_element();
        };
        let mut panel = div()
            .id("composer-action-menu")
            .track_focus(&menu.focus)
            .key_context("KeaChrome")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .max_h(px(220.))
            .overflow_y_scroll()
            .gap_1()
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().text_sm().child(
                "Composer actions · Enter selects · Escape closes · No automatic submission",
            ));
        if let Some(plan) = &menu.preview {
            panel = panel.child(div().text_sm().child(format!(
                "Preview: {} draft(s). Applying changes input only; it does not send it.",
                plan.sections.len()
            )));
            for (index, editor) in menu.preview_editors.iter().enumerate() {
                panel = panel
                    .child(div().text_xs().child(format!("Draft {}", index + 1)))
                    .child(
                        Input::new(editor)
                            .disabled(true)
                            .h(px(80.))
                            .appearance(false)
                            .bordered(false),
                    );
            }
            panel = panel.child(
                button("apply-composer-action", "Apply transformation · Enter").on_click(
                    cx.listener(|this, _, window, cx| this.accept_composer_preview(window, cx)),
                ),
            );
        } else if let Some(rec) = &menu.placeholder {
            panel = panel
                .child(div().text_sm().child(rec.label()))
                .child(div().text_xs().child(
                    "Literal text replacement; no shell quoting or evaluation is performed.",
                ))
                .child(Input::new(&menu.value))
                .child(
                    button("preview-placeholder", "Preview replacement · Enter").on_click(
                        cx.listener(|this, _, window, cx| this.preview_placeholder(window, cx)),
                    ),
                );
        } else {
            for (index, item) in menu.items.iter().enumerate() {
                panel = panel.child(
                    div()
                        .id(("composer-action", index))
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .when(index == menu.selected, |row| row.bg(cx.theme().accent))
                        .child(item.label())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.preview_composer_action(index, window, cx)
                        })),
                );
            }
            if menu.items.is_empty() {
                panel =
                    panel.child("No matching composer action. The trigger remains literal text.");
            }
        }
        panel
            .child(
                button("close-composer-actions", "Cancel · Escape").on_click(
                    cx.listener(|this, _, window, cx| this.dismiss_composer_actions(window, cx)),
                ),
            )
            .into_any_element()
    }
}
