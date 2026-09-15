"""Apply the reviewed unified-surface change to the pinned host source once."""
from pathlib import Path
import hashlib
p=Path('crates/kea-app/src/main.rs')
raw=p.read_bytes()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='17e275fcf806f7cf39271659f7d23b0bc9556198'
s=raw.decode()
s='#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]\n\n'+s
s=s.replace('mod document_view;', 'mod document_view;\nmod terminal_input;')
s=s.replace('    command_editor, input,','    command_editor, completion, input,')
s=s.replace('enum InputMode {\n    Document,\n    Direct,\n}', 'enum InitialFocus {\n    Editor,\n    Terminal,\n}')
s=s.replace('InputMode', 'InitialFocus').replace('InitialFocus::Document','InitialFocus::Editor').replace('InitialFocus::Direct','InitialFocus::Terminal')
s=s.replace('        eprintln!("kea: {error:#}");', '        eprintln!("kea: {error:#}");\n        #[cfg(windows)]\n        show_message(format!("Kea could not start:\\n\\n{error:#}"));',1)
s=s.replace('&& !direct && command.is_empty()', '&& command.is_empty()',1).replace('            "-NoProfile".into(),\n','',1)
a=s.index('            println!("Kea —');b=s.index('            return Ok(());',a)
s=s[:a]+'''            let help = "Kea — one session, terminal and editor together\n\nkea [--direct] [--record NEW.kea] [-- PROGRAM ARG...]\nkea --replay SESSION.kea\nkea --demo\n\nClick the terminal: keys belong to the child. Click the editor: normal text editing.\nEditor Enter = newline; Ctrl+Enter = Run in shell; Ctrl+Shift+Enter = Send to app.\nTab in the editor suggests local paths/history; terminal Tab goes to the child.\nBlocks are optional (show_blocks = false). --direct changes initial focus only.\nKea shortcuts are scoped to text components, not a live terminal.\nUse the toolbar for history, clipboard and focus while the terminal owns the keys.\n\nKEA_KEYBINDINGS and KEA_SETTINGS select explicit configuration files.\nRecording is opt-in, unencrypted, bounded and create-only. Commands and output can contain secrets.";
            #[cfg(not(windows))]
            println!("{help}");
            #[cfg(windows)]
            show_message(help.to_string());
'''+s[b:]
s=s.replace('    let session = if demo {', '    let mut session = if demo {',1)
s=s.replace('    let document = Document::from_recording(session.recording());','''    if let Some(shell) = shell {
        session.send_hidden(shell.integration(&command))?;
    }
    let document = Document::from_recording(session.recording());''',1)
s=s.replace('Document mode is unavailable for this program. Using the same process in Direct PTY mode.','No interactive shell integration for this program. Use terminal input or Send to app; the editor remains available.')
s=s.replace('    input_mode: InitialFocus,','''    show_blocks: bool,
    terminal_bounds: Option<Bounds<Pixels>>,
    terminal_composition: Entity<InputState>,
    completion_rx: Option<std::sync::mpsc::Receiver<(String, usize, Vec<completion::Candidate>)>>,
    candidates: Vec<completion::Candidate>,
    completion_text: String,
    completion_cursor: usize,''',1)
s=s.replace('        input_mode: InitialFocus,','        initial_focus: InitialFocus,')
s=s.replace('            input_mode,','''            show_blocks: settings.show_blocks,
            terminal_bounds: None,
            terminal_composition,
            completion_rx: None,
            candidates: Vec::new(),
            completion_text: String::new(),
            completion_cursor: 0,''')
s=s.replace('            settings,\n            show_blocks: settings.show_blocks,', '            show_blocks: settings.show_blocks,\n            settings,')
s=s.replace('        let focus = cx.focus_handle();','        let focus = cx.focus_handle();\n        let start_at_terminal = initial_focus == InitialFocus::Terminal;',1)
s=s.replace('        let document_ui = document_view::DocumentUi::new(window, cx);','''        if start_at_terminal { window.focus(&focus); }
        else { editor.update(cx, |state, cx| state.focus(window, cx)); }
        let terminal_composition = cx.new(|cx| InputState::new(window, cx));
        let document_ui = document_view::DocumentUi::new(window, cx);''',1)
