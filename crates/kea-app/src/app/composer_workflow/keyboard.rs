use super::*;

impl KeaView {
    pub(in crate::app) fn workflow_keystroke(&mut self, event: &KeystrokeEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.visible { return false; }
        let physical = if event.keystroke.key == "return" { "enter" } else { event.keystroke.key.as_str() };
        if self.composer_workflow.owned_keys.iter().any(|key| key == physical) {
            cx.stop_propagation();
            return true;
        }
        if self.interrupt_keystroke(event, window, cx) { return true; }
        let key = &event.keystroke;
        let action = self.keymap.action_for(key);
        let enter = matches!(key.key.as_str(), "enter" | "return");
        let submission = matches!(action, Some(Action::RunShell | Action::SendApplication | Action::SendSelection));
        let tracked = enter || submission;
        let fresh = !tracked || self.composer_workflow.key_latch.press(&key.key);
        if self.composer_workflow.menu.is_some() {
            self.validate_workflow(window, cx);
            if let Some(menu) = &mut self.composer_workflow.menu {
                if menu.value.focus_handle(cx).is_focused(window)
                    && menu.value.update(cx, |state, cx| state.marked_text_range(window, cx).is_some()) {
                    return false;
                }
                if key.key == "escape" {
                    self.dismiss_composer_actions(window, cx);
                } else if enter {
                    self.composer_workflow.owned_keys.push(physical.to_owned());
                    let plain = !key.modifiers.control && !key.modifiers.shift && !key.modifiers.alt
                        && !key.modifiers.platform && !key.modifiers.function;
                    if plain && fresh {
                        if menu.preview.is_some() { self.accept_composer_preview(window, cx); }
                        else if menu.placeholder.is_some() { self.preview_placeholder(window, cx); }
                        else { let index = menu.selected; self.preview_composer_action(index, window, cx); }
                    }
                } else if menu.focus.is_focused(window) && matches!(key.key.as_str(), "up" | "down") {
                    if !menu.items.is_empty() {
                        menu.selected = if key.key == "down" { (menu.selected + 1) % menu.items.len() }
                            else { (menu.selected + menu.items.len() - 1) % menu.items.len() };
                    }
                } else if submission {
                    // An explicitly opened action menu is not a submission surface.
                    self.composer_workflow.owned_keys.push(physical.to_owned());
                } else { return false; }
                cx.stop_propagation();
                cx.notify();
                return true;
            }
        }
        if self.editor.focus_handle(cx).is_focused(window) && (submission || self.pending_run.is_some() && enter) {
            self.composer_workflow.owned_keys.push(physical.to_owned());
            if !fresh {
                cx.stop_propagation();
                return true;
            }
        }
        false
    }
}
