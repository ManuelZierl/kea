use super::*;

impl KeaView {
    pub(in crate::app) fn cancel_workflow(&mut self) {
        self.interrupt.cancel();
        self.composer_workflow.menu = None;
        // Keep physical-key ownership until release, even after changing sections.
    }

    pub(in crate::app) fn validate_workflow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.validate_interrupt(window, cx);
        if !self.visible || !window.is_window_active() {
            self.cancel_workflow();
            self.composer_workflow.key_latch.clear();
            self.composer_workflow.owned_keys.clear();
            self.interrupt.key_latch.clear();
            self.interrupt.swallowed.clear();
            return;
        }
        let valid = self.composer_workflow.menu.as_ref().is_none_or(|menu| {
            menu.target == self.editor
                && self.editor.read(cx).value().as_ref() == menu.source
                && self.editor.read(cx).cursor() == menu.cursor
                && (menu.focus.is_focused(window) || menu.value.focus_handle(cx).is_focused(window)
                    || menu.preview_editors.iter().any(|e| e.focus_handle(cx).contains_focused(window, cx)))
        });
        if !valid { self.composer_workflow.menu = None; }
    }

    pub(in crate::app) fn has_pending_sections(&self, cx: &App) -> bool {
        self.composer_workflow.sections.iter().any(|editor| !editor.read(cx).value().is_empty())
    }

    pub(in crate::app) fn sync_composer_section(&mut self, cx: &mut Context<Self>) {
        let workflow = &mut self.composer_workflow;
        if workflow.sections[workflow.active] != self.editor {
            workflow.sections[workflow.active] = self.editor.clone();
            workflow.undo.clear();
            workflow.redo.clear();
            workflow.menu = None;
            workflow.subscriptions = observe_sections(&workflow.sections, cx);
        }
        command_editor::activate_section(&self.editor, cx);
    }

    pub(in crate::app) fn select_composer_section(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(editor) = self.composer_workflow.sections.get(index).cloned() else { return };
        self.cancel_workflow();
        self.pending_run = None;
        self.dismiss_completion();
        self.composer_workflow.active = index;
        self.editor = editor;
        self.observe_composer(cx);
        self.composer_workflow.scroll.scroll_to_item(index);
        cx.notify();
    }

    pub(in crate::app) fn navigate_composer(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx) { return; }
        let count = self.composer_workflow.sections.len();
        let active = self.composer_workflow.active;
        let index = if forward { (active + 1) % count } else { (active + count - 1) % count };
        self.select_composer_section(index, cx);
        self.focus_editor(window, cx);
    }

    pub(in crate::app) fn editor_available(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.visible && self.editor.focus_handle(cx).is_focused(window)
            && self.editor.update(cx, |state, cx| state.marked_text_range(window, cx).is_none())
    }

    fn snapshot_layout(&self, cx: &App) -> LayoutSnapshot {
        LayoutSnapshot {
            editors: self.composer_workflow.sections.clone(),
            values: self.composer_workflow.sections.iter().map(|e| e.read(cx).value().to_string()).collect(),
            active: self.composer_workflow.active,
        }
    }

    fn install_layout(&mut self, editors: Vec<Entity<InputState>>, active: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_workflow.sections = editors;
        self.composer_workflow.subscriptions = observe_sections(&self.composer_workflow.sections, cx);
        self.select_composer_section(active, cx);
        self.focus_editor(window, cx);
    }

    fn retain_layout_change(&mut self, before: LayoutSnapshot, cx: &App) {
        let after = self.snapshot_layout(cx);
        let workflow = &mut self.composer_workflow;
        workflow.redo.clear();
        workflow.undo.push(LayoutChange { before, after });
        while workflow.undo.len() > MAX_LAYOUT_HISTORY
            || workflow.undo.iter().map(LayoutChange::bytes).sum::<usize>() > MAX_LAYOUT_HISTORY_BYTES
        { workflow.undo.remove(0); }
    }

    pub(in crate::app) fn split_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx) { return; }
        let text = self.editor.read(cx).value();
        let cursor = self.editor.read(cx).cursor();
        if let Some(plan) = EditPlan::split(&text, cursor) { self.apply_composer_plan(plan, window, cx); }
        else {
            self.notice = Some("This draft exceeds the 256 KiB editor-action limit; it was not changed.".into());
            cx.notify();
        }
    }

    pub(in crate::app) fn apply_composer_plan(&mut self, plan: EditPlan, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx)
            || !plan.is_current(&self.editor.read(cx).value(), self.editor.read(cx).cursor())
        {
            self.notice = Some("The draft changed; open composer actions again. Nothing was applied.".into());
            cx.notify();
            return;
        }
        let new_count = self.composer_workflow.sections.len() - 1 + plan.sections.len();
        if plan.sections.is_empty() || new_count > workflow::MAX_SECTIONS {
            self.notice = Some("At most 32 composer sections are allowed. Nothing was changed.".into());
            cx.notify();
            return;
        }
        self.pending_run = None;
        self.dismiss_completion();
        command_editor::cancel_submission_candidate(cx);
        if plan.sections.len() == 1 {
            self.editor.update(cx, |state, cx| {
                state.replace_text_in_range(Some(0..plan.source.encode_utf16().count()), &plan.sections[0], window, cx);
            });
        } else {
            let before = self.snapshot_layout(cx);
            if before.bytes().saturating_mul(2) + plan.sections.iter().map(String::len).sum::<usize>() > MAX_LAYOUT_HISTORY_BYTES {
                self.notice = Some("The structural-undo budget is full for this draft. Nothing was changed.".into());
                cx.notify();
                return;
            }
            let mut editors = self.composer_workflow.sections.clone();
            let index = self.composer_workflow.active;
            let replacements = plan.sections.iter().map(|text| {
                command_editor::new_draft(self.shell, &self.settings, text, window, cx)
            }).collect::<Vec<_>>();
            editors.splice(index..index + 1, replacements);
            self.install_layout(editors, index, window, cx);
            self.retain_layout_change(before, cx);
        }
        self.composer_workflow.menu = None;
        cx.notify();
    }

    pub(in crate::app) fn merge_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx) || self.composer_workflow.active == 0 { return; }
        let before = self.snapshot_layout(cx);
        let index = self.composer_workflow.active;
        let merged = format!("{}\n{}", before.values[index - 1], before.values[index]);
        if merged.len() > workflow::MAX_ACTION_BYTES || before.bytes().saturating_mul(2) + merged.len() > MAX_LAYOUT_HISTORY_BYTES {
            self.notice = Some("Merged text exceeds the editor-action budget. Nothing was changed.".into());
            cx.notify();
            return;
        }
        command_editor::cancel_submission_candidate(cx);
        let mut editors = before.editors.clone();
        let replacement = command_editor::new_draft(self.shell, &self.settings, &merged, window, cx);
        editors.splice(index - 1..index + 1, [replacement]);
        self.install_layout(editors, index - 1, window, cx);
        self.retain_layout_change(before, cx);
    }

    pub(in crate::app) fn undo_composer_layout(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx) { return; }
        let workflow = &mut self.composer_workflow;
        let stack = if redo { &mut workflow.redo } else { &mut workflow.undo };
        let Some(change) = stack.last() else { return };
        let expected = if redo { &change.before } else { &change.after };
        if !expected.matches(&workflow.sections, cx) {
            self.notice = Some("Undo intervening text edits first. Composer layout undo will not discard edited text.".into());
            cx.notify();
            return;
        }
        let change = stack.pop().unwrap();
        let target = if redo { &change.after } else { &change.before };
        self.install_layout(target.editors.clone(), target.active, window, cx);
        if redo { self.composer_workflow.undo.push(change); } else { self.composer_workflow.redo.push(change); }
    }

    pub(in crate::app) fn clear_composer_layout_history(&mut self) {
        self.composer_workflow.undo.clear();
        self.composer_workflow.redo.clear();
    }

    pub(in crate::app) fn finish_section_submission(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Only a completed PTY write reaches here. Never wait for child completion.
        self.clear_composer_layout_history();
        let index = self.composer_workflow.active;
        let mut editors = self.composer_workflow.sections.clone();
        editors.remove(index);
        if index == editors.len() {
            editors.push(command_editor::new_draft(self.shell, &self.settings, "", window, cx));
        }
        self.install_layout(editors, index, window, cx);
    }

    pub(in crate::app) fn send_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx) { return; }
        let selected = self.editor.update(cx, |state, cx| {
            let selection = state.selected_text_range(true, window, cx)?;
            if selection.range.is_empty() { return None; }
            state.text_for_range(selection.range, &mut None, window, cx)
        });
        if let Some(text) = selected { self.request_submission(text, false, window, cx); }
    }
}
