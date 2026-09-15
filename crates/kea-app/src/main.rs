#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod document_view;
mod terminal_input;

use anyhow::{Context as _, Result};
use gpui::{prelude::*, *};
use gpui_component::{
    input::{self as edit, Input, InputState},
    ActiveTheme, Root, Theme, ThemeMode,
};
use kea_alacritty::Screen;
use kea_app::{
    command_editor, completion, input,
    keybindings::{Action, Invoke, Keymap},
    settings::{Appearance, Settings},
    shell::ShellFlavor,
};
use kea_document::Document;
use kea_session::{Observed, Session};
use std::{ffi::OsString, fs::File, path::PathBuf, time::Duration};

const CELL_WIDTH: f32 = 9.0;
const LINE_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitialFocus {
    Editor,
    Terminal,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kea: {error:#}");
        #[cfg(windows)]
        show_message(format!("Kea could not start:\n\n{error:#}"));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut demo, mut replay, mut record, mut direct) = (false, None, None, false);
    let mut command: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--" {
            command.extend(args);
            break;
        }
        if arg == "--demo" {
            demo = true;
        } else if arg == "--direct" {
            direct = true;
        } else if arg == "--replay" {
            replay = Some(PathBuf::from(args.next().context("--replay needs a path")?));
        } else if arg == "--record" {
            record = Some(PathBuf::from(
                args.next().context("--record needs a new file path")?,
            ));
        } else if arg == "--help" || arg == "-h" {
            let help = "Kea — one session, terminal and editor together

kea [--direct] [--record NEW.kea] [-- PROGRAM ARG...]
kea --replay SESSION.kea
kea --demo

Click the terminal: keys belong to the child. Click the editor: normal text editing.
Editor Enter = newline; Ctrl+Enter = Run in shell; Ctrl+Shift+Enter = Send to app.
Tab in the editor suggests local paths/history; terminal Tab goes to the child.
Blocks are optional (show_blocks = false). --direct changes initial focus only.
Kea shortcuts are scoped to text components, not a live terminal.
Use the toolbar for history, clipboard and focus while the terminal owns the keys.

KEA_KEYBINDINGS and KEA_SETTINGS select explicit configuration files.
Recording is opt-in, unencrypted, bounded and create-only. Commands and output can contain secrets.";
            #[cfg(not(windows))]
            println!("{help}");
            #[cfg(windows)]
            show_message(help.to_string());
            return Ok(());
        } else if arg.to_string_lossy().starts_with('-') {
            anyhow::bail!("unknown option: {}", arg.to_string_lossy());
        } else {
            command.push(arg);
            command.extend(args);
            break;
        }
    }
    let replay_requested = replay.is_some();
    if (demo && replay_requested)
        || ((demo || replay_requested) && (record.is_some() || !command.is_empty()))
    {
        anyhow::bail!(
            "--demo and --replay cannot be combined with each other, --record, or a command"
        );
    }
    #[cfg(windows)]
    if !demo && !replay_requested && command.is_empty() {
        command = vec!["powershell.exe".into(), "-NoLogo".into(), "-NoExit".into()];
    }
    let shell = if demo || replay_requested {
        None
    } else {
        ShellFlavor::detect(&command)
    };
    let mut session = if demo {
        Session::demo()?
    } else if let Some(path) = replay {
        let loaded = kea_core::read_from(File::open(path)?)?;
        let mut session = Session::from_recording(loaded.recording)?;
        session.go_live();
        if loaded.truncated_tail {
            session.warning =
                Some("Recovered complete events; the final recording frame was truncated.".into());
        }
        session
    } else {
        Session::spawn(&command, kea_core::Size::new(100, 26)?, record.as_deref())?
    };
    if let Some(shell) = shell {
        session.send_hidden(shell.integration(&command))?;
    }
    let document = Document::from_recording(session.recording());
    let mode = if demo {
        InitialFocus::Terminal
    } else if replay_requested && !document.blocks().is_empty() {
        InitialFocus::Editor
    } else if direct || shell.is_none() {
        InitialFocus::Terminal
    } else {
        InitialFocus::Editor
    };
    let (keymap, warning) = Keymap::load();
    let (settings, settings_warning) = Settings::load();
    let mut warnings: Vec<String> = warning.into_iter().chain(settings_warning).collect();
    if !demo && !replay_requested && !direct && shell.is_none() {
        warnings.push("No interactive shell integration for this program. Use terminal input or Send to app; the editor remains available.".into());
    }
    let notice = (!warnings.is_empty()).then(|| warnings.join(" "));
    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            command_editor::register_languages();
            keymap.install(cx);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1050.), px(780.)),
                    cx,
                ))),
                window_min_size: Some(size(px(760.), px(500.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Kea".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, |window, cx| {
                apply_appearance(&settings, window, cx);
                let view = cx.new(|cx| {
                    KeaView::new(
                        session, document, shell, keymap, settings, mode, notice, window, cx,
                    )
                });
                let weak = view.downgrade();
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if mode == InitialFocus::Terminal {
                            window.focus(&view.focus);
                        } else {
                            view.focus_active(window, cx);
                        }
                    });
                });
                cx.new(|cx| Root::new(view, window, cx))
            }) {
                eprintln!("kea: cannot open window: {error:#}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}

struct KeaView {
    session: Session,
    document: Document,
    shell: Option<ShellFlavor>,
    keymap: Keymap,
    settings: Settings,
    show_blocks: bool,
    terminal_bounds: Option<Bounds<Pixels>>,
    terminal_composition: Entity<InputState>,
    completion_rx: Option<std::sync::mpsc::Receiver<(String, usize, Vec<completion::Candidate>)>>,
    candidates: Vec<completion::Candidate>,
    completion_text: String,
    completion_cursor: usize,
    editor: Entity<InputState>,
    document_ui: document_view::DocumentUi,
    document_scroll: ScrollHandle,
    focus: FocusHandle,
    notice: Option<String>,
    _pump: Task<()>,
    _appearance: Subscription,
    _filter_change: Subscription,
}

impl KeaView {
    #[allow(clippy::too_many_arguments)]
    fn new(
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
        let focus = cx.focus_handle();
        let start_at_terminal = initial_focus == InitialFocus::Terminal;
        let editor = command_editor::new_draft(shell, &settings, "", window, cx);
        if start_at_terminal {
            window.focus(&focus);
        } else {
            editor.update(cx, |state, cx| state.focus(window, cx));
        }
        let terminal_composition = cx.new(|cx| InputState::new(window, cx));
        let document_ui = document_view::DocumentUi::new(window, cx);
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
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            apply_appearance(&this.settings, window, cx);
            cx.notify();
        });
        let pump = cx.spawn(async move |this, cx| loop {
            Timer::after(Duration::from_millis(16)).await;
            if this
                .update(cx, |this, cx| {
                    let completion = this.completion_rx.as_ref().map(|rx| rx.try_recv());
                    match completion {
                        Some(Ok((text, cursor, candidates))) => {
                            this.completion_rx = None;
                            if this.editor.read(cx).value().as_ref() == text && this.editor.read(cx).cursor() == cursor {
                                this.completion_text = text;
                                this.completion_cursor = cursor;
                                this.candidates = candidates;
                                this.notice = Some(if this.candidates.is_empty() { "No local path/history matches. Native shell Tab is available in terminal focus.".into() } else { "Choose a completion below, or press Tab again to accept the first. Nothing is executed.".into() });
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
                    let pump = this.session.pump_observed();
                    let mut changed = false;
                    for event in pump.observed {
                        changed |= match event {
                            Observed::Output { at, bytes } => {
                                this.document.ingest_output(at, &bytes)
                            }
                            Observed::Exit { at, .. } => this.document.abort_in_flight(at),
                        };
                    }
                    if changed {
                        this.document_ui.dirty = true;
                    }
                    // Never reset a reader's selection or scroll on each output chunk.
                    if pump.changed || changed {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        });
        Self {
            session,
            document,
            shell,
            keymap,
            show_blocks: settings.show_blocks,
            settings,
            terminal_bounds: None,
            terminal_composition,
            completion_rx: None,
            candidates: Vec::new(),
            completion_text: String::new(),
            completion_cursor: 0,
            editor,
            document_ui,
            document_scroll: ScrollHandle::new(),
            focus,
            notice,
            _pump: pump,
            _appearance: appearance,
            _filter_change: filter_change,
        }
    }

    fn result(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        self.notice = result.err().map(|error| error.to_string());
        cx.notify();
    }
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }
    fn copy_document(&self, cx: &mut App) {
        let text = if self.show_blocks && !self.session.is_history() {
            self.document.text()
        } else {
            self.session.screen().text()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
    fn paste_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let result = input::paste(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.session.send(bytes));
        if result.is_ok() {
            self.document.note_terminal_input();
        }
        self.result(result, cx);
    }
    fn execute_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.document.prompt_ready() {
            self.result(Err(anyhow::anyhow!("The shell has not reported an idle prompt. Use Send to app, or finish/cancel the command in the terminal.")), cx);
            return;
        }
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        if !self.session.input_allowed() {
            self.result(
                Err(anyhow::anyhow!("Return to LIVE before sending input.")),
                cx,
            );
            return;
        }
        if let Err(error) = self.document.validate_submission(&text) {
            self.result(Err(error.into()), cx);
            return;
        }
        let Some(shell) = self.shell else {
            self.result(
                Err(anyhow::anyhow!(
                    "Run in shell requires an integrated shell; use Send to app instead."
                )),
                cx,
            );
            return;
        };
        let id = self.document.allocate_id();
        let wrapper = match shell.wrap(id, &text) {
            Ok(wrapper) => wrapper,
            Err(error) => {
                self.result(Err(error.into()), cx);
                return;
            }
        };
        let at = self.session.elapsed_micros();
        let result = self.session.send_hidden(wrapper).and_then(|_| {
            self.document
                .queue_local(id, text.clone(), at)
                .map_err(anyhow::Error::from)
        });
        if result.is_ok() {
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            window.focus(&self.focus);
            self.candidates.clear();
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
            self.document_scroll.scroll_to_bottom();
        }
        self.result(result, cx);
    }
    // Legacy configuration name: this changes only the optional inspector.
    fn toggle_direct(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.show_blocks = !self.show_blocks;
        self.document_ui.page_start = None;
        self.document_scroll.scroll_to_bottom();
        cx.notify();
    }

    fn send_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
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
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.candidates.clear();
            window.focus(&self.focus);
        }
        self.result(result, cx);
    }

    fn complete_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let directory = self
            .document
            .prompt_ready()
            .then(|| self.document.directory().map(PathBuf::from))
            .flatten();
        let history = self
            .document
            .blocks()
            .iter()
            .rev()
            .take(200)
            .map(|b| b.input.clone())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        let shell = self.shell;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.completion_rx = Some(rx);
        self.candidates.clear();
        std::thread::spawn(move || {
            let candidates =
                completion::suggest(&text, cursor, directory.as_deref(), &history, shell);
            let _ = tx.send((text, cursor, candidates));
        });
    }

    fn apply_completion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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
    fn seek_x(&mut self, x: Pixels, width: f32, window: &mut Window, cx: &mut Context<Self>) {
        let fraction = ((f32::from(x) - 12.) / width).clamp(0., 1.);
        let at = (fraction as f64 * self.session.recording().duration() as f64) as u64;
        let result = self.session.seek_time(at);
        window.focus(&self.focus);
        self.result(result, cx);
    }
    fn invoke(&mut self, event: &Invoke, window: &mut Window, cx: &mut Context<Self>) {
        let terminal_focused = self.focus.is_focused(window);
        match event.action {
            Action::Copy if terminal_focused => {
                self.notice = Some("Direct PTY has no selection yet. Use Copy document/screen for an explicit whole-screen copy.".into());
                cx.notify();
            }
            Action::Paste if terminal_focused => self.paste_terminal(cx),
            Action::Copy => window.dispatch_action(Box::new(edit::Copy), cx),
            Action::Cut => window.dispatch_action(Box::new(edit::Cut), cx),
            Action::Paste => window.dispatch_action(Box::new(edit::Paste), cx),
            Action::Undo => window.dispatch_action(Box::new(edit::Undo), cx),
            Action::Redo => window.dispatch_action(Box::new(edit::Redo), cx),
            Action::SelectAll => window.dispatch_action(Box::new(edit::SelectAll), cx),
            Action::Find => window.dispatch_action(Box::new(edit::Search), cx),
            Action::CopyDocument => self.copy_document(cx),
            Action::FocusEditor => self.focus_active(window, cx),
            Action::Interrupt => {
                let result = self.session.send(vec![3]);
                self.result(result, cx);
            }
            Action::Execute => self.execute_editor(window, cx),
            Action::SendText => self.send_editor(window, cx),
            Action::Complete => self.complete_editor(window, cx),
            Action::ToggleDirect => self.toggle_direct(window, cx),
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
                self.notice = None;
                self.focus_active(window, cx);
                cx.notify();
            }
            Action::Quit => cx.quit(),
        }
    }
    // Text components receive platform text input themselves. This fallback belongs
    // only to the compatibility terminal, never to the command editor.
    fn terminal_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_composition.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        if let Some(bytes) = input::encode(&event.keystroke, self.session.application_cursor()) {
            if self.session.input_allowed() {
                let result = self.session.send(bytes);
                if result.is_ok() {
                    self.document.note_terminal_input();
                }
                self.result(result, cx);
            }
            cx.stop_propagation();
        }
        // Unencoded text continues to GPUI's platform composition/replacement handler.
    }
    fn control(
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
}

impl Render for KeaView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let duration = self.session.recording().duration();
        let position = self.session.position();
        let fraction = if duration == 0 {
            0.
        } else {
            position as f32 / duration as f32
        };
        let screen = self.session.screen();
        let weak = cx.entity().downgrade();
        let terminal = div()
            .id("terminal-surface")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .key_context(if self.session.is_running() {
                "KeaTerminal"
            } else {
                "KeaChrome"
            })
            .track_focus(&self.focus)
            .bg(rgb(0x11151a))
            .on_key_down(cx.listener(Self::terminal_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| window.focus(&this.focus)),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            let resized = this.terminal_bounds != Some(bounds);
                            this.terminal_bounds = Some(bounds);
                            if resized {
                                let weak = cx.entity().downgrade();
                                window.defer(cx, move |_, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Ok(size) = kea_core::Size::new(
                                            (f32::from(bounds.size.width) / CELL_WIDTH)
                                                .floor()
                                                .clamp(2., 512.)
                                                as u16,
                                            (f32::from(bounds.size.height) / LINE_HEIGHT)
                                                .floor()
                                                .clamp(1., 256.)
                                                as u16,
                                        ) {
                                            let result = this.session.resize(size);
                                            this.result(result, cx);
                                        }
                                    });
                                });
                            }
                        });
                    },
                    {
                        let focus = self.focus.clone();
                        let entity = cx.entity();
                        move |bounds, _, window, cx| {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, entity.clone()),
                                cx,
                            );
                            paint_screen(&screen, bounds, window, cx);
                        }
                    },
                )
                .size_full(),
            );
        let mut output = div()
            .flex()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(terminal);
        if self.show_blocks {
            let inspector = self.render_document(window, cx);
            output = output.child(
                div()
                    .id("block-inspector")
                    .w(px(450.))
                    .h_full()
                    .flex_shrink_0()
                    .child(inspector),
            );
        }
        let mut completions = div().flex().flex_wrap().gap_1();
        if self.editor.read(cx).value().as_ref() == self.completion_text {
            for (index, candidate) in self.candidates.iter().enumerate() {
                let label: String = candidate.label.chars().take(100).collect();
                completions = completions.child(
                    div()
                        .id(("completion", index))
                        .px_2()
                        .border_1()
                        .border_color(cx.theme().border)
                        .cursor_pointer()
                        .child(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.apply_completion(index, window, cx)
                        })),
                );
            }
        }
        let status = self.notice.clone().or_else(|| self.session.warning.clone()).unwrap_or_else(|| {
            if self.session.is_history() { "History is read-only; the live process continues.".into() }
            else if self.focus.is_focused(window) { "Terminal focus: keys go to the application. Click the editor to compose text.".into() }
            else { "Editor focus: Enter adds a newline; Tab suggests local paths/history. Run and Send are explicit actions.".into() }
        });
        let directory = self
            .document
            .directory()
            .map(|d| format!("Shell directory: {d}"))
            .unwrap_or_else(|| "Shell directory: not reported".into());
        div()
            .id("kea")
            .size_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .key_context("Kea")
            .on_action(cx.listener(Self::invoke))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(cx.theme().mono_font_size)
            .font_family(cx.theme().mono_font_family.clone())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(div().font_weight(FontWeight::BOLD).child("Kea"))
                    .child(self.control("previous", "Previous", Action::PreviousEvent, cx))
                    .child(self.control("next", "Next", Action::NextEvent, cx))
                    .child(self.control(
                        "play",
                        if self.session.is_playing() {
                            "Pause"
                        } else {
                            "Play"
                        },
                        Action::PlayPause,
                        cx,
                    ))
                    .child(self.control("live", "Live", Action::GoLive, cx))
                    .child(self.control(
                        "mode",
                        if self.show_blocks {
                            "Hide blocks"
                        } else {
                            "Show blocks"
                        },
                        Action::ToggleDirect,
                        cx,
                    ))
                    .child(self.control("copy-document", "Copy view", Action::CopyDocument, cx))
                    .child(
                        button("paste-terminal", "Paste to app")
                            .on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))),
                    )
                    .child(button("focus-input", "Focus editor").on_click(
                        cx.listener(|this, _, window, cx| this.focus_active(window, cx)),
                    )),
            )
            .child(
                div()
                    .h(px(20.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(directory),
            )
            .child(output)
            .child(
                div()
                    .id("command-editor")
                    .key_context("KeaCommand")
                    .h(px(195.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(self.control(
                                "run-draft",
                                format!("Run in shell · {}", self.keymap.label(Action::Execute)),
                                Action::Execute,
                                cx,
                            ))
                            .child(self.control(
                                "send-draft",
                                format!("Send to app · {}", self.keymap.label(Action::SendText)),
                                Action::SendText,
                                cx,
                            ))
                            .child(if self.document.prompt_ready() {
                                "Shell prompt ready"
                            } else {
                                "Application input / waiting for prompt"
                            }),
                    )
                    .child(
                        Input::new(&self.editor)
                            .h(px(100.))
                            .appearance(false)
                            .bordered(false),
                    )
                    .child(
                        div()
                            .id("completion-list")
                            .flex_1()
                            .overflow_y_scroll()
                            .child(completions),
                    ),
            )
            .child(
                div()
                    .h(px(20.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(status),
            )
            .child(
                div()
                    .id("timeline")
                    .h(px(12.))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            let width = f32::from(window.viewport_size().width) - 24.;
                            this.seek_x(event.position.x, width, window, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            let width = f32::from(window.viewport_size().width) - 24.;
                            this.seek_x(event.position.x, width, window, cx);
                        }
                    }))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                window.paint_quad(fill(bounds, rgb(0x687380)));
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin,
                                        size(bounds.size.width * fraction, bounds.size.height),
                                    ),
                                    rgb(0x61afef),
                                ));
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(div().h(px(20.)).flex_shrink_0().child(format!(
                "{} commands · {:.3}s / {:.3}s · {}",
                self.document.blocks().len(),
                position as f64 / 1_000_000.,
                duration as f64 / 1_000_000.,
                if self.session.is_running() {
                    "process running"
                } else {
                    "ended / recording"
                }
            )))
    }
}

