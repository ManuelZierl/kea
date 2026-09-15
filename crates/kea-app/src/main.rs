#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod document_view;
mod shell_metadata;
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
use shell_metadata::ShellMetadata;
use std::{ffi::OsString, fs::File, path::PathBuf, time::Duration};

const TERMINAL_LINE_HEIGHT: f32 = 1.2;
const FALLBACK_CELL_WIDTH_EM: f32 = 0.6;

#[derive(Clone)]
struct TerminalFontMetrics {
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
}

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
    let (mut demo, mut replay, mut record, mut terminal_focus) = (false, None, None, false);
    let mut command: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--" {
            command.extend(args);
            break;
        }
        if arg == "--demo" {
            demo = true;
        } else if arg == "--direct" || arg == "--terminal-focus" {
            // --direct is retained as a compatibility alias; there is no Direct mode.
            terminal_focus = true;
        } else if arg == "--replay" {
            replay = Some(PathBuf::from(args.next().context("--replay needs a path")?));
        } else if arg == "--record" {
            record = Some(PathBuf::from(
                args.next().context("--record needs a new file path")?,
            ));
        } else if arg == "--help" || arg == "-h" {
            let help = "Kea — one terminal session with a persistent text editor

kea [--terminal-focus] [--record NEW.kea] [-- PROGRAM ARG...]
kea --replay SESSION.kea
kea --demo

Terminal and editor are visible together; focus decides who receives keyboard input.
Editor defaults: Enter = newline, Ctrl+Enter = Run in shell, Ctrl+Shift+Enter = Send to app, Tab = complete.
All semantic shortcuts are configurable. Terminal-like editor behavior is possible with:
  run_shell = enter
  newline = shift-enter

Blocks are an optional observational view. They never gate command execution.
The current integrated local-shell directory is reported by shell hooks; while a TUI/remote app owns the terminal, Kea labels it last reported rather than guessing.
Terminal Tab and other representable keys go to the child application.

KEA_KEYBINDINGS and KEA_SETTINGS select explicit configuration files.
Recording is opt-in, bounded and unencrypted; commands/output can contain secrets.";
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
        // Keep the user's profile: shell configuration/completers are part of the
        // environment Kea should preserve, not replace.
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
    let initial_focus = if demo || terminal_focus || shell.is_none() {
        InitialFocus::Terminal
    } else {
        InitialFocus::Editor
    };
    let (keymap, warning) = Keymap::load();
    let (settings, settings_warning) = Settings::load();
    let mut warnings: Vec<String> = warning.into_iter().chain(settings_warning).collect();
    if !demo && !replay_requested && shell.is_none() {
        warnings.push(
            "No integrated local shell detected. Terminal input and Send to app remain available; Run in shell and shell cwd completion are unavailable."
                .into(),
        );
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
                        session,
                        document,
                        shell,
                        keymap,
                        settings,
                        initial_focus,
                        notice,
                        window,
                        cx,
                    )
                });
                let weak = view.downgrade();
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if initial_focus == InitialFocus::Terminal {
                            window.focus(&view.focus);
                        } else {
                            view.focus_editor(window, cx);
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
    shell_metadata: ShellMetadata,
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
        let editor = command_editor::new_draft(shell, &settings, "", window, cx);
        if initial_focus == InitialFocus::Terminal {
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
                            if this.editor.read(cx).value().as_ref() == text
                                && this.editor.read(cx).cursor() == cursor
                            {
                                this.completion_text = text;
                                this.completion_cursor = cursor;
                                this.candidates = candidates;
                                this.notice = Some(if this.candidates.is_empty() {
                                    "No local completion matches. Terminal Tab still uses the running application's native completion."
                                        .into()
                                } else {
                                    "Choose a completion, or press Tab again to accept the first. Nothing is executed."
                                        .into()
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

                    let pump = this.session.pump_observed();
                    let mut changed = false;
                    for event in pump.observed {
                        changed |= match event {
                            Observed::Output { at, bytes } => {
                                let metadata_changed = this.shell_metadata.ingest(&bytes);
                                metadata_changed || this.document.ingest_output(at, &bytes)
                            }
                            Observed::Exit { at, .. } => this.document.abort_in_flight(at),
                        };
                    }
                    if changed {
                        this.document_ui.dirty = true;
                    }
                    if pump.changed || changed {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        });
        let show_blocks = settings.show_blocks;
        Self {
            session,
            document,
            shell_metadata: ShellMetadata::default(),
            shell,
            keymap,
            settings,
            show_blocks,
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

    fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }

    // Kept for document_view's existing helper calls.
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
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

    /// Run the draft in the integrated shell. Execution is authoritative; block
    /// tracking is observational. We never queue a local block or make document
    /// retention a prerequisite for sending the command.
    fn run_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.result(
                Err(anyhow::anyhow!(
                    "The local shell has not reported an idle prompt. Use Send to app while another application owns input."
                )),
                cx,
            );
            return;
        }

        let id = self.document.allocate_id();
        let wrapper = match shell.wrap(id, &text) {
            Ok(wrapper) => wrapper,
            Err(error) => {
                self.result(Err(error.into()), cx);
                return;
            }
        };
        let result = self.session.send_hidden(wrapper);
        if result.is_ok() {
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.candidates.clear();
            window.focus(&self.focus);
            // A start marker may create an optional block shortly afterward. If it
            // cannot be retained, execution still proceeds normally.
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
        }
        self.result(result, cx);
    }

    /// Send editor text literally to whichever application currently owns stdin.
    /// No shell wrapper, block boundary, cwd assumption or application-name sniffing.
    fn send_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn insert_newline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn toggle_blocks(&mut self, cx: &mut Context<Self>) {
        self.show_blocks = !self.show_blocks;
        self.document_ui.page_start = None;
        self.document_scroll.scroll_to_bottom();
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
                self.notice = Some(
                    "Terminal selection is not implemented yet. Use Copy view for an explicit whole-screen copy."
                        .into(),
                );
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
            Action::FocusEditor => self.focus_editor(window, cx),
            Action::Interrupt => {
                let result = self.session.send(vec![3]);
                if result.is_ok() {
                    self.document.note_terminal_input();
                }
                self.result(result, cx);
            }
            Action::RunShell => self.run_shell(window, cx),
            Action::SendApplication => self.send_editor(window, cx),
            Action::Newline => self.insert_newline(window, cx),
            Action::Complete => self.complete_editor(window, cx),
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
                self.notice = None;
                self.focus_editor(window, cx);
                cx.notify();
            }
            Action::Quit => cx.quit(),
        }
    }

    /// Raw terminal keyboard bridge. Kea semantic shortcuts are masked in the
    /// KeaTerminal context, so representable keys reach this encoder. Unencoded
    /// composed text falls through to the platform text-input handler.
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
        let terminal_metrics = terminal_font_metrics(window, cx);
        let layout_metrics = terminal_metrics.clone();
        let paint_metrics = terminal_metrics;
        let weak = cx.entity().downgrade();
        let terminal = div()
            .id("terminal-surface")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .key_context(if self.session.input_allowed() {
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
                        let size = kea_core::Size::new(
                            (f32::from(bounds.size.width) / f32::from(layout_metrics.cell_width))
                                .floor()
                                .clamp(2., 512.) as u16,
                            (f32::from(bounds.size.height) / f32::from(layout_metrics.line_height))
                                .floor()
                                .clamp(1., 256.) as u16,
                        );
                        let _ = weak.update(cx, |this, cx| {
                            let resized = this.terminal_bounds != Some(bounds);
                            this.terminal_bounds = Some(bounds);
                            if resized {
                                let weak = cx.entity().downgrade();
                                window.defer(cx, move |_, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Ok(size) = size {
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
                            paint_screen(&screen, bounds, &paint_metrics, window, cx);
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
            output = output.child(
                div()
                    .id("block-inspector")
                    .w(px(450.))
                    .h_full()
                    .flex_shrink_0()
                    .child(self.render_document(window, cx)),
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

        let status = self
            .notice
            .clone()
            .or_else(|| self.session.warning.clone())
            .unwrap_or_else(|| {
                if self.session.is_history() {
                    "History is read-only; the live process continues.".into()
                } else if self.focus.is_focused(window) {
                    "Terminal focus: representable keys belong to the child application.".into()
                } else if self.document.prompt_ready() {
                    "Editor focus: local shell ready. Run, Send and completion are explicit actions."
                        .into()
                } else {
                    "Editor focus: another application may own stdin; use Send to app unless the local shell reports ready."
                        .into()
                }
            });

        let directory = match (
            self.shell,
            self.document.directory(),
            self.document.prompt_ready(),
        ) {
            (None, _, _) => "Shell directory: unavailable for this program".into(),
            (_, Some(path), true) => format!("Current shell directory: {path}"),
            (_, Some(path), false) => format!("Shell directory (last reported): {path}"),
            _ => "Shell directory: waiting for shell integration".into(),
        };

        let newline_label = self.keymap.label_or(Action::Newline, "Enter");
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
                        "blocks",
                        if self.show_blocks {
                            "Hide blocks"
                        } else {
                            "Show blocks"
                        },
                        Action::ToggleBlocks,
                        cx,
                    ))
                    .child(self.control("copy-document", "Copy view", Action::CopyDocument, cx))
                    .child(
                        button("paste-terminal", "Paste to app")
                            .on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))),
                    )
                    .child(button("focus-input", "Focus editor").on_click(
                        cx.listener(|this, _, window, cx| this.focus_editor(window, cx)),
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
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .child(self.control(
                                "run-draft",
                                format!("Run in shell · {}", self.keymap.label(Action::RunShell)),
                                Action::RunShell,
                                cx,
                            ))
                            .child(self.control(
                                "send-draft",
                                format!(
                                    "Send to app · {}",
                                    self.keymap.label(Action::SendApplication)
                                ),
                                Action::SendApplication,
                                cx,
                            ))
                            .child(format!(
                                "Newline · {newline_label}   ·   Complete · {}",
                                self.keymap.label(Action::Complete)
                            )),
                    )
                    .child(
                        Input::new(&self.editor)
                            .h(px(100.))
                            .appearance(false)
                            .disabled(!self.session.input_allowed())
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
                "{} observed command blocks · {:.3}s / {:.3}s · {}",
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

fn terminal_font_metrics(window: &mut Window, cx: &mut App) -> TerminalFontMetrics {
    let (font_family, font_size) = {
        let theme = cx.theme();
        (theme.mono_font_family.clone(), theme.mono_font_size)
    };
    let mut terminal_font = font(font_family);
    // Terminal cells must not change width because a programming font turns
    // character sequences into contextual ligatures.
    terminal_font.features = FontFeatures::disable_ligatures();

    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&terminal_font);
    let cell_width = text_system
        .advance(font_id, font_size, 'M')
        .map(|advance| advance.width)
        .unwrap_or_else(|_| px(f32::from(font_size) * FALLBACK_CELL_WIDTH_EM));
    let line_height = px(f32::from(font_size) * TERMINAL_LINE_HEIGHT);

    TerminalFontMetrics {
        font: terminal_font,
        font_size,
        cell_width,
        line_height,
    }
}

fn paint_screen(
    screen: &Screen,
    bounds: Bounds<Pixels>,
    metrics: &TerminalFontMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    let cell_width = f32::from(metrics.cell_width);
    let line_height = f32::from(metrics.line_height);
    for (index, cell) in screen.cells.iter().enumerate() {
        let row = index / usize::from(screen.size.columns);
        let column = index % usize::from(screen.size.columns);
        let origin =
            bounds.origin + point(px(column as f32 * cell_width), px(row as f32 * line_height));
        if origin.x >= bounds.right() || origin.y >= bounds.bottom() {
            continue;
        }
        if cell.background != 0x11151a {
            window.paint_quad(fill(
                Bounds::new(origin, size(metrics.cell_width, metrics.line_height)),
                rgb(cell.background),
            ));
        }
        if cell.spacer || cell.text == " " {
            continue;
        }
        let mut cell_font = metrics.font.clone();
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
        let shaped = window.text_system().shape_line(
            cell.text.clone().into(),
            metrics.font_size,
            &[run],
            Some(px(if cell.wide {
                cell_width * 2.
            } else {
                cell_width
            })),
        );
        let _ = shaped.paint(origin, metrics.line_height, window, cx);
        if cell.underline {
            window.paint_quad(fill(
                Bounds::new(
                    origin + point(px(0.), px(line_height - 2.)),
                    size(
                        px(if cell.wide {
                            cell_width * 2.
                        } else {
                            cell_width
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
                px(column as f32 * cell_width),
                px(row as f32 * line_height + line_height - 2.),
            );
        if origin.x < bounds.right() && origin.y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(origin, size(metrics.cell_width, px(2.))),
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
