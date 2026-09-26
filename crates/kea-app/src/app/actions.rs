use super::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input as edit, IconName, Sizable as _,
};
use kea_app::{
    history::playback,
    terminal::{
        input,
        selection::{self, LocalKey},
    },
};

impl KeaView {
    pub(super) fn apply_setting_change(
        &mut self,
        change: settings_window::SettingsChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use settings_window::SettingsChange;

        let previous = self.settings.clone();
        match change {
            SettingsChange::Appearance(value) => self.settings.appearance = value,
            SettingsChange::FontSize(value) => self.settings.font_size = value,
            SettingsChange::PostSubmitFocus(value) => self.settings.post_submit_focus = value,
            SettingsChange::LineNumbers(value) => self.settings.line_numbers = value,
            SettingsChange::SoftWrap(value) => self.settings.soft_wrap = value,
            SettingsChange::OutputWrap(value) => self.settings.output_wrap = value,
            SettingsChange::SyntaxHighlighting(value) => self.settings.syntax_highlighting = value,
            SettingsChange::ShowBlocks(value) => self.settings.show_blocks = value,
            SettingsChange::PersistHistory(value) => self.settings.persist_history = value,
            SettingsChange::HistoryPersistence(value) => self.settings.history_persistence = value,
            SettingsChange::ShiftMouseSelectsLocally(value) => {
                self.settings.shift_mouse_selects_locally = value
            }
            SettingsChange::AnimateLogo(value) => self.settings.animate_logo = value,
            SettingsChange::ConfirmCtrlC(value) => self.settings.confirm_ctrl_c = value,
            SettingsChange::ComposerSuggestions(value) => {
                self.settings.composer_suggestions = value
            }
        }

        let path = match self.settings.save() {
            Ok(path) => path,
            Err(error) => {
                self.settings = previous;
                self.notice = Some(format!("Settings were not changed: {error}."));
                cx.notify();
                return;
            }
        };

        self.apply_shared_settings(self.settings.clone(), change, window, cx);
        cx.emit(WorkspaceEvent::SettingsChanged(
            self.settings.clone(),
            change,
        ));

        self.notice = Some(
            if matches!(
                change,
                SettingsChange::PersistHistory(_) | SettingsChange::HistoryPersistence(_)
            ) {
                format!(
                    "Settings saved to {}. Draft-history persistence changes take effect on the next launch.",
                    path.display()
                )
            } else {
                format!("Settings saved to {}.", path.display())
            },
        );
        cx.notify();
    }

    /// Apply a successfully saved global change without writing the file again.
    pub(super) fn apply_shared_settings(
        &mut self,
        settings: Settings,
        change: settings_window::SettingsChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use settings_window::SettingsChange;
        self.settings = settings;
        self.cancel_workflow();
        match change {
            SettingsChange::Appearance(_) | SettingsChange::FontSize(_) => {
                rendering::apply_appearance(&self.settings, window, cx);
                self.logo_warmed_frames = 0;
            }
            SettingsChange::LineNumbers(_)
            | SettingsChange::SoftWrap(_)
            | SettingsChange::SyntaxHighlighting(_) => {
                for editor in self.composer_workflow.sections.clone() {
                    command_editor::apply_settings(&editor, self.shell, &self.settings, window, cx);
                }
            }
            SettingsChange::OutputWrap(_) => {
                self.document_ui
                    .set_output_wrap(self.settings.output_wrap, window, cx);
            }
            SettingsChange::ShowBlocks(value) => {
                self.show_blocks = value;
                self.document_ui.page_start = None;
            }
            SettingsChange::AnimateLogo(false) => {
                self.composer_logo_active = false;
                self.composer_logo_again = false;
                self.composer_logo_deadline = None;
            }
            _ => {}
        }
        cx.notify();
    }

