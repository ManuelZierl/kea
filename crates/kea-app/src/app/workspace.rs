use super::*;
use gpui_component::{input as edit, ActiveTheme as _};
use kea_app::ui::logo::{
    warm_frame, KeaLogo, KeaLogoAnimation, FRAME_COUNT as LOGO_FRAME_COUNT, KEA_LOGO_PECK_DURATION,
};
use kea_session::Observed;
use std::time::Duration;

impl KeaView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        session: Session,
        document: Document,
        shell: Option<ShellFlavor>,
        keymap: Keymap,
        settings: Settings,
        initial_focus: InitialFocus,
        notice: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (completion_providers, provider_warning) = Providers::load();
        let notice = provider_warning.or(notice);
        let weak = cx.entity().downgrade();
        let composer_keys = cx.intercept_keystrokes(move |event, window, cx| {
            let _ = weak.update(cx, |this, cx| this.composer_keystroke(event, window, cx));
        });
        let focus = cx.focus_handle();
        let editor = command_editor::new_draft(shell, &settings, "", window, cx);
        if initial_focus == InitialFocus::Terminal {
            window.focus(&focus);
        } else {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        let reverse_search =
            cx.new(|cx| ReverseSearchView::new(settings.history_persistence, window, cx));
        reverse_search.update(cx, |search, cx| {
            search.set_target(editor.clone(), session.input_allowed(), cx);
        });
        let memory_changed = cx.observe(&reverse_search, |_, _, cx| cx.notify());
        let terminal_composition = cx.new(|cx| InputState::new(window, cx));
        let document_ui = document_view::DocumentUi::new(window, cx);
        let composer_change =
            cx.subscribe(
                &editor,
                |this, _, event: &edit::InputEvent, cx| match event {
                    edit::InputEvent::Change => {
                        this.pending_run = None;
                        this.dismiss_completion();
                        this.note_composer_typed(cx);
                    }
                    edit::InputEvent::Blur => {
                        this.composer_enter_down = false;
                        this.pending_run = None;
                        this.dismiss_completion();
                        cx.notify();
                    }
                    _ => {}
                },
            );
        let filter_change = cx.subscribe(
            &document_ui.filter,
            |this, _, event: &edit::InputEvent, cx| {
                if matches!(event, edit::InputEvent::Change) {
                    this.document_ui.page_start = Some(0);
                    this.document_ui.dirty = true;
                    cx.notify();
                }
            },
        );
        let focus_lost = cx.on_blur(&focus, window, |this, _, cx| {
            this.session.terminal_selection_focus_lost();
            this.dismiss_completion();
            cx.notify();
        });
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            rendering::apply_appearance(&this.settings, window, cx);
            this.logo_warmed_frames = 0;
            cx.notify();
        });
        let pump = cx.spawn_in(window, async move |this, cx| loop {
            Timer::after(Duration::from_millis(16)).await;
            let disconnected = cx
                .update(|window, cx| {
                    this.update(cx, |this, cx| {
                        let completion = this.completion_rx.as_ref().map(|rx| rx.try_recv());
                        match completion {
                            Some(Ok((generation, text, cursor, candidates))) => {
                                let request_live = generation == this.completion_generation
                                    && !this.completion_invalidated
                                    && this.completion_context == this.input_context.generation();
                                this.completion_rx = None;
                                this.completion_invalidated = false;
                                let composing = this.editor.update(cx, |state, cx| {
                                    state.marked_text_range(window, cx).is_some()
                                });
                                if request_live
                                    && !composing
                                    && this.editor.focus_handle(cx).is_focused(window)
                                    && this.editor.read(cx).value().as_ref() == text
                                    && this.editor.read(cx).cursor() == cursor
                                {
                                    let candidates = match candidates {
                                        Ok(candidates) => candidates,
                                        Err(error) => {
                                            this.notice = Some(format!("Completion failed: {error}. Native terminal Tab remains available."));
                                            cx.notify();
                                            return;
                                        }
                                    };
                                    this.completion_text = text;
                                    this.completion_cursor = cursor;
                                    this.completion_bounds = vec![None; candidates.len()];
                                    this.candidates = candidates;
                                    this.completion_index = 0;
                                    this.completion_scroll.scroll_to_item(0);
                                    this.notice = Some(if this.candidates.is_empty() {
                                        "No completion matches from this source. Native terminal Tab remains available."
                                            .into()
                                    } else {
                                        format!("{} · Left/Right or Tab/Shift+Tab cycle · Up/Down change row · Enter accepts · Escape dismisses.", this.completion_source)
                                    });
                                    cx.notify();
                                }
                            }
                            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                                this.completion_rx = None;
                                this.notice =
                                    Some("Completion worker stopped; the draft was not changed.".into());
                                cx.notify();
                            }
                            _ => {}
                        }
                        this.pump_session(cx);
                        if !this.focus.is_focused(window) {
                            this.session.terminal_selection_focus_lost();
                        }
                        if this.settings.animate_logo && this.poll_composer_logo() {
                            cx.notify();
                        }
                        if this.settings.animate_logo && this.logo_warmed_frames < LOGO_FRAME_COUNT {
                            let color = cx.theme().foreground;
                            for _ in 0..6 {
                                let frame = this.logo_warmed_frames;
                                if frame >= LOGO_FRAME_COUNT {
                                    break;
                                }
                                warm_frame(frame, color, window, cx);
                                this.logo_warmed_frames += 1;
                            }
                        }
                    })
                    .is_err()
                })
                .unwrap_or(true);
            if disconnected {
                break;
            }
        });
        let show_blocks = settings.show_blocks;
        Self {
            session,
            document,
            shell_metadata: ShellMetadata::default(),
            input_context: InputContext::default(),
            shell,
            keymap,
            settings,
            show_blocks,
            terminal_bounds: None,
            terminal_scroll_remainder: 0.,
            terminal_mouse_scroll_remainder: 0.,
            terminal_gesture: None,
            terminal_gesture_bounds: None,
            timeline_track_bounds: None,
            timeline_hovered: false,
            prompt_line: PromptLineTracker::default(),
            pending_run: None,
            composer_enter_down: false,
            terminal_composition,
            completion_rx: None,
            completion_generation: 0,
            completion_invalidated: false,
            candidates: Vec::new(),
            completion_index: 0,
            completion_scroll: ScrollHandle::new(),
            completion_text: String::new(),
            completion_cursor: 0,
            completion_context: 0,
            completion_providers,
            completion_source: String::new(),
            completion_bounds: Vec::new(),
            editor,
            reverse_search,
            document_ui,
            document_scroll: ScrollHandle::new(),
            focus,
            notice,
            warning: None,
            composer_logo_gen: 0,
            composer_logo_active: false,
            composer_logo_again: false,
            composer_logo_deadline: None,
            logo_warmed_frames: 0,
            _composer_change: composer_change,
            _composer_keys: composer_keys,
            _pump: pump,
            _appearance: appearance,
            _filter_change: filter_change,
            _focus_lost: focus_lost,
            _memory_changed: memory_changed,
        }
    }

    pub(super) fn result(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        self.notice = result.err().map(|error| error.to_string());
        cx.notify();
    }

    pub(super) fn capture_draft_history_warning(&mut self, cx: &mut Context<Self>) {
        if let Some(warning) = command_editor::take_history_warning(cx) {
            self.warning = Some(warning);
            cx.notify();
        }
    }

    pub(super) fn save_session(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.session.persistence_path() {
            self.notice = Some(format!("Session is saving to {}", path.display()));
            cx.notify();
            return;
        }
        let result = kea_app::history::session_files::new_recording_path().and_then(|path| {
            self.session.start_persistence(&path)?;
            self.notice = Some(format!("Saving session locally: {}", path.display()));
            Ok(())
        });
        self.result(result, cx);
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_run = None;
        settings_window::open(cx.entity(), window, cx);
    }

    pub(super) fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }

    pub(super) fn observe_composer(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.clone();
        self._composer_change =
            cx.subscribe(
                &editor,
                |this, _, event: &edit::InputEvent, cx| match event {
                    edit::InputEvent::Change => {
                        this.pending_run = None;
                        this.dismiss_completion();
                        this.note_composer_typed(cx);
                    }
                    edit::InputEvent::Blur => {
                        this.composer_enter_down = false;
                        this.pending_run = None;
                        this.dismiss_completion();
                        cx.notify();
                    }
                    _ => {}
                },
            );
    }

    fn note_composer_typed(&mut self, cx: &mut Context<Self>) {
        if !self.settings.animate_logo {
            return;
        }
        if self.composer_logo_active {
            self.composer_logo_again = true;
            return;
        }
        self.composer_logo_active = true;
        self.composer_logo_again = false;
        self.composer_logo_gen += 1;
        self.composer_logo_deadline = Some(Instant::now() + KEA_LOGO_PECK_DURATION);
        cx.notify();
    }

    fn poll_composer_logo(&mut self) -> bool {
        if !self.composer_logo_active {
            return false;
        }
        let finished = self
            .composer_logo_deadline
            .is_some_and(|deadline| Instant::now() >= deadline);
        if !finished {
            return false;
        }
        if self.composer_logo_again {
            self.composer_logo_again = false;
            self.composer_logo_gen += 1;
            self.composer_logo_deadline = Some(Instant::now() + KEA_LOGO_PECK_DURATION);
        } else {
            self.composer_logo_active = false;
            self.composer_logo_deadline = None;
        }
        true
    }

    pub(super) fn composer_logo(&self) -> AnyElement {
        if self.settings.animate_logo && self.composer_logo_active {
            KeaLogoAnimation::new(("composer-logo", self.composer_logo_gen))
                .size(px(24.))
                .into_any_element()
        } else {
            KeaLogo::new().size(px(24.)).into_any_element()
        }
    }

    pub(super) fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
    }

    pub(super) fn pump_session(&mut self, cx: &mut Context<Self>) -> bool {
        let previous_context = self.input_context.generation();
        let was_prompt_ready = self.input_context.ready();
        let pump = self.session.pump_observed();
        let mut changed = false;
        for event in pump.observed {
            changed |= match event {
                Observed::Output { at, bytes } => {
                    self.shell_metadata.ingest(&bytes);
                    self.input_context.ingest(&bytes);
                    self.document.ingest_output(at, &bytes)
                }
                Observed::Exit { at, .. } => {
                    self.input_context.invalidate();
                    self.document.finish(at)
                }
            };
        }
        if previous_context != self.input_context.generation() {
            self.pending_run = None;
            self.dismiss_completion();
            changed = true;
        }
        let prompt_arrived = !was_prompt_ready && self.input_context.ready();
        if prompt_arrived {
            self.prompt_line.note_prompt();
        }
        if !self.session.is_running() && self.pending_run.take().is_some() {
            self.notice = Some(
                "The terminal process ended before the shell prompt returned; the draft was not run."
                    .into(),
            );
            changed = true;
        }
        if changed {
            self.document_ui.dirty = true;
        }
        if pump.changed || changed {
            cx.notify();
        }
        prompt_arrived
    }
}