s=s.replace('let _ = weak.update(cx, |view, cx| view.focus_active(window, cx));','''let _ = weak.update(cx, |view, cx| {
                        if mode == InitialFocus::Terminal { window.focus(&view.focus); }
                        else { view.focus_active(window, cx); }
                    });''',1)
s=s.replace('                    let pump = this.session.pump_observed();','''                    let completion = this.completion_rx.as_ref().map(|rx| rx.try_recv());
                    match completion {
                        Some(Ok((text, cursor, candidates))) => {
                            this.completion_rx = None;
                            if this.editor.read(cx).value().as_ref() == text && this.editor.read(cx).cursor() == cursor {
                                this.completion_text = text;
                                this.completion_cursor = cursor;
                                this.candidates = candidates;
                                this.notice = Some(if this.candidates.is_empty() { "No local path/history matches. Native shell Tab is available in terminal focus.".into() } else { "Choose a completion below. Nothing is executed.".into() });
                                cx.notify();
                            }
                        }
                        Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                            this.completion_rx = None;
                            this.notice = Some("Completion worker stopped; the draft was not changed.".into());
                            cx.notify();
                        }
                        _ => {}
                    }
                    let pump = this.session.pump_observed();''')
a=s.index('    fn focus_active(');b=s.index('    fn copy_document',a)
s=s[:a]+'''    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }
'''+s[b:]
s=s.replace('if self.input_mode == InitialFocus::Editor && !self.session.is_history()', 'if self.show_blocks && !self.session.is_history()')
s=s.replace('''        if self.input_mode != InitialFocus::Editor {
            return;
        }
''','''        if !self.document.prompt_ready() {
            self.result(Err(anyhow::anyhow!("The shell has not reported an idle prompt. Use Send to app, or finish/cancel the command in the terminal.")), cx);
            return;
        }
''',1)
s=s.replace('            self.focus_active(window, cx);\n            self.document_ui.page_start', '            window.focus(&self.focus);\n            self.candidates.clear();\n            self.document_ui.page_start',1)
a=s.index('    fn toggle_direct(');b=s.index('    fn seek_x(',a)
s=s[:a]+r'''    // Legacy configuration name: this changes only the optional inspector.
    fn toggle_direct(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.show_blocks = !self.show_blocks;
        self.document_ui.page_start = None;
        self.document_scroll.scroll_to_bottom();
        cx.notify();
    }

    fn send_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else { return; };
        if text.is_empty() { return; }
        let result = input::paste(&text, self.session.bracketed_paste()).and_then(|mut bytes| {
            bytes.push(b'\r');
            self.session.send(bytes)
        });
        if result.is_ok() {
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.candidates.clear();
            window.focus(&self.focus);
        }
        self.result(result, cx);
    }

    fn complete_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else { return; };
        let cursor = self.editor.read(cx).cursor();
        let line_prefix = text[..cursor].rsplit('\n').next().unwrap_or("");
        if line_prefix.trim().is_empty() {
            window.dispatch_action(Box::new(edit::Indent), cx);
            return;
        }
        if self.completion_rx.is_some() { return; }
        let directory = self.document.prompt_ready().then(|| self.document.directory().map(PathBuf::from)).flatten();
        let history = self.document.blocks().iter().rev().take(200).map(|b| b.input.clone()).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>();
        let shell = self.shell;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.completion_rx = Some(rx);
        self.candidates.clear();
        std::thread::spawn(move || {
            let candidates = completion::suggest(&text, cursor, directory.as_deref(), &history, shell);
            let _ = tx.send((text, cursor, candidates));
        });
    }

    fn apply_completion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(candidate) = self.candidates.get(index).cloned() else { return; };
        let valid = self.editor.read(cx).value().as_ref() == self.completion_text
            && self.editor.read(cx).cursor() == self.completion_cursor;
        if valid {
            let range = self.completion_text[..candidate.range.start].encode_utf16().count()
                ..self.completion_text[..candidate.range.end].encode_utf16().count();
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
'''+s[b:]
s=s.replace('            Action::Execute => self.execute_editor(window, cx),','            Action::Execute => self.execute_editor(window, cx),\n            Action::SendText => self.send_editor(window, cx),\n            Action::Complete => self.complete_editor(window, cx),')
a=s.index('    fn terminal_key(');b=s.index('    fn control(',a)
s=s[:a]+'''    fn terminal_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_composition.update(cx, |state, cx| state.marked_text_range(window, cx).is_some()) { return; }
        if let Some(bytes) = input::encode(&event.keystroke, self.session.application_cursor()) {
            if self.session.input_allowed() {
                let result = self.session.send(bytes);
                if result.is_ok() { self.document.note_terminal_input(); }
                self.result(result, cx);
            }
            cx.stop_propagation();
        }
        // Unencoded text continues to GPUI's platform composition/replacement handler.
    }
'''+s[b:]
a=s.index('impl Render for KeaView');b=s.index('\nfn button(',a)
s=s[:a]+r'''impl Render for KeaView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let duration = self.session.recording().duration();
        let position = self.session.position();
        let fraction = if duration == 0 { 0. } else { position as f32 / duration as f32 };
        let screen = self.session.screen();
        let weak = cx.entity().downgrade();
        let terminal = div()
            .id("terminal-surface")
            .flex_1().min_w_0().h_full().overflow_hidden()
            .key_context(if self.session.is_running() { "KeaTerminal" } else { "KeaChrome" })
            .track_focus(&self.focus)
            .bg(rgb(0x11151a))
            .on_key_down(cx.listener(Self::terminal_key))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, _| window.focus(&this.focus)))
            .child(canvas(
                move |bounds, window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        let resized = this.terminal_bounds != Some(bounds);
                        this.terminal_bounds = Some(bounds);
                        if resized {
                            let weak = cx.entity().downgrade();
                            window.defer(cx, move |_, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if let Ok(size) = kea_core::Size::new(
                                        (f32::from(bounds.size.width) / CELL_WIDTH).floor().clamp(2.,512.) as u16,
                                        (f32::from(bounds.size.height) / LINE_HEIGHT).floor().clamp(1.,256.) as u16,
                                    ) { let result = this.session.resize(size); this.result(result, cx); }
                                });
                            });
                        }
                    });
                },
                {
                    let focus = self.focus.clone();
                    let entity = cx.entity();
                    move |bounds, _, window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
                        paint_screen(&screen, bounds, window, cx);
                    }
                },
            ).size_full());
        let mut output = div().flex().flex_1().min_h_0().overflow_hidden().child(terminal);
        if self.show_blocks {
            let inspector = self.render_document(window, cx);
            output = output.child(div().id("block-inspector").w(px(450.)).h_full().flex_shrink_0().child(inspector));
        }
        let mut completions = div().flex().flex_wrap().gap_1();
        if self.editor.read(cx).value().as_ref() == self.completion_text {
            for (index, candidate) in self.candidates.iter().take(8).enumerate() {
                let label: String = candidate.label.chars().take(100).collect();
                completions = completions.child(div().id(("completion", index)).px_2().border_1()
                    .border_color(cx.theme().border).cursor_pointer().child(label)
                    .on_click(cx.listener(move |this, _, window, cx| this.apply_completion(index, window, cx))));
            }
        }
        let status = self.notice.clone().or_else(|| self.session.warning.clone()).unwrap_or_else(|| {
            if self.session.is_history() { "History is read-only; the live process continues.".into() }
            else if self.focus.is_focused(window) { "Terminal focus: keys go to the application. Click the editor to compose text.".into() }
            else { "Editor focus: Enter adds a newline; Tab suggests local paths/history. Run and Send are explicit actions.".into() }
        });
        let directory = self.document.directory().map(|d| format!("Shell directory: {d}"))
            .unwrap_or_else(|| "Shell directory: not reported".into());
        div().id("kea").size_full().flex().flex_col().p_3().gap_2()
            .key_context("Kea")
            .on_action(cx.listener(Self::invoke))
            .bg(cx.theme().background).text_color(cx.theme().foreground)
            .text_size(cx.theme().mono_font_size).font_family(cx.theme().mono_font_family.clone())
            .child(div().flex().flex_wrap().items_center().gap_2().flex_shrink_0()
                .child(div().font_weight(FontWeight::BOLD).child("Kea"))
                .child(self.control("previous", "Previous", Action::PreviousEvent, cx))
                .child(self.control("next", "Next", Action::NextEvent, cx))
                .child(self.control("play", if self.session.is_playing() { "Pause" } else { "Play" }, Action::PlayPause, cx))
                .child(self.control("live", "Live", Action::GoLive, cx))
                .child(self.control("mode", if self.show_blocks { "Hide blocks" } else { "Show blocks" }, Action::ToggleDirect, cx))
                .child(self.control("copy-document", "Copy view", Action::CopyDocument, cx))
                .child(button("paste-terminal", "Paste to app").on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))))
                .child(button("focus-input", "Focus editor").on_click(cx.listener(|this, _, window, cx| this.focus_active(window,cx)))))
            .child(div().h(px(20.)).flex_shrink_0().overflow_hidden().child(directory))
            .child(output)
            .child(div().id("command-editor").key_context("KeaCommand").h(px(195.)).flex_shrink_0().flex().flex_col().gap_1()
                .p_2().border_1().border_color(cx.theme().border)
                .child(div().flex().gap_2().items_center()
                    .child(self.control("run-draft", format!("Run in shell · {}", self.keymap.label(Action::Execute)), Action::Execute, cx))
                    .child(self.control("send-draft", format!("Send to app · {}", self.keymap.label(Action::SendText)), Action::SendText, cx))
                    .child(if self.document.prompt_ready() { "Shell prompt ready" } else { "Application input / waiting for prompt" }))
                .child(Input::new(&self.editor).h(px(100.)).appearance(false).bordered(false))
                .child(div().flex_1().overflow_y_scroll().id("completion-list").child(completions)))
            .child(div().h(px(20.)).flex_shrink_0().overflow_hidden().child(status))
            .child(div().id("timeline").h(px(12.)).flex_shrink_0().cursor_pointer()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window,cx| {
                    let width = f32::from(window.viewport_size().width)-24.; this.seek_x(event.position.x,width,window,cx);
                }))
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window,cx| {
                    if event.pressed_button == Some(MouseButton::Left) {
                        let width = f32::from(window.viewport_size().width)-24.; this.seek_x(event.position.x,width,window,cx);
                    }
                }))
                .child(canvas(|_,_,_| (), move |bounds,_,window,_| {
                    window.paint_quad(fill(bounds,rgb(0x687380)));
                    window.paint_quad(fill(Bounds::new(bounds.origin,size(bounds.size.width*fraction,bounds.size.height)),rgb(0x61afef)));
                }).size_full()))
            .child(div().h(px(20.)).flex_shrink_0().child(format!("{} commands · {:.3}s / {:.3}s · {}",self.document.blocks().len(),position as f64/1_000_000.,duration as f64/1_000_000.,if self.session.is_running(){"process running"}else{"ended / recording"})))
    }
}
''' + s[b:]
s += r'''
#[cfg(windows)]
fn show_message(message: String) {
    struct Message(String);
    impl Render for Message {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().p_4().bg(rgb(0x171b20)).text_color(rgb(0xd7dae0)).child(self.0.clone())
        }
    }
    Application::new().run(move |cx| {
        cx.on_window_closed(|cx| if cx.windows().is_empty() { cx.quit(); }).detach();
        let _ = cx.open_window(WindowOptions::default(), |_,cx| cx.new(|_| Message(message)));
        cx.activate(true);
    });
}
'''
assert 'input_mode' not in s
p.write_text(s,encoding='utf-8')
