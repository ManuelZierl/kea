#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod document_view;
mod shell_metadata;
mod terminal_input;

use anyhow::{Context as _, Result};
use gpui::{prelude::*, *};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{self as edit, Input, InputState},
    resizable::{resizable_panel, v_resizable},
    ActiveTheme, Disableable as _, IconName, Root, Selectable as _, Sizable as _, Theme, ThemeMode,
    TitleBar,
};
use kea_alacritty::{MouseTracking, Screen, TerminalPoint, TERMINAL_SCROLLBACK_LINES};
use kea_app::{
    command_editor, completion, draft_history, input,
    keybindings::{Action, Invoke, Keymap},
    playback, session_files,
    settings::{Appearance, Settings},
    shell::ShellFlavor,
    terminal_mouse::{self, PointerButton, PointerEvent, WheelDirection},
    terminal_recovery::PromptLineTracker,
    terminal_selection::{self, KeyModifiers, LocalKey, MouseOwner},
};
use kea_document::Document;
use kea_session::{Observed, Session};
use shell_metadata::ShellMetadata;
use std::{ffi::OsString, fs::File, path::PathBuf, time::Duration};

const FALLBACK_CELL_WIDTH_EM: f32 = 0.6;
const MAX_SCROLL_LINES_PER_EVENT: i32 = TERMINAL_SCROLLBACK_LINES as i32;
const MAX_MOUSE_WHEEL_REPORTS_PER_EVENT: i32 = 32;
const TERMINAL_SELECTION_BACKGROUND: u32 = 0x264f78;
const TERMINAL_SELECTION_FOREGROUND: u32 = 0xffffff;

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

