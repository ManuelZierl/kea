use super::*;
use gpui_component::ActiveTheme as _;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    Disableable as _, Sizable as _,
};

pub(super) fn section_editor_height(text: &str, font_size: f32) -> Pixels {
    // Include a trailing empty line and scale with the actual editor typography.
    let rows = (text.bytes().filter(|byte| *byte == b'\n').count() + 1).clamp(3, 10);
    px(rows as f32 * font_size * COMPONENT_LINE_HEIGHT_EM + 12.)
}

impl KeaView {
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
        if sections.len() == 1 {
            column = column.child(
                Input::new(&sections[0])
                    .flex_1()
                    .h_full()
                    .min_h(px(72.))
                    .appearance(false)
                    .bordered(false),
            );
        } else {
            let mut list = div()
                .id("composer-sections")
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .gap_2()
                .track_scroll(&self.composer_workflow.scroll)
                .overflow_y_scroll();
            for (index, editor) in sections.iter().enumerate() {
                let height = section_editor_height(
                    &editor.read(cx).value(),
                    f32::from(cx.theme().mono_font_size),
                );
                let label = if index == active {
                    format!("Draft {} · active", index + 1)
                } else {
                    format!("Draft {}", index + 1)
                };
                list = list.child(
                    div()
                        .id(("composer-section", editor.entity_id()))
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .min_w_0()
                        .border_1()
                        .rounded_sm()
                        .border_color(if index == active {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .min_w_0()
                                .text_xs()
                                .px_1()
                                .py_1()
                                .bg(cx.theme().muted)
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    Button::new(("focus-composer-section", editor.entity_id()))
                                        .label(label)
                                        .tooltip("Focus this draft; Submit sends only this draft")
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select_composer_section(index, cx);
                                            this.composer_workflow
                                                .scroll
                                                .scroll_to_top_of_item(index);
                                            this.focus_editor(window, cx);
                                        })),
                                )
                                .child(div().flex_1().min_w_0())
                                .child(
                                    Button::new(("merge-composer-section", editor.entity_id()))
                                        .label("Merge ↑")
                                        .tooltip(
                                            "Join this draft to the previous draft with a newline",
                                        )
                                        .ghost()
                                        .small()
                                        .disabled(index == 0)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.select_composer_section(index, cx);
                                            this.focus_editor(window, cx);
                                            this.merge_composer(window, cx);
                                        })),
                                ),
                        )
                        .child(
                            Input::new(editor)
                                .h(height)
                                .min_w_0()
                                .appearance(false)
                                .bordered(false),
                        ),
                );
            }
            column = column.child(list);
        }
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