    fn copy_document(&mut self, cx: &mut Context<Self>) {
        let scope = if self.show_blocks && !self.session.is_history() {
            "command history"
        } else {
            "visible terminal"
        };
        let text = if self.show_blocks && !self.session.is_history() {
            self.document.text()
        } else {
            self.session.screen().text().trim_end().to_string()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notice = Some(format!("Copy requested for {scope}."));
        cx.notify();
    }

    pub(super) fn copy_terminal_selection(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.session.terminal_selection_text() else {
            self.notice = if self.session.terminal_explicit_selection_active() {
                Some("No terminal text is selected.".into())
            } else if self.session.terminal_mouse_reporting()
                && self.settings.shift_mouse_selects_locally
            {
                Some("No terminal text is selected. Hold Shift while dragging over the terminal, then copy the selection.".into())
            } else if self.session.terminal_mouse_reporting() {
                Some(
                    "No terminal text is selected. Use Select terminal text for local selection."
                        .into(),
                )
            } else {
                Some("No terminal text is selected. Drag over terminal output, then copy the selection.".into())
            };
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notice = Some("Copy requested for terminal selection.".into());
        cx.notify();
    }

    pub(super) fn return_terminal_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.session.scroll_bottom();
        self.terminal_scroll_remainder = 0.;
        cx.notify();
    }

    pub(super) fn note_forwarded_terminal_input(&mut self) {
        self.interrupt.cancel();
        self.session.scroll_bottom();
        self.session.clear_terminal_selection();
        self.prompt_line.invalidate();
        self.pending_run = None;
        self.document.note_terminal_input();
        self.input_context.invalidate();
        self.notice = None;
    }

    pub(super) fn select_terminal_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.terminal_local_selection_active() {
            self.session.clear_terminal_selection();
        } else {
            self.session.enter_terminal_selection();
        }
        window.focus(&self.focus);
        self.notice = None;
        cx.notify();
    }