struct PendingRun {
    text: String,
    editor: Entity<InputState>,
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
Editor defaults: Enter = new line, Ctrl+Enter = Run in shell, Ctrl+Shift+Enter = send literal text to the current terminal app, Tab = complete.
All semantic shortcuts are configurable. Terminal-like editor behavior is possible with:
  run_shell = enter
  newline = shift-enter

Blocks are an optional observational view. They never gate command execution.
The current integrated local-shell directory is reported by shell hooks; while a TUI/remote app owns the terminal, Kea labels it last reported rather than guessing.
Terminal Tab and other representable keys go to the child application.
Mouse buttons/wheel are forwarded when the child negotiates mouse reporting; hold Shift for local selection/scrollback.
Modified Enter is distinguished only after the child negotiates an extended keyboard protocol.

KEA_KEYBINDINGS and KEA_SETTINGS select explicit configuration files.
Sessions are temporary unless Save session or --record is used. Saved recordings are bounded and unencrypted; commands/output can contain secrets.";
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
    let initial_focus = if demo || terminal_focus || shell.is_none() {
        InitialFocus::Terminal
    } else {
        InitialFocus::Editor
    };
    let (keymap, warning) = Keymap::load();
    let (settings, settings_warning) = Settings::load();
    let mut warnings: Vec<String> = warning.into_iter().chain(settings_warning).collect();
    // Draft history persistence is explicit opt-in (default off): the file is
    // plaintext and submissions can contain secrets. Load outside the UI loop;
    // the loop only hands the entries to recall.
    let persisted_history = if settings.persist_history && !demo && !replay_requested {
        match session_files::session_directory() {
            Ok(directory) => {
                let path = directory.join("draft-history.txt");
                match draft_history::load_history_file(&path) {
                    Ok(entries) => Some((path, entries)),
                    Err(error) => {
                        warnings.push(format!(
                            "Draft history will not persist: {}: {error}.",
                            path.display()
                        ));
                        None
                    }
                }
            }
            Err(error) => {
                warnings.push(format!("Draft history will not persist: {error}."));
                None
            }
        }
    } else {
        None
    };
    if !demo && !replay_requested && shell.is_none() {
        warnings.push(
            "No integrated local shell detected. Terminal input and Send to app remain available; Run in shell and shell cwd completion are unavailable."
                .into(),
        );
    }
    let notice = (!warnings.is_empty()).then(|| format!("Warning: {}", warnings.join(" ")));

    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            command_editor::register_languages();
            keymap.install(cx);
            if let Some((path, entries)) = persisted_history {
                command_editor::set_history_persistence(path, entries, cx);
            }
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
                    ..TitleBar::title_bar_options()
                }),
                window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
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
    terminal_scroll_remainder: f32,
    terminal_mouse_scroll_remainder: f32,
    terminal_gesture: Option<terminal_selection::Gesture>,
    timeline_track_bounds: Option<Bounds<Pixels>>,
    timeline_hovered: bool,
    prompt_line: PromptLineTracker,
    pending_run: Option<PendingRun>,
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
    _focus_lost: Subscription,
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
        let focus_lost = cx.on_blur(&focus, window, |this, _, cx| {
            this.session.terminal_selection_focus_lost();
            cx.notify();
        });
        let appearance = cx.observe_window_appearance(window, |this, window, cx| {
            apply_appearance(&this.settings, window, cx);
            cx.notify();
        });
        let pump = cx.spawn_in(window, async move |this, cx| loop {
            Timer::after(Duration::from_millis(16)).await;
            let disconnected = cx
                .update(|window, cx| {
                    this.update(cx, |this, cx| {
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
                        let prompt_arrived = this.pump_session(cx);
                        if !this.focus.is_focused(window) {
                            // Output can invalidate a range after focus has
                            // already left. Do not retain a new caret-only
                            // interaction in an unfocused surface.
                            this.session.terminal_selection_focus_lost();
                        }
                        if prompt_arrived {
                            this.complete_pending_run(window, cx);
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
            shell,
            keymap,
            settings,
            show_blocks,
            terminal_bounds: None,
            terminal_scroll_remainder: 0.,
            terminal_mouse_scroll_remainder: 0.,
            terminal_gesture: None,
            timeline_track_bounds: None,
            timeline_hovered: false,
            prompt_line: PromptLineTracker::default(),
            pending_run: None,
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
            _focus_lost: focus_lost,
        }
    }

    fn result(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        self.notice = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn save_session(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.session.persistence_path() {
            self.notice = Some(format!("Session is saving to {}", path.display()));
            cx.notify();
            return;
        }
        let result = session_files::new_recording_path().and_then(|path| {
            self.session.start_persistence(&path)?;
            self.notice = Some(format!("Saving session locally: {}", path.display()));
            Ok(())
        });
        self.result(result, cx);
    }

    fn pump_session(&mut self, cx: &mut Context<Self>) -> bool {
        let was_prompt_ready = self.document.prompt_ready();
        let pump = self.session.pump_observed();
        let mut changed = false;
        for event in pump.observed {
            changed |= match event {
                Observed::Output { at, bytes } => {
                    let metadata_changed = self.shell_metadata.ingest(&bytes);
                    metadata_changed || self.document.ingest_output(at, &bytes)
                }
                Observed::Exit { at, .. } => self.document.abort_in_flight(at),
            };
        }
        let prompt_arrived = !was_prompt_ready && self.document.prompt_ready();
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

    fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor(window, cx);
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
            // Manual acceptance: copying the visible screen used to append one
            // blank line per empty grid row below the content. Drop that empty
            // tail; it was never selected or visible content.
            self.session.screen().text().trim_end().to_string()
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        // Manual acceptance A7: the copy buttons gave no visible feedback.
        self.notice = Some(format!("Copied {scope}."));
        cx.notify();
    }

    fn copy_terminal_selection(&mut self, cx: &mut Context<Self>) {
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

    fn return_terminal_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.session.scroll_bottom();
        self.terminal_scroll_remainder = 0.;
        cx.notify();
    }

    fn note_forwarded_terminal_input(&mut self) {
        self.session.scroll_bottom();
        self.session.clear_terminal_selection();
        self.prompt_line.invalidate();
        self.pending_run = None;
        self.document.note_terminal_input();
        self.notice = None;
    }

    fn select_terminal_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.terminal_local_selection_active() {
            self.session.clear_terminal_selection();
        } else {
            self.session.enter_terminal_selection();
        }
        window.focus(&self.focus);
        self.notice = None;
        cx.notify();
    }

    fn terminal_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus);
        let metrics = terminal_font_metrics(window, cx);
        let Some(point) = terminal_point(
            event.position,
            self.terminal_bounds,
            &metrics,
            self.session.terminal_size(),
        ) else {
            return;
        };
        let reporting = self.session.terminal_mouse_reporting();
        let owner = terminal_selection::mouse_owner(
            reporting,
            self.settings.shift_mouse_selects_locally,
            self.session.terminal_explicit_selection_active(),
            event.modifiers.shift,
            event.modifiers.alt,
        );
        let gesture = terminal_selection::Gesture::new(
            point,
            owner,
            self.session.terminal_explicit_selection_active(),
            event.modifiers.shift,
            self.session.terminal_has_selection(),
        );
        self.terminal_gesture = Some(gesture);
        match owner {
            MouseOwner::LocalSimple | MouseOwner::LocalBlock => {
                if gesture.shift_extend {
                    self.session.extend_terminal_selection(point);
                } else if gesture.explicit {
                    self.session.place_terminal_selection_caret(point);
                } else {
                    // Alt changes the shape only after movement. An Alt click
                    // without movement follows the ordinary local click rule.
                    self.session.begin_terminal_selection(point);
                }
                self.notice = None;
                cx.notify();
            }
            MouseOwner::Forward => {
                let result = self.forward_pointer(
                    PointerEvent::Press,
                    point,
                    KeyModifiers {
                        shift: event.modifiers.shift,
                        alt: event.modifiers.alt,
                        control: event.modifiers.control,
                        ..Default::default()
                    },
                    true,
                    cx,
                );
                if result.is_err() {
                    // No successful press means there is no child gesture to
                    // complete with later motion or release events.
                    self.terminal_gesture = None;
                }
                self.result(result, cx);
                cx.stop_propagation();
            }
        }
    }

    fn terminal_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut gesture) = self.terminal_gesture {
            if !event.dragging() {
                return;
            }
            let metrics = terminal_font_metrics(window, cx);
            if let Some(point) = terminal_point(
                event.position,
                self.terminal_bounds,
                &metrics,
                self.session.terminal_size(),
            ) {
                if gesture.owner != MouseOwner::Forward {
                    self.update_local_gesture(&mut gesture, point);
                    cx.notify();
                } else if gesture.owner == MouseOwner::Forward {
                    let tracking = self.session.terminal_mouse_tracking();
                    let should_report =
                        matches!(tracking, Some(MouseTracking::Drag | MouseTracking::Motion))
                            && (event.pressed_button == Some(MouseButton::Left)
                                || matches!(tracking, Some(MouseTracking::Motion)));
                    if should_report {
                        let result = self.forward_pointer(
                            PointerEvent::Motion,
                            point,
                            KeyModifiers {
                                shift: event.modifiers.shift,
                                alt: event.modifiers.alt,
                                control: event.modifiers.control,
                                ..Default::default()
                            },
                            false,
                            cx,
                        );
                        self.result(result, cx);
                        cx.stop_propagation();
                    }
                }
                self.terminal_gesture = Some(gesture);
            }
            return;
        }

        // A held button without a press owned by this surface is not a child
        // gesture. Only unpressed, negotiated hover can start here.
        if event.pressed_button.is_some()
            || self.session.terminal_mouse_tracking() != Some(MouseTracking::Motion)
        {
            return;
        }
        let metrics = terminal_font_metrics(window, cx);
        let Some(point) = terminal_point(
            event.position,
            self.terminal_bounds,
            &metrics,
            self.session.terminal_size(),
        ) else {
            return;
        };
        let Some(encoding) = self.session.terminal_mouse_encoding() else {
            return;
        };
        let result = terminal_mouse::encode_pointer(
            encoding,
            PointerEvent::Motion,
            None,
            point,
            event.modifiers.shift,
            event.modifiers.alt,
            event.modifiers.control,
        )
        .map_err(anyhow::Error::msg)
        .and_then(|bytes| self.session.send(bytes));
        if result.is_err() {
            self.result(result, cx);
        }
        cx.stop_propagation();
    }

    fn terminal_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut gesture) = self.terminal_gesture.take() {
            let metrics = terminal_font_metrics(window, cx);
            let point = terminal_point(
                event.position,
                self.terminal_bounds,
                &metrics,
                self.session.terminal_size(),
            );
            if gesture.owner != MouseOwner::Forward {
                if let Some(point) = point {
                    self.update_local_gesture(&mut gesture, point);
                    if !gesture.moved && !gesture.preserve_click && !gesture.explicit {
                        self.session.clear_terminal_selection();
                    }
                }
                cx.notify();
            } else if let Some(point) = point {
                let result = self.forward_pointer(
                    PointerEvent::Release,
                    point,
                    KeyModifiers {
                        shift: event.modifiers.shift,
                        alt: event.modifiers.alt,
                        control: event.modifiers.control,
                        ..Default::default()
                    },
                    false,
                    cx,
                );
                self.result(result, cx);
                cx.stop_propagation();
            }
        }
    }

    fn update_local_gesture(
        &mut self,
        gesture: &mut terminal_selection::Gesture,
        point: TerminalPoint,
    ) {
        let first_move = !gesture.moved;
        if !gesture.moved_to(point) {
            return;
        }
        if first_move && gesture.potential_block {
            // Keep the original content-anchored endpoint, including when
            // Shift extends an existing selection or the viewport scrolls.
            self.session.set_terminal_selection_block(true);
        }
        self.session.update_terminal_selection(point);
    }

    fn forward_pointer(
        &mut self,
        event: PointerEvent,
        point: TerminalPoint,
        modifiers: KeyModifiers,
        input_effects: bool,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if event == PointerEvent::Press {
            // A child-owned press ends local interaction even if delivery fails.
            self.session.clear_terminal_selection();
        }
        let Some(encoding) = self.session.terminal_mouse_encoding() else {
            return Ok(());
        };
        let button = Some(PointerButton::Left);
        let bytes = terminal_mouse::encode_pointer(
            encoding,
            event,
            button,
            point,
            modifiers.shift,
            modifiers.alt,
            modifiers.control,
        )
        .map_err(anyhow::Error::msg)?;
        let result = self.session.send(bytes);
        if input_effects && result.is_ok() {
            self.note_forwarded_terminal_input();
        }
        if result.is_err() {
            cx.notify();
        }
        result
    }

    fn terminal_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let metrics = terminal_font_metrics(window, cx);
        if let Some(encoding) = self.session.terminal_mouse_encoding() {
            if !event.modifiers.shift {
                self.terminal_scroll_remainder = 0.;
                let reports = terminal_scroll_units(
                    event.delta,
                    metrics.line_height,
                    &mut self.terminal_mouse_scroll_remainder,
                    MAX_MOUSE_WHEEL_REPORTS_PER_EVENT,
                );
                if reports != 0 {
                    let Some(point) = terminal_point(
                        event.position,
                        self.terminal_bounds,
                        &metrics,
                        self.session.terminal_size(),
                    ) else {
                        cx.stop_propagation();
                        return;
                    };
                    let direction = if reports > 0 {
                        WheelDirection::Up
                    } else {
                        WheelDirection::Down
                    };
                    let result = terminal_mouse::encode_wheel(
                        encoding,
                        direction,
                        point,
                        event.modifiers.alt,
                        event.modifiers.control,
                    )
                    .map_err(anyhow::Error::msg)
                    .and_then(|report| {
                        let mut bytes =
                            Vec::with_capacity(report.len() * reports.unsigned_abs() as usize);
                        for _ in 0..reports.unsigned_abs() {
                            bytes.extend_from_slice(&report);
                        }
                        self.session.send(bytes)
                    });
                    if result.is_ok() {
                        self.note_forwarded_terminal_input();
                    }
                    self.result(result, cx);
                }
                cx.stop_propagation();
                return;
            }
        }
        self.terminal_mouse_scroll_remainder = 0.;
        let lines = terminal_scroll_lines(
            event.delta,
            metrics.line_height,
            &mut self.terminal_scroll_remainder,
        );
        if lines != 0 {
            self.session.scroll_lines(lines);
            self.notice = None;
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn paste_terminal(&mut self, cx: &mut Context<Self>) {
        self.session.clear_terminal_selection();
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            cx.notify();
            return;
        };
        let result = input::paste(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.session.send(bytes));
        if result.is_ok() {
            self.note_forwarded_terminal_input();
        }
        self.result(result, cx);
    }

    fn run_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
            self.session.scroll_bottom();
            self.session.clear_terminal_selection();
            self.prompt_line.invalidate();
            self.document.note_terminal_input();
            self.editor = command_editor::new_draft(self.shell, &self.settings, "", window, cx);
            self.candidates.clear();
            window.focus(&self.focus);
            self.document_ui.page_start = None;
            self.document_ui.dirty = true;
        }
        self.result(result, cx);
    }

    fn complete_pending_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn send_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else {
            self.notice =
                Some("Focus the composer before sending text to the terminal app.".into());
            cx.notify();
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
            self.session.scroll_bottom();
            self.session.clear_terminal_selection();
            self.prompt_line.invalidate();
            self.pending_run = None;
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

    fn seek_x(&mut self, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
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

    fn invoke(&mut self, event: &Invoke, window: &mut Window, cx: &mut Context<Self>) {
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
            Action::FocusEditor => self.focus_editor(window, cx),
            Action::SelectTerminalText => self.select_terminal_text(window, cx),
            Action::Interrupt => {
                let result = self.session.send(vec![3]);
                if result.is_ok() {
                    self.session.scroll_bottom();
                    self.session.clear_terminal_selection();
                    self.prompt_line.invalidate();
                    self.pending_run = None;
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
                self.prompt_line.invalidate();
                self.pending_run = None;
                self.notice = None;
                self.focus_editor(window, cx);
                cx.notify();
            }
            Action::Quit => cx.quit(),
        }
    }

    fn terminal_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let has_marked_text = self.terminal_composition.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        });
        if input::defer_to_ime(&event.keystroke, has_marked_text) {
            return;
        }
        match terminal_selection::local_key(
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
        // Fail open: the triggering input ends local interaction even if PTY
        // delivery later fails, so selection never traps terminal input.
        self.session.clear_terminal_selection();
        cx.notify();
        if let Some(bytes) = input::encode(
            &event.keystroke,
            self.session.application_cursor(),
            self.session.terminal_extended_keyboard(),
        ) {
            if self.session.input_allowed() {
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

    fn icon_control(
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
        let terminal_display_offset = screen.display_offset;
        let terminal_history_size = screen.history_size;
        let terminal_selection_available = self.session.terminal_has_selection();
        let terminal_local_active = self.session.terminal_local_selection_active();
        let terminal_selection_invalidated = screen.selection_invalidated;
        let terminal_mouse_reporting = self.session.terminal_mouse_reporting();
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
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::terminal_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::terminal_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::terminal_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::terminal_mouse_up))
            .on_mouse_move(cx.listener(Self::terminal_mouse_move))
            .on_scroll_wheel(cx.listener(Self::terminal_scroll))
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

        let terminal_mode = if self.session.is_history() {
            "History"
        } else {
            "Live"
        };
        let terminal_context = if self
            .terminal_gesture
            .is_some_and(|gesture| gesture.owner != MouseOwner::Forward)
        {
            "Selecting terminal text locally"
        } else if terminal_selection_invalidated {
            "Selection changed · local caret retained"
        } else if terminal_local_active {
            "Local selection · arrows move · Shift+arrows extend · Esc clears"
        } else if self.session.is_history() {
            "History · select/copy only"
        } else if terminal_mouse_reporting && self.settings.shift_mouse_selects_locally {
            "Keyboard and mouse to app · Shift-drag selects locally"
        } else if terminal_mouse_reporting {
            "Keyboard and mouse to app · Use Select terminal text"
        } else if self.focus.is_focused(window) {
            "Keyboard to app · Drag selects terminal text"
        } else {
            "Click to interact"
        };
        let mut terminal_header = div()
            .min_h(px(30.))
            .flex_shrink_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::BOLD).child("Terminal"))
            .child(terminal_mode)
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(terminal_context),
            )
            .child(div().flex_1())
            .child(if terminal_display_offset == 0 {
                format!("{terminal_history_size} lines")
            } else {
                format!("{terminal_display_offset} above bottom")
            })
            .child(
                Button::new("select-terminal-text")
                    .label(if terminal_local_active {
                        "Stop selecting"
                    } else {
                        "Select text"
                    })
                    .tooltip("Toggle local terminal text selection")
                    .ghost()
                    .small()
                    .selected(terminal_local_active)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.select_terminal_text(window, cx)),
                    ),
            )
            .child(
                Button::new("copy-terminal-selection")
                    .icon(IconName::Copy)
                    .tooltip("Copy terminal selection")
                    .ghost()
                    .small()
                    .disabled(!terminal_selection_available)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_terminal_selection(cx))),
            )
            // Manual acceptance E5: keyboard paste intentionally reaches the
            // child while live-terminal focus owns the keyboard, which left no
            // paste affordance for the terminal at all. This button lives on
            // the surface it acts on, next to terminal copy.
            .child(
                Button::new("paste-terminal")
                    .label("Paste")
                    .tooltip("Paste clipboard into the terminal")
                    .ghost()
                    .small()
                    .disabled(!self.session.input_allowed())
                    .on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))),
            );
        if terminal_display_offset > 0 {
            terminal_header = terminal_header.child(
                Button::new("terminal-bottom")
                    .icon(IconName::ArrowDown)
                    .tooltip("Return to latest output")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.return_terminal_to_bottom(cx))),
            );
        }
        let terminal_panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(terminal_header)
            .child(terminal);

        let mut output = div()
            .flex()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(terminal_panel);
        if self.show_blocks {
            output = output.child(
                div()
                    .id("block-inspector")
                    .w(px(420.))
                    .h_full()
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(cx.theme().border)
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
                        .rounded_sm()
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
                    // Manual acceptance D2: users did not notice read-only mode
                    // and felt the app had frozen. Name the way back out.
                    format!(
                        "Read-only history; the live process continues. Return live ({}) to resume typing.",
                        self.keymap.label(Action::GoLive)
                    )
                } else if self.document.prompt_ready() {
                    "Shell ready".into()
                } else if self.shell.is_some() {
                    "Shell busy · Send to app remains available".into()
                } else {
                    "Terminal application owns input".into()
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
        let persistence_status = if self.session.persistence_active() {
            let name = self
                .session
                .persistence_path()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "session.kea".into());
            format!("Saving · {name}")
        } else if self.session.persistence_path().is_some() {
            "Saving stopped".into()
        } else {
            "Temporary".into()
        };

        let shell_state = if self.session.is_history() {
            "History"
        } else if self.shell.is_none() {
            "App input"
        } else if self.document.prompt_ready() {
            "Ready"
        } else {
            "Busy"
        };
        let command_panel = div()
            .id("command-editor")
            .key_context("KeaCommand")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().font_weight(FontWeight::BOLD).child("Composer"))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(shell_state),
                    )
                    .child(div().flex_1())
                    .child(self.control(
                        "run-draft",
                        format!("Run · {}", self.keymap.label(Action::RunShell)),
                        Action::RunShell,
                        cx,
                    ))
                    .child(self.control(
                        "send-draft",
                        format!("Send · {}", self.keymap.label(Action::SendApplication)),
                        Action::SendApplication,
                        cx,
                    )),
            )
            .child(
                Input::new(&self.editor)
                    .flex_1()
                    .min_h(px(72.))
                    .appearance(false)
                    .disabled(!self.session.input_allowed())
                    .bordered(false),
            )
            .child(
                div()
                    .id("completion-list")
                    .max_h(px(100.))
                    .overflow_y_scroll()
                    .child(completions),
            );
        let session_split = div().flex_1().min_h_0().child(
            v_resizable("terminal-command-split")
                .child(resizable_panel().child(output))
                .child(
                    resizable_panel()
                        .size(px(205.))
                        .size_range(px(120.)..px(520.))
                        .child(command_panel),
                ),
        );

        let copy_view_label = if self.show_blocks && !self.session.is_history() {
            "Copy command history"
        } else {
            "Copy visible terminal"
        };
        let mut toolbar = div()
            .h(px(36.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                button("focus-terminal", "Terminal")
                    .on_click(cx.listener(|this, _, window, _| window.focus(&this.focus))),
            )
            .child(
                button("focus-input", "Composer")
                    .on_click(cx.listener(|this, _, window, cx| this.focus_editor(window, cx))),
            )
            .child(self.icon_control(
                "blocks",
                if self.show_blocks {
                    IconName::PanelRightClose
                } else {
                    IconName::PanelRightOpen
                },
                if self.show_blocks {
                    "Hide command history"
                } else {
                    "Show command history"
                },
                Action::ToggleBlocks,
                cx,
            ))
            .child(self.icon_control(
                "copy-document",
                IconName::Copy,
                copy_view_label,
                Action::CopyDocument,
                cx,
            ))
            .child(div().flex_1())
            .child(
                button(
                    "save-session",
                    if self.session.persistence_active() {
                        "Saved"
                    } else {
                        "Save session"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| this.save_session(cx))),
            );
        if self.session.is_history() {
            toolbar = toolbar
                .child(self.icon_control(
                    "previous",
                    IconName::ArrowLeft,
                    "Previous history event",
                    Action::PreviousEvent,
                    cx,
                ))
                .child(self.icon_control(
                    "next",
                    IconName::ArrowRight,
                    "Next history event",
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
                .child(self.control("live", "Return live", Action::GoLive, cx));
        } else if self.session.recording().events().len() > 1 {
            toolbar = toolbar.child(button("history", "History").on_click(cx.listener(
                |this, _, window, cx| {
                    let result = this.session.step(-1);
                    window.focus(&this.focus);
                    this.result(result, cx);
                },
            )));
        }

        let timeline_weak = cx.entity().downgrade();
        let timeline_hovered = self.timeline_hovered;
        let timeline_bar_color = cx.theme().slider_bar;
        let timeline_progress_color = cx.theme().primary;
        let timeline_thumb_color = cx.theme().slider_thumb;
        let timeline_ring_color = cx.theme().ring;
        let timeline_position = format!(
            "{} / {}",
            playback::format_micros(position),
            playback::format_micros(duration)
        );
        let history_timeline = div()
            .h(px(34.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::BOLD).child("History"))
            .child(
                div()
                    .id("timeline")
                    .flex_1()
                    .h(px(26.))
                    .cursor_pointer()
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        this.timeline_hovered = *hovered;
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.seek_x(event.position.x, window, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.seek_x(event.position.x, window, cx);
                        }
                    }))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let bounds = playback_track_bounds(bounds);
                                let _ = timeline_weak.update(cx, |this, _| {
                                    this.timeline_track_bounds = Some(bounds);
                                });
                            },
                            move |bounds, _, window, _| {
                                let track = playback_track_bounds(bounds);
                                window.paint_quad(
                                    fill(track, timeline_bar_color).corner_radii(px(3.)),
                                );
                                if fraction > 0.0 {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                track.origin,
                                                size(
                                                    track.size.width * fraction,
                                                    track.size.height,
                                                ),
                                            ),
                                            timeline_progress_color,
                                        )
                                        .corner_radii(px(3.)),
                                    );
                                }
                                let diameter = if timeline_hovered { 16.0 } else { 14.0 };
                                let radius = diameter / 2.0;
                                let center = track.origin
                                    + point(track.size.width * fraction, track.size.height / 2.0);
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            center - point(px(radius), px(radius)),
                                            size(px(diameter), px(diameter)),
                                        ),
                                        timeline_thumb_color,
                                    )
                                    .corner_radii(px(radius))
                                    .border_widths(px(2.))
                                    .border_color(timeline_ring_color),
                                );
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(div().w(px(145.)).flex_shrink_0().child(timeline_position));

        let status_bar = div()
            .h(px(24.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .px_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().min_w_0().overflow_hidden().child(directory))
            .child(div().flex_1())
            .child(status)
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(persistence_status),
            );

        let mut workspace = div()
            .id("workspace")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(session_split);
        if self.session.is_history() {
            workspace = workspace.child(history_timeline);
        }
        workspace = workspace.child(status_bar);

        div()
            .id("kea")
            .size_full()
            .flex()
            .flex_col()
            .key_context("Kea")
            .on_action(cx.listener(Self::invoke))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(cx.theme().mono_font_size)
            .font_family(cx.theme().mono_font_family.clone())
            .child(TitleBar::new().child(div().font_weight(FontWeight::BOLD).child("Kea")))
            .child(workspace)
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
    terminal_font.features = FontFeatures::disable_ligatures();

    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&terminal_font);
    let cell_width = text_system
        .advance(font_id, font_size, 'M')
        .map(|advance| advance.width)
        .unwrap_or_else(|_| px(f32::from(font_size) * FALLBACK_CELL_WIDTH_EM));
    let line_height = px((f32::from(text_system.ascent(font_id, font_size)).abs()
        + f32::from(text_system.descent(font_id, font_size)).abs())
    .max(1.));

    TerminalFontMetrics {
        font: terminal_font,
        font_size,
        cell_width,
        line_height,
    }
}

