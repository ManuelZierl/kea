#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod completion_view;
mod document_view;
mod terminal_text;

use anyhow::{Context as _, Result};
use gpui::{prelude::*, *};
use gpui_component::{
    input::{self as edit, Input, InputState},
    ActiveTheme, Root, Theme, ThemeMode,
};
use kea_alacritty::Screen;
use kea_app::{
    command_editor, input,
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
enum InputMode {
    Document,
    Direct,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kea: {error:#}");
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
            println!("Kea — a document-native terminal\n\nkea [--direct] [--record NEW.kea] [-- PROGRAM ARG...]\nkea --replay SESSION.kea\nkea --demo\n\nEnter = newline; Ctrl+Enter = execute the command draft.\nCopy/Cut/Paste/Undo/Redo act on the focused text component.\nCtrl+Shift+Space = Document / Direct PTY; F10 = copy entire document/screen.\nLinux/Windows interrupt: Ctrl+Shift+C. macOS interrupt: Ctrl+C.\nF6/F7 = previous/next history event; F8 = playback; F9 = live.\n\nKeybindings and settings are configurable in Kea's configuration directory.\nKEA_KEYBINDINGS and KEA_SETTINGS select explicit configuration files.\n\nDocument mode supports POSIX shells and PowerShell. Other programs start in Direct mode.\nRecording is explicit, unencrypted, bounded and create-only. Output and command text can contain secrets.");
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
    if !demo && !replay_requested && !direct && command.is_empty() {
        command = vec![
            "powershell.exe".into(),
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NoExit".into(),
        ];
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
    if !demo && !replay_requested && !direct {
        if let Some(shell) = shell {
            session.send_hidden(shell.report_directory())?;
        }
    }
    let document = Document::from_recording(session.recording());
    let mode = if demo {
        InputMode::Direct
    } else if replay_requested && !document.blocks().is_empty() {
        InputMode::Document
    } else if direct || shell.is_none() {
        InputMode::Direct
    } else {
        InputMode::Document
    };
    let (keymap, warning) = Keymap::load();
    let (settings, settings_warning) = Settings::load();
    let mut warnings: Vec<String> = warning.into_iter().chain(settings_warning).collect();
    if !demo && !replay_requested && !direct && shell.is_none() {
        warnings.push("Document mode is unavailable for this program. Using the same process in Direct PTY mode.".into());
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
                    let _ = weak.update(cx, |view, cx| view.focus_active(window, cx));
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
    input_mode: InputMode,
    editor: Entity<InputState>,
    document_ui: document_view::DocumentUi,
    document_scroll: ScrollHandle,
    focus: FocusHandle,
    notice: Option<String>,
    terminal_preedit: Option<String>,
    completion: Option<completion_view::CompletionMenu>,
    completion_busy: bool,
    completion_task: Option<Task<()>>,
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
        input_mode: InputMode,
        notice: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let editor = command_editor::new_draft(shell, &settings, "", window, cx);
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
            settings,
            input_mode,
            editor,
            document_ui,
            document_scroll: ScrollHandle::new(),
            focus,
            notice,
            terminal_preedit: None,
            completion: None,
            completion_busy: false,
            completion_task: None,
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
        if self.input_mode == InputMode::Document && self.session.input_allowed() {
            self.editor.update(cx, |state, cx| state.focus(window, cx));
        } else {
            window.focus(&self.focus);
        }
    }
    fn copy_document(&self, cx: &mut App) {
        let text = if self.input_mode == InputMode::Document && !self.session.is_history() {
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
        self.result(result, cx);
    }
    fn execute_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input_mode != InputMode::Document {
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
                Err(anyhow::anyhow!(
                    "Return to LIVE before executing a document command."
                )),
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
                    "Document execution is unavailable for this program."
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
            self.completion = None;
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.focus_active(window, cx);
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
            self.document_scroll.scroll_to_bottom();
        }
        self.result(result, cx);
    }
    fn toggle_direct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        }) {
            self.notice = Some("Finish text composition before switching input modes.".into());
            cx.notify();
            return;
        }
        match self.input_mode {
            InputMode::Document => self.input_mode = InputMode::Direct,
            InputMode::Direct if self.shell.is_some() || !self.document.blocks().is_empty() => {
                self.input_mode = InputMode::Document
            }
            InputMode::Direct => {
                self.notice = Some(
                    "This process has no supported document adapter or recorded command blocks."
                        .into(),
                );
                cx.notify();
                return;
            }
        }
        self.notice = None;
        self.focus_active(window, cx);
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
            Action::Complete => self.complete(window, cx),
            Action::AcceptCompletion => self.accept_completion(window, cx),
            Action::NextCompletion => self.move_completion(1, cx),
            Action::PreviousCompletion => self.move_completion(-1, cx),
            Action::DismissCompletion => {
                self.completion = None;
                cx.notify();
            }
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
    fn terminal_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.session.input_allowed() {
            cx.stop_propagation();
            return;
        }
        if self.terminal_preedit.is_some() {
            // Candidate confirmation and dead-key composition belong to the OS.
            return;
        }
        if let Some(bytes) = input::encode(&event.keystroke, self.session.application_cursor()) {
            let result = self.session.send(bytes);
            self.result(result, cx);
            cx.stop_propagation();
        }
        // Do NOT consume an unhandled printable key. Windows delivers its text
        // later through WM_CHAR / EntityInputHandler (Space included).
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
        self.clear_stale_completion(window, cx);
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 24.).max(18.);
        let show_document = self.input_mode == InputMode::Document && !self.session.is_history();
        let input_height = if show_document && self.session.input_allowed() {
            160. + if self.completion.is_some() { 145. } else { 0. }
        } else {
            0.
        };
        let height = (f32::from(viewport.height) - 150. - input_height).max(20.);
        if show_document {
            if let Ok(size) = kea_core::Size::new(
                (width / CELL_WIDTH).floor().clamp(2., 512.) as u16,
                (height / LINE_HEIGHT).floor().clamp(1., 256.) as u16,
            ) {
                if let Err(error) = self.session.resize(size) {
                    self.notice = Some(error.to_string());
                }
            }
        }
        let duration = self.session.recording().duration();
        let position = self.session.position();
        let fraction = if duration == 0 {
            0.
        } else {
            position as f32 / duration as f32
        };
        let mode = if self.session.is_history() {
            "History · read only"
        } else if show_document {
            "Document"
        } else {
            "Direct PTY"
        };
        let status = self.terminal_preedit.as_ref().map(|text| format!("Composing: {text}")).or_else(|| self
            .session
            .warning
            .clone()
            .or_else(|| self.notice.clone()))
            .unwrap_or_else(|| {
                if self.session.is_history() {
                    "Historical state is read only. The live process continues.".into()
                } else if show_document && self.document.has_in_flight() {
                    format!(
                        "Command running. {} opens the same PTY for interactive input.",
                        self.keymap.label(Action::ToggleDirect)
                    )
                } else if show_document {
                    "Select and edit text normally. Only the execute action runs a command.".into()
                } else {
                    "All delivered keys go to the application. Use the toolbar for Paste, history, or Document mode.".into()
                }
            });
        let main_panel = if show_document {
            self.render_document(window, cx)
        } else {
            let screen = self.session.screen();
            let view = cx.entity();
            let focus = self.focus.clone();
            let live = self.session.input_allowed();
            div()
                .size_full()
                .key_context("KeaTerminal")
                .track_focus(&self.focus)
                .bg(rgb(0x11151a))
                .on_key_down(cx.listener(Self::terminal_key))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, _| window.focus(&this.focus)),
                )
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            paint_screen(&screen, bounds, window, cx);
                            if live {
                                window.handle_input(
                                    &focus,
                                    ElementInputHandler::new(bounds, view.clone()),
                                    cx,
                                );
                                // Use the actual terminal rectangle, not a guessed
                                // toolbar/editor height which can clip the last row.
                                if let Ok(size) = kea_core::Size::new(
                                    (f32::from(bounds.size.width) / CELL_WIDTH)
                                        .floor()
                                        .clamp(2., 512.) as u16,
                                    (f32::from(bounds.size.height) / LINE_HEIGHT)
                                        .floor()
                                        .clamp(1., 256.) as u16,
                                ) {
                                    if size != screen.size {
                                        let view = view.clone();
                                        window.defer(cx, move |_, cx| {
                                            view.update(cx, |this, cx| {
                                                let result = this.session.resize(size);
                                                this.result(result, cx);
                                            })
                                        });
                                    }
                                }
                            }
                        },
                    )
                    .size_full(),
                )
                .into_any_element()
        };
        let input_panel = if input_height != 0. {
            div()
                .id("command-editor")
                .key_context(if self.completion.is_some() {
                    "KeaCommand KeaCompletion"
                } else {
                    "KeaCommand"
                })
                .h(px(input_height))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .border_1()
                .border_color(cx.theme().border)
                .child(div().text_color(cx.theme().muted_foreground).child(format!(
                    "Command   ·   {} execute   ·   Enter newline",
                    self.keymap.label(Action::Execute)
                )))
                .child(
                    Input::new(&self.editor)
                        .h(px(122.))
                        .appearance(false)
                        .bordered(false),
                )
                .child(self.render_completions(cx))
                .into_any_element()
        } else {
            div().h(px(0.)).into_any_element()
        };
        let toolbar = div()
            .flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .child(div().font_weight(FontWeight::BOLD).child("Kea"))
            .child(self.control(
                "previous",
                format!("< {}", self.keymap.label(Action::PreviousEvent)),
                Action::PreviousEvent,
                cx,
            ))
            .child(self.control(
                "next",
                format!("{} >", self.keymap.label(Action::NextEvent)),
                Action::NextEvent,
                cx,
            ))
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
            .child(self.control("mode", mode, Action::ToggleDirect, cx))
            .when(!show_document, |bar| {
                bar.child(
                    button("paste-terminal", "Paste")
                        .on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))),
                )
            })
            .child(self.control(
                "copy-document",
                format!(
                    "Copy document/screen · {}",
                    self.keymap.label(Action::CopyDocument)
                ),
                Action::CopyDocument,
                cx,
            ));
        div()
            .id("kea")
            .size_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .key_context(if show_document {
                "Kea KeaDocument"
            } else if self.session.input_allowed() {
                // No ancestor Kea bindings in live Direct mode. Every delivered
                // key belongs to the child; use the toolbar to leave this mode.
                "KeaLive"
            } else {
                "Kea"
            })
            .on_action(cx.listener(Self::invoke))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(cx.theme().mono_font_size)
            .font_family(cx.theme().mono_font_family.clone())
            .child(toolbar)
            .child(
                div()
                    .h(px(22.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(format!(
                        "Directory{}: {}",
                        if self.input_mode == InputMode::Direct {
                            " (last reported)"
                        } else {
                            ""
                        },
                        self.document
                            .current_directory()
                            .unwrap_or("awaiting shell report")
                    )),
            )
            .child(
                div()
                    .h(px(20.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(status),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(main_panel))
            .child(input_panel)
            .child(
                div()
                    .id("timeline")
                    .h(px(12.))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.seek_x(event.position.x, width, window, cx)
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                            if event.pressed_button == Some(MouseButton::Left) {
                                this.seek_x(event.position.x, width, window, cx);
                            }
                        }),
                    )
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
                "{} blocks   ·   {:.3}s / {:.3}s   ·   {}",
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