    pub(super) fn seek_x(&mut self, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bounds) = self.timeline_track_bounds else {
            return;
        };
        let Some(fraction) = playback::fraction_at_x(
            f32::from(x),
            f32::from(bounds.origin.x),
            f32::from(bounds.size.width),
        ) else {
            return;
        };
        let at = playback::micros_at_fraction(fraction, self.session.recording().duration());
        let result = self.session.seek_time(at);
        window.focus(&self.focus);
        self.result(result, cx);
    }

    pub(super) fn invoke(&mut self, event: &Invoke, window: &mut Window, cx: &mut Context<Self>) {
        let terminal_focused = self.focus.is_focused(window);
        match event.action {
            Action::Copy if terminal_focused => self.copy_terminal_selection(cx),
            Action::Paste if terminal_focused => self.paste_terminal(cx),
            Action::Copy => window.dispatch_action(Box::new(edit::Copy), cx),
            Action::Cut => window.dispatch_action(Box::new(edit::Cut), cx),
            Action::Paste => window.dispatch_action(Box::new(edit::Paste), cx),
            Action::Undo => window.dispatch_action(Box::new(edit::Undo), cx),
            Action::Redo => window.dispatch_action(Box::new(edit::Redo), cx),
            Action::SelectAll => window.dispatch_action(Box::new(edit::SelectAll), cx),
            Action::Find => window.dispatch_action(Box::new(edit::Search), cx),
            Action::CopyDocument => self.copy_document(cx),
            Action::FocusEditor => {
                if terminal_focused {
                    self.focus_editor(window, cx);
                } else if self.editor.focus_handle(cx).is_focused(window) {
                    window.focus(&self.focus);
                } else {
                    self.focus_editor(window, cx);
                }
            }
            Action::SelectTerminalText => self.select_terminal_text(window, cx),
            Action::Interrupt => self.request_interrupt(vec![3], window, cx),
            Action::ComposerActions => self.open_composer_actions(window, cx),
            Action::SplitComposer => self.split_composer(window, cx),
            Action::MergeComposer => self.merge_composer(window, cx),
            Action::NextComposer => self.navigate_composer(true, window, cx),
            Action::PreviousComposer => self.navigate_composer(false, window, cx),
            Action::SendSelection => self.send_selection(window, cx),
            Action::UndoComposerLayout => self.undo_composer_layout(false, window, cx),
            Action::RedoComposerLayout => self.undo_composer_layout(true, window, cx),
            Action::RunShell => self.run_shell(window, cx),
            Action::SendApplication => self.send_editor(window, cx),
            Action::Newline => {
                if !self.accept_completion(window, cx) {
                    self.insert_newline(window, cx);
                }
            }
            Action::Complete => self.complete_editor(window, cx),
            Action::ReverseSearch => {
                if self.editor.focus_handle(cx).is_focused(window) {
                    self.open_reverse_search(window, cx);
                }
            }
            Action::ToggleBlocks => self.toggle_blocks(cx),
            Action::PreviousEvent | Action::NextEvent => {
                let result = self.session.step(if event.action == Action::PreviousEvent {
                    -1
                } else {
                    1
                });
                window.focus(&self.focus);
                self.result(result, cx);
            }
            Action::BackFiveSeconds | Action::ForwardFiveSeconds => {
                let at = if event.action == Action::BackFiveSeconds {
                    self.session.position().saturating_sub(5_000_000)
                } else {
                    self.session.position().saturating_add(5_000_000)
                };
                let result = self.session.seek_time(at);
                window.focus(&self.focus);
                self.result(result, cx);
            }
            Action::PlayPause => {
                self.session.toggle_playback();
                cx.notify();
            }
            Action::GoLive => {
                self.session.go_live();
                self.prompt_line.invalidate();
                self.pending_run = None;
                self.notice = None;
                self.focus_editor(window, cx);
                cx.notify();
            }
            Action::Quit
            | Action::NewTerminal
            | Action::CloseTerminal
            | Action::NextTerminal
            | Action::PreviousTerminal
            | Action::MoveTerminalLeft
            | Action::MoveTerminalRight => {
                cx.emit(WorkspaceEvent::Action(event.action));
            }
        }
    }

    pub(super) fn terminal_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_marked_text = self.terminal_composition.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        });
        if input::defer_to_ime(&event.keystroke, has_marked_text) {
            return;
        }
        if input::is_terminal_paste(&event.keystroke) {
            if self.session.input_allowed() {
                self.paste_terminal(cx);
            }
            cx.stop_propagation();
            return;
        }
        match selection::local_key(
            &event.keystroke.key,
            event.keystroke.modifiers.shift,
            event.keystroke.modifiers.control,
            event.keystroke.modifiers.alt,
            event.keystroke.modifiers.platform,
            event.keystroke.modifiers.function,
            self.session.terminal_local_selection_active(),
        ) {
            LocalKey::Copy => {
                self.copy_terminal_selection(cx);
                cx.stop_propagation();
                return;
            }
            LocalKey::Escape => {
                self.session.clear_terminal_selection();
                self.notice = None;
                cx.notify();
                cx.stop_propagation();
                return;
            }
            LocalKey::Move { motion, extend } => {
                self.session.move_terminal_selection(motion, extend);
                cx.notify();
                cx.stop_propagation();
                return;
            }
            LocalKey::Forward => {}
        }
        self.session.clear_terminal_selection();
        cx.notify();
        if let Some(bytes) = input::encode(
            &event.keystroke,
            self.session.application_cursor(),
            self.session.terminal_extended_keyboard(),
        ) {
            if self.session.input_allowed() {
                if interrupt::is_ctrl_c(&event.keystroke) {
                    self.request_interrupt(bytes, window, cx);
                    cx.stop_propagation();
                    return;
                }
                self.interrupt.cancel();
                let tracked_bytes = bytes.clone();
                let result = self.session.send(bytes);
                if result.is_ok() {
                    self.session.scroll_bottom();
                    self.session.clear_terminal_selection();
                    self.pending_run = None;
                    let modifiers = event.keystroke.modifiers;
                    if event.keystroke.key == "backspace"
                        && !modifiers.control
                        && !modifiers.alt
                        && !modifiers.platform
                        && !modifiers.function
                        && tracked_bytes == [127]
                    {
                        self.prompt_line.note_backspace();
                    } else if !modifiers.control
                        && !modifiers.alt
                        && !modifiers.platform
                        && !modifiers.function
                        && tracked_bytes
                            .iter()
                            .all(|byte| *byte == b' ' || byte.is_ascii_graphic())
                    {
                        self.prompt_line
                            .note_text(std::str::from_utf8(&tracked_bytes).unwrap_or_default());
                    } else {
                        self.prompt_line.invalidate();
                    }
                    self.document.note_terminal_input();
                    self.input_context.invalidate();
                }
                self.result(result, cx);
            }
            cx.stop_propagation();
        }
    }

    pub(super) fn control(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        action: Action,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        button(id, label).on_click(
            cx.listener(move |this, _, window, cx| this.invoke(&Invoke { action }, window, cx)),
        )
    }

    pub(super) fn icon_control(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: impl Into<SharedString>,
        action: Action,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .icon(icon)
            .tooltip(tooltip)
            .ghost()
            .small()
            .on_click(
                cx.listener(move |this, _, window, cx| this.invoke(&Invoke { action }, window, cx)),
            )
    }
}