fn playback_track_bounds(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let horizontal_inset = 8.0;
    let track_height = 6.0;
    let width = (f32::from(bounds.size.width) - horizontal_inset * 2.0).max(1.0);
    let top = ((f32::from(bounds.size.height) - track_height) / 2.0).max(0.0);
    Bounds::new(
        bounds.origin + point(px(horizontal_inset), px(top)),
        size(px(width), px(track_height)),
    )
}

fn terminal_point(
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    metrics: &TerminalFontMetrics,
    terminal_size: kea_core::Size,
) -> Option<TerminalPoint> {
    let bounds = bounds?;
    let cell_width = f32::from(metrics.cell_width);
    let line_height = f32::from(metrics.line_height);
    if cell_width <= 0. || line_height <= 0. {
        return None;
    }
    let column = ((f32::from(position.x) - f32::from(bounds.origin.x)) / cell_width)
        .floor()
        .max(0.) as usize;
    let row = ((f32::from(position.y) - f32::from(bounds.origin.y)) / line_height)
        .floor()
        .max(0.) as usize;
    Some(TerminalPoint {
        row: row.min(usize::from(terminal_size.rows).saturating_sub(1)),
        column: column.min(usize::from(terminal_size.columns).saturating_sub(1)),
    })
}

fn terminal_scroll_lines(delta: ScrollDelta, line_height: Pixels, remainder: &mut f32) -> i32 {
    terminal_scroll_units(delta, line_height, remainder, MAX_SCROLL_LINES_PER_EVENT)
}

