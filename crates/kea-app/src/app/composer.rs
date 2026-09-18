use super::*;
use gpui_component::input as edit;
use kea_app::{editor::completion, reverse_search::InputKind, terminal::input};
use std::path::PathBuf;

impl KeaView {
    pub(super) fn run_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.input_allowed() {
            self.result(
                Err(anyhow::anyhow!("Return to LIVE before sending input.")),
                cx,
            );
            return;
        }
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            self.notice = Some("Focus the composer before using Run in shell.".into());
            cx.notify();
            return;
        };
        if text.is_empty() {
            return;
        }
        if self.pending_run.is_some() {
            self.notice =
                Some("Waiting for a fresh shell prompt; the draft has not run yet.".into());
            cx.notify();
            return;
        }
        let _ = self.pump_session(cx);
        let Some(shell) = self.shell else {
            self.result(
                Err(anyhow::anyhow!(
                    "Run in shell requires an integrated local shell; use Send to app instead."
                )),
                cx,
            );
            return;
        };
        if !self.document.prompt_ready() {
            if self.prompt_line.can_recover_empty_line() {
                let result = self.session.send(vec![3]);
                if result.is_ok() {
                    self.pending_run = Some(PendingRun {
                        text,
                        editor: self.editor.clone(),
                    });
                    self.prompt_line.invalidate();
                    self.document.note_terminal_input();
                    self.notice =
                        Some("Recovering the shell prompt before running the draft.".into());
                    cx.notify();
                } else {
                    self.result(result, cx);
                }
                return;
            }
            self.result(
                Err(anyhow::anyhow!(
                    "Run in shell needs a freshly reported prompt. Use the terminal to recover the shell, or Send to app when another program owns input."
                )),
                cx,
            );
            return;
        }
        self.submit_shell(text, shell, window, cx);
    }

    fn submit_shell(
        &mut self,
        text: String,
        shell: ShellFlavor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.session.input_allowed() || !self.document.prompt_ready() {
            self.result(
                Err(anyhow::anyhow!(
                    "The shell prompt changed before the draft could run; the draft was preserved."
                )),
                cx,
            );
            return;
        }
        let id = self.document.allocate_id();
        let prompt_column = self.session.screen().cursor.map_or(0, |(_, column)| column);
        let wrapper = match shell.wrap(id, &text, prompt_column) {
            Ok(wrapper) => wrapper,
            Err(error) => {
                self.result(Err(error.into()), cx);
                return;
            }
        };
        let result = self.session.send_hidden(wrapper);
        if result.is_ok() {
            let directory = self.document.directory().map(ToOwned::to_owned);
            let kind = match shell {
                ShellFlavor::Posix => InputKind::Posix,
                ShellFlavor::PowerShell => InputKind::PowerShell,
            };
            self.reverse_search.update(cx, |search, cx| {
                search.record(text.clone(), kind, directory, cx);
            });
            self.session.scroll_bottom();
            self.session.clear_terminal_selection();
            self.prompt_line.invalidate();
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.observe_composer(cx);
            self.candidates.clear();
            window.focus(&self.focus);
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
        }
        self.result(result, cx);
        self.capture_draft_history_warning(cx);
    }

    pub(super) fn complete_pending_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_run.take() else {
            return;
        };
        let unchanged = self.editor == pending.editor
            && command_editor::submission_text(&self.editor, window, cx).as_deref()
                == Some(pending.text.as_str());
        if !unchanged {
            self.notice = Some(
                "The draft or focus changed while the shell prompt was recovering; nothing was run."
                    .into(),
            );
            cx.notify();
            return;
        }
        let Some(shell) = self.shell else {
            self.notice =
                Some("Shell integration became unavailable; the draft was not run.".into());
            cx.notify();
            return;
        };
        self.submit_shell(pending.text, shell, window, cx);
    }

    pub(super) fn send_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.input_allowed() {
            self.result(
                Err(anyhow::anyhow!("Return to LIVE before sending input.")),
                cx,
            );
            return;
        }
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            self.notice =
                Some("Focus the composer before sending text to the terminal app.".into());
            cx.notify();
            return;
        };
        if text.is_empty() {
            return;
        }
        let result = input::paste(&text, self.session.bracketed_paste()).and_then(|mut bytes| {
            bytes.push(b'\r');
            self.session.send(bytes)
        });
        if result.is_ok() {
            self.reverse_search.update(cx, |search, cx| {
                search.record(text.clone(), InputKind::Application, None, cx);
            });
            self.session.scroll_bottom();
            self.session.clear_terminal_selection();
            self.prompt_line.invalidate();
            self.pending_run = None;
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.observe_composer(cx);
            self.candidates.clear();
            window.focus(&self.focus);
        }
        self.result(result, cx);
        self.capture_draft_history_warning(cx);
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
        self.candidates.clear();
        cx.notify();
    }

    pub(super) fn complete_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            return;
        };
        let cursor = self.editor.read(cx).cursor();
        let line_prefix = text[..cursor].rsplit('\n').next().unwrap_or("");
        if line_prefix.trim().is_empty() {
            window.dispatch_action(Box::new(edit::Indent), cx);
            return;
        }
        if !self.candidates.is_empty()
            && text == self.completion_text
            && cursor == self.completion_cursor
        {
            self.apply_completion(0, window, cx);
            return;
        }
        if self.completion_rx.is_some() {
            return;
        }
        let metadata_current = self.document.prompt_ready();
        let directory = metadata_current
            .then(|| self.document.directory().map(PathBuf::from))
            .flatten();
        let shell_path = metadata_current
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
        self.completion_rx = Some(rx);
        self.candidates.clear();
        std::thread::spawn(move || {
            let candidates = completion::suggest(
                &text,
                cursor,
                directory.as_deref(),
                shell_path.as_deref(),
                &history,
                shell,
            );
            let _ = tx.send((text, cursor, candidates));
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
        let valid = self.editor.read(cx).value().as_ref() == self.completion_text
            && self.editor.read(cx).cursor() == self.completion_cursor;
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
        self.candidates.clear();
        self.notice = None;
        cx.notify();
    }

    pub(super) fn open_reverse_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_run = None;
        self.completion_rx = None;
        self.candidates.clear();
        let editor = self.editor.clone();
        let enabled = self.session.input_allowed();
        let directory = self
            .document
            .prompt_ready()
            .then(|| self.document.directory().map(ToOwned::to_owned))
            .flatten();
        let kind = match (self.document.prompt_ready(), self.shell) {
            (true, Some(ShellFlavor::Posix)) => InputKind::Posix,
            (true, Some(ShellFlavor::PowerShell)) => InputKind::PowerShell,
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
