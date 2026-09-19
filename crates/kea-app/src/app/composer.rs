use super::*;
use gpui_component::input as edit;
use kea_app::{editor::completion, reverse_search::InputKind, terminal::input};
use std::path::PathBuf;

impl KeaView {
    /// `run_shell` is retained as a configuration alias. Both old actions now
    /// submit authored text to the current receiver, never shell driver source.
    pub(super) fn run_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.input_allowed() {
            self.pending_run = None;
            self.result(
                Err(anyhow::anyhow!(
                    "Return to a live terminal before submitting."
                )),
                cx,
            );
            return;
        }
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.pump_session(cx);
        // Validate paste policy before arming a confirmation. In particular,
        // multiline input is not sent to a child without bracketed-paste support.
        if let Err(error) = input::paste(&text, self.session.bracketed_paste()) {
            self.result(Err(error), cx);
            return;
        }
        self.dismiss_completion();
        if self.input_context.ready() {
            self.submit_text(text, window, cx);
        } else {
            self.pending_run = Some(PendingRun {
                text,
                editor: self.editor.clone(),
                context_generation: self.input_context.generation(),
                released: !self.composer_enter_down,
            });
            self.notice = Some("Input state is unconfirmed or nonempty. Enter again sends to the current terminal; any other key cancels. Nothing has been sent.".into());
            cx.notify();
        }
    }

    fn submit_text(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let shell_ready = self.input_context.shell_ready();
        let context_id = self.input_context.id().to_owned();
        let kind = match self.input_context.kind() {
            kea_app::terminal::context::ReceiverKind::Posix if shell_ready => InputKind::Posix,
            kea_app::terminal::context::ReceiverKind::PowerShell if shell_ready => {
                InputKind::PowerShell
            }
            _ => InputKind::Application,
        };
        let result = input::paste(&text, self.session.bracketed_paste()).and_then(|mut bytes| {
            bytes.push(b'\r');
            self.session.send(bytes)
        });
        self.pending_run = None;
        if result.is_ok() {
            if shell_ready
                && !text.is_empty()
                && text.len() <= kea_document::MAX_COMMAND_BYTES
                && !text.contains('\0')
            {
                let id = self.document.allocate_id();
                let at = self.session.note_submission(id, &context_id, &text);
                self.document.note_submission(id, &context_id, &text, at);
            }
            let directory = shell_ready
                .then(|| self.document.directory().map(ToOwned::to_owned))
                .flatten();
            self.reverse_search.update(cx, |search, cx| {
                search.record(text.clone(), kind, directory, cx)
            });
            self.session.scroll_bottom();
            self.session.clear_terminal_selection();
            self.input_context.invalidate();
            self.prompt_line.invalidate();
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.observe_composer(cx);
            self.dismiss_completion();
            match self.settings.post_submit_focus {
                kea_app::config::settings::PostSubmitFocus::Editor => self.focus_editor(window, cx),
                kea_app::config::settings::PostSubmitFocus::Terminal => window.focus(&self.focus),
            }
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
        }
        self.result(result, cx);
        self.capture_draft_history_warning(cx);
    }

    pub(super) fn submit_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pump_session(cx);
        if let Some(pending) = self.pending_run.take() {
            let valid = pending.editor == self.editor
                && pending.context_generation == self.input_context.generation()
                && command_editor::submission_text(&self.editor, window, cx).as_deref()
                    == Some(&pending.text);
            if valid {
                self.submit_text(pending.text, window, cx);
            }
        } else {
            self.run_shell(window, cx);
        }
    }

    pub(super) fn send_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.run_shell(window, cx);
    }

    /// Runs before action dispatch, but only while the actual composer owns
    /// focus. Cancellation falls through to ordinary platform text editing.
    pub(super) fn composer_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editor.focus_handle(cx).is_focused(window) {
            return;
        }
        if self.editor.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        }) {
            self.pending_run = None;
            return;
        }
        self.pump_session(cx);
        let key = &event.keystroke;
        if matches!(key.key.as_str(), "enter" | "return") {
            self.composer_enter_down = true;
        }
        if matches!(
            key.key.as_str(),
            "control" | "ctrl" | "shift" | "alt" | "cmd" | "super"
        ) {
            return;
        }
        let plain = !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.platform
            && !key.modifiers.function;
        let enter = matches!(key.key.as_str(), "enter" | "return")
            && !key.modifiers.alt
            && !key.modifiers.platform
            && !key.modifiers.function
            && (!key.modifiers.shift || key.modifiers.control);
        if self.pending_run.is_some() {
            // Drain queued lifecycle reports before validating the exact target.
            self.pump_session(cx);
            if let Some(pending) = self.pending_run.as_ref() {
                let valid = pending.editor == self.editor
                    && pending.context_generation == self.input_context.generation()
                    && self.editor.read(cx).value().as_ref() == pending.text
                    && self.session.input_allowed();
                if enter && valid {
                    if pending.released {
                        let pending = self.pending_run.take().unwrap();
                        self.submit_text(pending.text, window, cx);
                    }
                    // A held first chord cannot acknowledge its own warning.
                    cx.stop_propagation();
                    return;
                }
            }
            self.pending_run = None;
            self.notice = None;
            cx.notify();
        }
        if self.completion_is_current(window, cx) && plain {
            match key.key.as_str() {
                "tab" => {
                    self.move_completion(!key.modifiers.shift, window, cx);
                    cx.stop_propagation();
                }
                "enter" | "return" if !key.modifiers.shift => {
                    self.accept_completion(window, cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
        }
    }

    pub(super) fn composer_key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if matches!(event.keystroke.key.as_str(), "enter" | "return") {
            self.composer_enter_down = false;
            if let Some(pending) = &mut self.pending_run {
                pending.released = true;
            }
        }
    }

    pub(super) fn insert_newline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editor.focus_handle(cx).is_focused(window) {
            return;
        }
        self.editor.update(cx, |state, cx| {
            if state.marked_text_range(window, cx).is_none() {
                state.replace_text_in_range(None, "\n", window, cx);
            }
        });
        self.dismiss_completion();
        cx.notify();
    }

    pub(super) fn complete_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            return;
        };
        let cursor = self.editor.read(cx).cursor();
        if self.move_completion(true, window, cx) {
            return;
        }
        if !self.candidates.is_empty() {
            self.dismiss_completion();
        }
        let line_prefix = text[..cursor].rsplit('\n').next().unwrap_or("");
        if line_prefix.trim().is_empty() {
            window.dispatch_action(Box::new(edit::Indent), cx);
            return;
        }
        if self.completion_rx.is_some() {
            return;
        }
        self.pump_session(cx);
        let completion_context = self.input_context.local_shell_ready();
        let provider = self
            .input_context
            .ready()
            .then(|| {
                self.completion_providers
                    .for_context(self.input_context.id())
            })
            .flatten();
        if !completion_context && provider.is_none() {
            self.notice = Some("Native completion is available with terminal focus. This receiver has no configured composer completion provider; no text was sent.".into());
            cx.notify();
            return;
        }
        self.completion_context = self.input_context.generation();
        self.completion_source = if provider.is_some() {
            format!("Provider: {}", self.input_context.id())
        } else {
            "Local suggestions (not native shell completion)".into()
        };
        let context_id = self.input_context.id().to_owned();
        let directory = completion_context
            .then(|| self.document.directory().map(PathBuf::from))
            .flatten();
        let shell_path = completion_context
            .then(|| self.shell_metadata.path().map(ToOwned::to_owned))
            .flatten();
        let history = self
            .document
            .blocks()
            .iter()
            .rev()
            .take(500)
            .map(|block| block.input.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        let shell = self.shell;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.completion_generation = self.completion_generation.wrapping_add(1);
        let generation = self.completion_generation;
        self.completion_invalidated = false;
        self.completion_rx = Some(rx);
        std::thread::spawn(move || {
            let candidates = if let Some(provider) = provider {
                provider
                    .complete(generation, &context_id, &text, cursor)
                    .map_err(|error| error.to_string())
            } else {
                Ok(completion::suggest(
                    &text,
                    cursor,
                    directory.as_deref(),
                    shell_path.as_deref(),
                    &history,
                    shell,
                ))
            };
            let _ = tx.send((generation, text, cursor, candidates));
        });
    }

    pub(super) fn apply_completion(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(candidate) = self.candidates.get(index).cloned() else {
            return;
        };
        let valid = self.completion_is_current(window, cx);
        if valid {
            let range = self.completion_text[..candidate.range.start]
                .encode_utf16()
                .count()
                ..self.completion_text[..candidate.range.end]
                    .encode_utf16()
                    .count();
            self.editor.update(cx, |state, cx| {
                if state.marked_text_range(window, cx).is_none() {
                    state.replace_text_in_range(Some(range), &candidate.replacement, window, cx);
                }
                state.focus(window, cx);
            });
        }
        self.dismiss_completion();
        cx.notify();
    }

    pub(super) fn completion_is_current(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.candidates.is_empty()
            || !self.editor.focus_handle(cx).is_focused(window)
            || self.completion_context != self.input_context.generation()
        {
            return false;
        }
        let expected_text = self.completion_text.clone();
        let expected_cursor = self.completion_cursor;
        self.editor.update(cx, |state, cx| {
            state.value().as_ref() == expected_text
                && state.cursor() == expected_cursor
                && state.marked_text_range(window, cx).is_none()
        })
    }

    pub(super) fn dismiss_completion(&mut self) {
        if self.completion_rx.is_some() {
            self.completion_invalidated = true;
        }
        self.candidates.clear();
        self.completion_bounds.clear();
        self.completion_index = 0;
        self.completion_scroll.scroll_to_item(0);
        self.completion_text.clear();
        self.completion_cursor = 0;
        if self.notice.as_deref().is_some_and(|notice| {
            notice.starts_with("Local suggestions")
                || notice.starts_with("Provider:")
                || notice.starts_with("No completion")
        }) {
            self.notice = None;
        }
    }

    pub(super) fn accept_completion(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.completion_is_current(window, cx) {
            return false;
        }
        let index = self.completion_index;
        self.apply_completion(index, window, cx);
        true
    }

    pub(super) fn move_completion(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.completion_is_current(window, cx) {
            return false;
        }
        self.completion_index =
            next_completion_index(self.completion_index, self.candidates.len(), forward);
        self.completion_scroll.scroll_to_item(self.completion_index);
        cx.notify();
        true
    }

    pub(super) fn move_completion_row(
        &mut self,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.completion_is_current(window, cx) {
            return false;
        }
        let points = self
            .completion_bounds
            .iter()
            .map(|bounds| {
                bounds.map(|b| {
                    let center = b.center();
                    (f32::from(center.x), f32::from(center.y))
                })
            })
            .collect::<Vec<_>>();
        self.completion_index = vertical_completion_index(self.completion_index, &points, down);
        self.completion_scroll.scroll_to_item(self.completion_index);
        cx.notify();
        true
    }

    pub(super) fn open_reverse_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_run = None;
        self.dismiss_completion();
        let editor = self.editor.clone();
        let enabled = self.session.input_allowed();
        let directory = self
            .input_context
            .shell_ready()
            .then(|| self.document.directory().map(ToOwned::to_owned))
            .flatten();
        let kind = match (self.input_context.shell_ready(), self.input_context.kind()) {
            (true, kea_app::terminal::context::ReceiverKind::Posix) => InputKind::Posix,
            (true, kea_app::terminal::context::ReceiverKind::PowerShell) => InputKind::PowerShell,
            _ => InputKind::Application,
        };
        self.reverse_search.update(cx, |search, cx| {
            search.set_target(editor, enabled, cx);
            search.open(directory, kind, window, cx);
        });
        cx.notify();
    }

    pub(super) fn toggle_blocks(&mut self, cx: &mut Context<Self>) {
        self.show_blocks = !self.show_blocks;
        self.document_ui.page_start = None;
        self.document_scroll.scroll_to_bottom();
        cx.notify();
    }
}

fn next_completion_index(current: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        (current + 1) % len
    } else if current == 0 {
        len - 1
    } else {
        current - 1
    }
}

fn vertical_completion_index(current: usize, points: &[Option<(f32, f32)>], down: bool) -> usize {
    let Some(Some((x, y))) = points.get(current) else {
        return next_completion_index(current, points.len(), down);
    };
    points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let (px, py) = (*point)?;
            let dy = if down { py - y } else { y - py };
            (dy > 1.).then_some((index, dy, (px - x).abs()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)))
        .map_or_else(
            || next_completion_index(current, points.len(), down),
            |(index, _, _)| index,
        )
}

#[cfg(test)]
#[path = "../../tests/unit/app/composer.rs"]
mod tests;