fn terminal_scroll_units(
    delta: ScrollDelta,
    line_height: Pixels,
    remainder: &mut f32,
    maximum: i32,
) -> i32 {
    let line_height = f32::from(line_height).max(1.);
    let delta = match delta {
        ScrollDelta::Pixels(delta) => f32::from(delta.y) / line_height,
        ScrollDelta::Lines(delta) => delta.y,
    };
    terminal_mouse::accumulate_wheel_delta(delta, remainder, maximum)
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
        let background = if cell.selected {
            TERMINAL_SELECTION_BACKGROUND
        } else {
            cell.background
        };
        if background != 0x11151a {
            window.paint_quad(fill(
                Bounds::new(origin, size(metrics.cell_width, metrics.line_height)),
                rgb(background),
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
            color: rgb(if cell.selected {
                TERMINAL_SELECTION_FOREGROUND
            } else {
                cell.foreground
            })
            .into(),
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
                rgb(if cell.selected {
                    TERMINAL_SELECTION_FOREGROUND
                } else {
                    cell.foreground
                }),
            ));
        }
    }
    if let Some((row, column)) = screen.local_caret {
        let origin =
            bounds.origin + point(px(column as f32 * cell_width), px(row as f32 * line_height));
        if origin.x < bounds.right() && origin.y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(origin, size(px(2.), metrics.line_height)),
                rgb(0xffc857),
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
