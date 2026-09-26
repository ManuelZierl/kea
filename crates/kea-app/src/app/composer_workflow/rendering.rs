use super::*;
use gpui_component::ActiveTheme as _;

impl KeaView {
    pub(in crate::app) fn render_composer_sections(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let sections = self.composer_workflow.sections.clone();
        let active = self.composer_workflow.active;
        let recommendations = if self.settings.composer_suggestions && self.editor.focus_handle(cx).is_focused(window) && self.composer_workflow.menu.is_none() {
            self.composer_recommendations(cx)
        } else { Vec::new() };
        let hint = recommendations.first().map(Recommendation::label);
        let mut column = div().flex().flex_col().flex_1().min_w_0().min_h_0();
        if sections.len() == 1 {
            column = column.child(Input::new(&sections[0]).flex_1().h_full().min_h(px(72.)).appearance(false).bordered(false));
        } else {
            let mut list = div().id("composer-sections").flex().flex_col().flex_1().min_h_0().gap_1()
                .track_scroll(&self.composer_workflow.scroll).overflow_y_scroll();
            for (index, editor) in sections.iter().enumerate() {
                let lines = editor.read(cx).value().lines().count().clamp(3, 10);
                let label = if index == active {
                    format!("Draft {} · active · {} submits only this draft", index + 1, self.keymap.label(Action::RunShell))
                } else { format!("Draft {}", index + 1) };
                list = list.child(div().id(("composer-section", index)).flex().flex_col().flex_shrink_0()
                    .border_t_1().border_color(if index == active { cx.theme().primary } else { cx.theme().border })
                    .child(div().text_xs().px_1().text_color(cx.theme().muted_foreground).child(label))
                    .child(Input::new(editor).h(px(lines as f32 * 22. + 12.)).appearance(false).bordered(false)));
            }
            column = column.child(list);
        }
        if let Some(label) = hint {
            column = column.child(button("composer-recommendation", format!("Suggestion: {label} · {}", self.keymap.label(Action::ComposerActions)))
                .text_xs().on_click(cx.listener(|this, _, window, cx| this.open_composer_actions(window, cx))));
        }
        column.into_any_element()
    }

    pub(in crate::app) fn render_composer_actions(&self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.composer_workflow.menu else { return div().into_any_element() };
        let mut panel = div().id("composer-action-menu").track_focus(&menu.focus).key_context("KeaChrome")
            .flex().flex_col().flex_shrink_0().max_h(px(220.)).overflow_y_scroll().gap_1().p_2()
            .border_1().border_color(cx.theme().border)
            .child(div().text_sm().child("Composer actions · Enter selects · Escape closes · No automatic submission"));
        if let Some(plan) = &menu.preview {
            panel = panel.child(div().text_sm().child(format!("Preview: {} draft(s). Applying changes input only; it does not send it.", plan.sections.len())));
            for (index, editor) in menu.preview_editors.iter().enumerate() {
                panel = panel.child(div().text_xs().child(format!("Draft {}", index + 1)))
                    .child(Input::new(editor).disabled(true).h(px(80.)).appearance(false).bordered(false));
            }
            panel = panel.child(button("apply-composer-action", "Apply transformation · Enter")
                .on_click(cx.listener(|this, _, window, cx| this.accept_composer_preview(window, cx))));
        } else if let Some(rec) = &menu.placeholder {
            panel = panel.child(div().text_sm().child(rec.label()))
                .child(div().text_xs().child("Literal text replacement; no shell quoting or evaluation is performed."))
                .child(Input::new(&menu.value))
                .child(button("preview-placeholder", "Preview replacement · Enter")
                    .on_click(cx.listener(|this, _, window, cx| this.preview_placeholder(window, cx))));
        } else {
            for (index, item) in menu.items.iter().enumerate() {
                panel = panel.child(div().id(("composer-action", index)).px_2().py_1().cursor_pointer()
                    .when(index == menu.selected, |row| row.bg(cx.theme().accent))
                    .child(item.label()).on_click(cx.listener(move |this, _, window, cx| this.preview_composer_action(index, window, cx))));
            }
            if menu.items.is_empty() { panel = panel.child("No matching composer action. The trigger remains literal text."); }
        }
        panel.child(button("close-composer-actions", "Cancel · Escape")
            .on_click(cx.listener(|this, _, window, cx| this.dismiss_composer_actions(window, cx)))).into_any_element()
    }
}