fn button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x687380))
        .cursor_pointer()
        .child(label.into())
}
fn apply_appearance(settings: &Settings, window: &mut Window, cx: &mut App) {
    match settings.appearance {
        Appearance::System => Theme::sync_system_appearance(Some(window), cx),
        Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    let theme = Theme::global_mut(cx);
    if let Some(family) = &settings.font_family {
        theme.mono_font_family = family.clone().into();
    }
    if let Some(size) = settings.font_size {
        theme.mono_font_size = px(size);
        theme.font_size = px(size);
    }
}

fn paint_screen(screen: &Screen, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    for (index, cell) in screen.cells.iter().enumerate() {
        let row = index / usize::from(screen.size.columns);
        let column = index % usize::from(screen.size.columns);
        let origin =
            bounds.origin + point(px(column as f32 * CELL_WIDTH), px(row as f32 * LINE_HEIGHT));
        if origin.x >= bounds.right() || origin.y >= bounds.bottom() {
            continue;
        }
        if cell.background != 0x11151a {
            window.paint_quad(fill(
                Bounds::new(origin, size(px(CELL_WIDTH), px(LINE_HEIGHT))),
                rgb(cell.background),
            ));
        }
        if cell.spacer || cell.text == " " {
            continue;
        }
        let mut cell_font = font("monospace");
        if cell.bold {
            cell_font.weight = FontWeight::BOLD;
        }
        let run = TextRun {
            len: cell.text.len(),
            font: cell_font,
            color: rgb(cell.foreground).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped =
            window
                .text_system()
                .shape_line(cell.text.clone().into(), px(14.), &[run], None);
        let _ = shaped.paint(origin, px(LINE_HEIGHT), window, cx);
        if cell.underline {
            window.paint_quad(fill(
                Bounds::new(
                    origin + point(px(0.), px(LINE_HEIGHT - 2.)),
                    size(
                        px(if cell.wide {
                            CELL_WIDTH * 2.
                        } else {
                            CELL_WIDTH
                        }),
                        px(1.),
                    ),
                ),
                rgb(cell.foreground),
            ));
        }
    }
    if let Some((row, column)) = screen.cursor {
        let origin = bounds.origin
            + point(
                px(column as f32 * CELL_WIDTH),
                px(row as f32 * LINE_HEIGHT + LINE_HEIGHT - 2.),
            );
        if origin.x < bounds.right() && origin.y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(origin, size(px(CELL_WIDTH), px(2.))),
                rgb(0x61afef),
            ));
        }
    }
}

#[cfg(windows)]
fn show_message(message: String) {
    struct Message(String);
    impl Render for Message {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .p_4()
                .bg(rgb(0x171b20))
                .text_color(rgb(0xd7dae0))
                .child(self.0.clone())
        }
    }
    Application::new().run(move |cx| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let _ = cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| Message(message))
        });
        cx.activate(true);
    });
}
