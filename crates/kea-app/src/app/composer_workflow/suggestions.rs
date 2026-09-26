use super::*;

impl KeaView {
    pub(in crate::app) fn composer_recommendations(&mut self, cx: &App) -> Vec<Recommendation> {
        let text = self.editor.read(cx).value();
        let cursor = self.editor.read(cx).cursor();
        let prefix = &self.settings.composer_action_prefix;
        if let Some((editor, old, old_cursor, old_prefix, recommendations)) = &self.composer_workflow.recommendations {
            if *editor == self.editor && *old == text && *old_cursor == cursor && old_prefix == prefix {
                return recommendations.clone();
            }
        }
        let recommendations = workflow::recommend(&text, cursor, prefix);
        self.composer_workflow.recommendations = (text.len() <= workflow::MAX_ACTION_BYTES)
            .then(|| (self.editor.clone(), text, cursor, prefix.clone(), recommendations.clone()));
        recommendations
    }

    pub(in crate::app) fn open_composer_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor_available(window, cx) { return; }
        let source = self.editor.read(cx).value().to_string();
        if source.len() > workflow::MAX_ACTION_BYTES {
            self.notice = Some("Composer actions analyze at most 256 KiB. The draft is unchanged.".into());
            cx.notify();
            return;
        }
        self.cancel_workflow();
        self.pending_run = None;
        self.dismiss_completion();
        let cursor = self.editor.read(cx).cursor();
        let recommendations = self.composer_recommendations(cx);
        let trigger = recommendations.iter().find_map(|rec| match rec {
            Recommendation::Actions { query, range } => Some((query.clone(), range.clone())),
            _ => None,
        });
        let mut items = recommendations.into_iter().filter(|rec| !matches!(rec, Recommendation::Actions { .. }))
            .map(ComposerAction::Recommendation).collect::<Vec<_>>();
        items.extend([ComposerAction::Split, ComposerAction::Add, ComposerAction::Merge,
            ComposerAction::Previous, ComposerAction::Next, ComposerAction::UndoLayout,
            ComposerAction::RedoLayout, ComposerAction::History, ComposerAction::SendSelection]);
        if let Some((query, _)) = &trigger {
            let query = if query == "memory" { "saved memory" } else { query.as_str() };
            // Only actions with an explicit trigger-consumption contract are
            // exposed by shorthand. The full menu remains available elsewhere.
            items.retain(|item| matches!(item, ComposerAction::Split | ComposerAction::Add | ComposerAction::History)
                && item.label().to_lowercase().contains(query));
        }
        let focus = cx.focus_handle();
        window.focus(&focus);
        let value = cx.new(|cx| InputState::new(window, cx).placeholder("Literal replacement value"));
        self.composer_workflow.menu = Some(ComposerMenu {
            target: self.editor.clone(), source, cursor, items, selected: 0,
            prefix_range: trigger.map(|(_, range)| range), placeholder: None,
            value, preview: None, preview_editors: Vec::new(), focus,
        });
        cx.notify();
    }

    pub(in crate::app) fn dismiss_composer_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_workflow.menu = None;
        self.focus_editor(window, cx);
        cx.notify();
    }

    pub(in crate::app) fn preview_composer_action(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &self.composer_workflow.menu else { return };
        let Some(action) = menu.items.get(index).cloned() else { return };
        let source = menu.source.clone();
        let cursor = menu.cursor;
        let prefix_range = menu.prefix_range.clone();
        if menu.target != self.editor || self.editor.read(cx).value().as_ref() != source
            || self.editor.read(cx).cursor() != cursor { return; }
        match action {
            ComposerAction::Recommendation(rec @ Recommendation::FillPlaceholder { .. }) => {
                let menu = self.composer_workflow.menu.as_mut().unwrap();
                menu.placeholder = Some(rec);
                menu.value.update(cx, |state, cx| state.focus(window, cx));
                cx.notify();
            }
            ComposerAction::Recommendation(rec) => {
                if let Some(plan) = EditPlan::for_recommendation(&source, cursor, &rec, None) {
                    self.show_composer_preview(plan, window, cx);
                }
            }
            ComposerAction::Split | ComposerAction::Add => {
                let plan = if matches!(action, ComposerAction::Add) {
                    let text = if let Some(range) = prefix_range {
                        format!("{}{}", &source[..range.start], &source[range.end..])
                    } else { source.clone() };
                    Some(EditPlan { source, cursor, sections: vec![text, String::new()] })
                } else if let Some(range) = prefix_range {
                    EditPlan::split_marker(&source, cursor, range)
                } else { EditPlan::split(&source, cursor) };
                if let Some(plan) = plan { self.show_composer_preview(plan, window, cx); }
            }
            action => {
                self.dismiss_composer_actions(window, cx);
                if matches!(action, ComposerAction::History) {
                    if let Some(range) = prefix_range {
                        let utf16 = source[..range.start].encode_utf16().count()..source[..range.end].encode_utf16().count();
                        self.editor.update(cx, |state, cx| state.replace_text_in_range(Some(utf16), "", window, cx));
                    }
                }
                match action {
                    ComposerAction::Merge => self.merge_composer(window, cx),
                    ComposerAction::Previous => self.navigate_composer(false, window, cx),
                    ComposerAction::Next => self.navigate_composer(true, window, cx),
                    ComposerAction::UndoLayout => self.undo_composer_layout(false, window, cx),
                    ComposerAction::RedoLayout => self.undo_composer_layout(true, window, cx),
                    ComposerAction::History => self.open_reverse_search(window, cx),
                    ComposerAction::SendSelection => self.send_selection(window, cx),
                    _ => {}
                }
            }
        }
    }

    pub(in crate::app) fn preview_placeholder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &self.composer_workflow.menu else { return };
        let Some(rec) = &menu.placeholder else { return };
        if menu.value.update(cx, |state, cx| state.marked_text_range(window, cx).is_some()) { return; }
        let plan = EditPlan::for_recommendation(&menu.source, menu.cursor, rec, Some(&menu.value.read(cx).value()));
        if let Some(plan) = plan { self.show_composer_preview(plan, window, cx); }
        else {
            self.notice = Some("Replacement exceeds the 256 KiB action limit. Nothing was changed.".into());
            cx.notify();
        }
    }

    pub(in crate::app) fn show_composer_preview(&mut self, plan: EditPlan, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = &mut self.composer_workflow.menu else { return };
        menu.preview_editors = plan.sections.iter().map(|text| {
            cx.new(|cx| InputState::new(window, cx).multi_line(true).default_value(text.clone()))
        }).collect();
        menu.preview = Some(plan);
        window.focus(&menu.focus);
        cx.notify();
    }

    pub(in crate::app) fn accept_composer_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.composer_workflow.menu.take() else { return };
        let Some(plan) = menu.preview else { return };
        if menu.target != self.editor { return; }
        self.focus_editor(window, cx);
        self.apply_composer_plan(plan, window, cx);
    }
}
