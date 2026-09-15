#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod document_view;

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
enum InputTarget {
    ShellCommand,
    Application,
}

fn main() {
    if let Err(error) = run() {
        report_error(&format!("kea: {error:#}"));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut demo, mut replay, mut record, mut direct) = (false, None, None, false);
    let mut blocks_requested = false;
    let mut command: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--" {
            command.extend(args);
            break;
        }
        if arg == "--demo" {
            demo = true;
        } else if arg == "--blocks" {
            blocks_requested = true;
        } else if arg == "--direct" {
            direct = true;
        } else if arg == "--replay" {
            replay = Some(PathBuf::from(args.next().context("--replay needs a path")?));
        } else if arg == "--record" {
            record = Some(PathBuf::from(
                args.next().context("--record needs a new file path")?,
            ));
        } else if arg == "--help" || arg == "-h" {
            println!("Kea — terminal and editor over one live session\n\nkea [--blocks] [--direct] [--record NEW.kea] [-- PROGRAM ARG...]\nkea --replay SESSION.kea\nkea --demo\n\nThe terminal and bottom editor are available together. Blocks are optional.\nClick the terminal: Kea does not reserve its shortcuts there.\nClick Editor: normal editing; Ctrl+Enter runs/sends, Enter is newline.\nCtrl+Space (editor only): transfer one line and Tab to the application's completion.\nNo Enter is sent by Complete. Application text never receives shell wrappers.\nUse the toolbar to select shell-command input after unmanaged terminal interaction.\nSettings and shortcuts: KEA_SETTINGS, KEA_KEYBINDINGS.\nRecordings are explicit, bounded, unencrypted and can contain secrets.");
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
    if !demo && !replay_requested {
        if let Some(shell) = shell {
            // A recognized shell starts under our ownership. The metadata request
            // runs once, before accepting GUI input, never inside a launched TUI.
            session.send_hidden(shell.directory_query())?;
        }
    }
    let document = Document::from_recording(session.recording());
    let mode = if demo {
        InputTarget::Application
    } else if replay_requested && !document.blocks().is_empty() {
        InputTarget::ShellCommand
    } else if direct || shell.is_none() {
        InputTarget::Application
    } else {
        InputTarget::ShellCommand
    };
    let (keymap, warning) = Keymap::load();
    let (mut settings, settings_warning) = Settings::load();
    settings.show_blocks |= blocks_requested;
    let mut warnings: Vec<String> = warning.into_iter().chain(settings_warning).collect();
    if !demo && !replay_requested && !direct && shell.is_none() {
        warnings.push(
            "Application input: the editor sends text to this process without shell wrappers."
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
                        session, document, shell, keymap, settings, mode, notice, window, cx,
                    )
                });
                let weak = view.downgrade();
                window.defer(cx, move |window, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if mode == InputTarget::Application {
                            window.focus(&view.focus);
                        } else {
                            view.focus_active(window, cx);
                        }
                    });
                });
                cx.new(|cx| Root::new(view, window, cx))
            }) {
                report_error(&format!("kea: cannot open window: {error:#}"));
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
    input_target: InputTarget,
    show_blocks: bool,
    terminal_size: Option<kea_core::Size>,
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
        input_target: InputTarget,
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
        let show_blocks = settings.show_blocks;
        Self {
            session,
            document,
            shell,
            keymap,
            settings,
            input_target,
            show_blocks,
            terminal_size: None,
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
        if self.session.input_allowed() {
            self.editor.update(cx, |state, cx| state.focus(window, cx));
        } else {
            window.focus(&self.focus);
        }
    }
    fn shell_submission(&self) -> bool {
        self.input_target == InputTarget::ShellCommand
            && self.shell.is_some()
            && !self.document.has_in_flight()
    }
    fn send_application(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) -> Result<()> {
        self.session.send(bytes)?;
        // After unmanaged terminal input we cannot know whether a shell, REPL or
        // agent owns stdin. Do not send an eval wrapper there by accident.
        if !self.document.has_in_flight() {
            self.input_target = InputTarget::Application;
        }
        cx.notify();
        Ok(())
    }
    fn complete_in_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            return;
        };
        let result = input::complete(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.send_application(bytes, cx));
        if result.is_ok() {
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            window.focus(&self.focus);
        }
        self.result(result, cx);
    }
    fn copy_document(&self, cx: &mut App) {
        let text = if self.show_blocks
            && (self.shell.is_some() || !self.document.blocks().is_empty())
            && !self.session.is_history()
        {
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
            .and_then(|bytes| self.send_application(bytes, cx));
        self.result(result, cx);
    }
    fn execute_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        if !self.shell_submission() {
            let result = input::submit(&text, self.session.bracketed_paste())
                .and_then(|bytes| self.send_application(bytes, cx));
            if result.is_ok() {
                self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
                self.focus_active(window, cx);
            }
            self.result(result, cx);
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
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.focus_active(window, cx);
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
            self.document_ui.follow_latest();
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
        if self.shell.is_none() || self.document.has_in_flight() {
            self.notice = Some("The running application owns input. Finish it before choosing shell-command submission.".into());
            cx.notify();
            return;
        }
        self.input_target = match self.input_target {
            InputTarget::ShellCommand => InputTarget::Application,
            InputTarget::Application => InputTarget::ShellCommand,
        };
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
                let result = self.send_application(vec![3], cx);
                self.result(result, cx);
            }
            Action::Execute => self.execute_editor(window, cx),
            Action::Complete => self.complete_in_terminal(window, cx),
            Action::ToggleBlocks => {
                self.show_blocks = !self.show_blocks;
                if self.show_blocks {
                    self.document_ui.follow_latest();
                }
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
        if self.session.input_allowed() {
            if let Some(bytes) = input::encode(&event.keystroke, self.session.application_cursor())
            {
                let result = self.send_application(bytes, cx);
                self.result(result, cx);
            }
        }
        cx.stop_propagation();
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
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 24.).max(18.);
        let show_document = self.show_blocks && !self.session.is_history();
        let input_height = 160.;
        let duration = self.session.recording().duration();
        let position = self.session.position();
        let fraction = if duration == 0 {
            0.
        } else {
            position as f32 / duration as f32
        };
        let shell_submission = self.shell_submission();
        let mode = if shell_submission {
            "Input: shell command"
        } else {
            "Input: application text"
        };
        let status = self.session.warning.clone().or_else(|| self.notice.clone()).unwrap_or_else(|| {
            if !self.session.input_allowed() {
                "Read-only history/recording. Live processes are not rewound.".into()
            } else if shell_submission {
                "Editor: write a command. Terminal: click to interact directly; all Kea shortcuts are passed through.".into()
            } else {
                "Application input: Send adds Enter; Complete transfers one line plus Tab, without Enter. No shell wrappers.".into()
            }
        });
        let screen = self.session.screen();
        let terminal_entity = cx.entity();
        let layout_entity = cx.entity().downgrade();
        let terminal = div()
            .id("terminal-surface")
            .size_full()
            .key_context(if self.session.input_allowed() {
                "KeaTerminal"
            } else {
                "KeaHistory"
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
                        // Use the actual viewport, not guessed toolbar/editor heights.
                        // The old estimate could hide the last row/prompt entirely.
                        let columns = (f32::from(bounds.size.width) / CELL_WIDTH)
                            .floor()
                            .clamp(2., 512.) as u16;
                        let rows = (f32::from(bounds.size.height) / LINE_HEIGHT)
                            .floor()
                            .clamp(1., 256.) as u16;
                        if let Ok(size) = kea_core::Size::new(columns, rows) {
                            let entity = layout_entity.clone();
                            window.defer(cx, move |_, cx| {
                                let _ = entity.update(cx, |this, cx| {
                                    if this.terminal_size != Some(size) {
                                        this.terminal_size = Some(size);
                                        if let Err(error) = this.session.resize(size) {
                                            this.notice = Some(error.to_string());
                                        }
                                        cx.notify();
                                    }
                                });
                            });
                        }
                    },
                    move |bounds, _, window, cx| {
                        let _ = &terminal_entity; // keep the owning view alive through paint
                        paint_screen(&screen, bounds, window, cx);
                    },
                )
                .size_full(),
            );
        let mut main_panel = div().size_full().flex().gap_2().child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_hidden()
                .child(terminal),
        );
        if show_document {
            main_panel = main_panel.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(self.render_document(window, cx)),
            );
        }
        let input_panel = if input_height != 0. {
            div()
                .id("command-editor")
                .key_context("KeaCommand")
                .h(px(input_height))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .border_1()
                .border_color(cx.theme().border)
                .child(div().text_color(cx.theme().muted_foreground).child(format!(
                    "{}   ·   {} {}   ·   Enter newline",
                    mode,
                    self.keymap.label(Action::Execute),
                    if shell_submission { "run" } else { "send" }
                )))
                .child(
                    Input::new(&self.editor)
                        .h(px(input_height - 38.))
                        .appearance(false)
                        .disabled(!self.session.input_allowed())
                        .bordered(false),
                )
                .into_any_element()
        } else {
            div().h(px(0.)).into_any_element()
        };
        let toolbar = div()
            .flex()
            .flex_wrap()
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
            .child(self.control(
                "show-blocks",
                if self.show_blocks {
                    "Hide blocks"
                } else {
                    "Show blocks"
                },
                Action::ToggleBlocks,
                cx,
            ))
            .child(self.control("focus-editor", "Editor", Action::FocusEditor, cx))
            .child(self.control("complete", "Complete", Action::Complete, cx))
            .child(
                button("paste-terminal", "Paste to terminal")
                    .on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))),
            )
            .child(self.control(
                "copy-document",
                format!("Copy all · {}", self.keymap.label(Action::CopyDocument)),
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
            .key_context("Kea KeaDocument")
            .on_action(cx.listener(Self::invoke))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(cx.theme().mono_font_size)
            .font_family(cx.theme().mono_font_family.clone())
            .child(toolbar)
            .child(
                div()
                    .h(px(20.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(status),
            )
            .child(div().h(px(20.)).flex_shrink_0().overflow_hidden().child(
                self.document.current_directory().map(|path| format!("Shell directory (last reported): {}", path.replace(['\r', '\n'], " ")))
                    .unwrap_or_else(|| "Shell directory: awaiting shell metadata (not inferred from the prompt)".into())
            ))
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

fn report_error(message: &str) {
    eprintln!("{message}");
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        let directory = PathBuf::from(root).join("Kea");
        if std::fs::create_dir_all(&directory).is_ok() {
            // One bounded startup diagnostic; never dump terminal output or input.
            let _ = std::fs::write(directory.join("startup-error.log"), message);
        }
    }
}
