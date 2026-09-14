mod command_editor;
mod input;
mod keybindings;

use anyhow::{Context as _, Result};
use command_editor::CommandEditor;
use gpui::{prelude::*, *};
use kea_alacritty::Screen;
use kea_session::Session;
use keybindings::{Action, Keymap};
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
            println!(
                "Kea - a document-native terminal\n\n\
kea [--direct] [--record NEW.kea] [-- PROGRAM ARG...]\n\
kea --replay SESSION.kea\n\
kea --demo\n\n\
Document mode: Enter inserts a newline; Ctrl+Enter executes the whole input block.\n\
Ctrl+Shift+Space toggles Direct PTY mode for TUIs and REPLs.\n\
Linux/Windows defaults: Ctrl+C copy, Ctrl+V paste, Ctrl+Shift+C interrupt.\n\
macOS defaults: Cmd+C/V copy/paste, Ctrl+C interrupt.\n\
F6/F7 step history | F8 play/pause | F9 live.\n\n\
All shortcuts are configurable in Kea's keybindings.conf. Set KEA_KEYBINDINGS to use an explicit path.\n\
History stays in memory unless --record is supplied. Output can contain secrets."
            );
            return Ok(());
        } else if arg.to_string_lossy().starts_with('-') {
            anyhow::bail!("unknown option: {}", arg.to_string_lossy());
        } else {
            command.push(arg);
            command.extend(args);
            break;
        }
    }
    if (demo && replay.is_some())
        || ((demo || replay.is_some()) && (record.is_some() || !command.is_empty()))
    {
        anyhow::bail!(
            "--demo and --replay cannot be combined with each other, --record, or a command"
        );
    }
    let session = if demo {
        Session::demo()?
    } else if let Some(path) = replay {
        let loaded = kea_core::read_from(File::open(path)?)?;
        let mut session = Session::from_recording(loaded.recording)?;
        if loaded.truncated_tail {
            session.warning =
                Some("Recovered complete events; the final recording frame was truncated.".into());
        }
        session
    } else {
        Session::spawn(&command, kea_core::Size::new(100, 26)?, record.as_deref())?
    };
    let (keymap, keymap_warning) = Keymap::load();
    let initial_mode = if direct {
        InputMode::Direct
    } else {
        InputMode::Document
    };
    Application::new().run(move |cx: &mut App| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1000.0), px(700.0)),
                cx,
            ))),
            window_min_size: Some(size(px(760.0), px(420.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("Kea".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, |window, cx| {
            cx.new(|cx| KeaView::new(session, keymap, keymap_warning, initial_mode, window, cx))
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
    keymap: Keymap,
    input_mode: InputMode,
    editor: CommandEditor,
    submitted_commands: usize,
    focus: FocusHandle,
    notice: Option<String>,
    _pump: Task<()>,
}

impl KeaView {
    fn new(
        session: Session,
        keymap: Keymap,
        keymap_warning: Option<String>,
        input_mode: InputMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let pump = cx.spawn(async move |this, cx| loop {
            Timer::after(Duration::from_millis(16)).await;
            if this
                .update(cx, |this, cx| {
                    if this.session.pump() {
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
            keymap,
            input_mode,
            editor: CommandEditor::default(),
            submitted_commands: 0,
            focus,
            notice: keymap_warning,
            _pump: pump,
        }
    }

    fn result(&mut self, result: Result<()>, cx: &mut Context<Self>) {
        self.notice = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn previous(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.session.step(-1);
        self.result(result, cx);
    }

    fn next(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.session.step(1);
        self.result(result, cx);
    }

    fn play(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.session.toggle_playback();
        cx.notify();
    }

    fn live(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.session.go_live();
        self.notice = None;
        cx.notify();
    }

    fn toggle_input_mode(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_direct(cx);
    }

    fn copy(&self, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.session.screen().text()));
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if self.input_mode == InputMode::Document && self.session.input_allowed() {
            self.editor.insert(&text);
            self.notice = None;
            cx.notify();
            return;
        }
        let result = input::paste(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.session.send(bytes));
        self.result(result, cx);
    }

    fn execute_editor(&mut self, cx: &mut Context<Self>) {
        if self.input_mode != InputMode::Document {
            return;
        }
        let text = self.editor.text().to_owned();
        let result = self.session.send(command_editor::terminal_bytes(&text));
        if result.is_ok() {
            self.editor.clear_after_submit();
            self.submitted_commands += 1;
        }
        self.result(result, cx);
    }

    fn interrupt(&mut self, cx: &mut Context<Self>) {
        let result = self.session.send(vec![3]);
        self.result(result, cx);
    }

    fn toggle_direct(&mut self, cx: &mut Context<Self>) {
        self.input_mode = match self.input_mode {
            InputMode::Document => InputMode::Direct,
            InputMode::Direct => InputMode::Document,
        };
        self.notice = None;
        cx.notify();
    }

    fn seek_x(&mut self, x: Pixels, width: f32, cx: &mut Context<Self>) {
        let fraction = ((f32::from(x) - 12.0) / width).clamp(0.0, 1.0);
        let at = (fraction as f64 * self.session.recording().duration() as f64) as u64;
        let result = self.session.seek_time(at);
        self.result(result, cx);
    }

    fn dispatch_action(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Copy => self.copy(cx),
            Action::Paste => self.paste(cx),
            Action::Interrupt => self.interrupt(cx),
            Action::Execute => self.execute_editor(cx),
            Action::ToggleDirect => self.toggle_direct(cx),
            Action::PreviousEvent => {
                let result = self.session.step(-1);
                self.result(result, cx);
            }
            Action::NextEvent => {
                let result = self.session.step(1);
                self.result(result, cx);
            }
            Action::BackFiveSeconds => {
                let result = self
                    .session
                    .seek_time(self.session.position().saturating_sub(5_000_000));
                self.result(result, cx);
            }
            Action::ForwardFiveSeconds => {
                let result = self
                    .session
                    .seek_time(self.session.position().saturating_add(5_000_000));
                self.result(result, cx);
            }
            Action::PlayPause => {
                self.session.toggle_playback();
                cx.notify();
            }
            Action::GoLive => {
                self.session.go_live();
                self.notice = None;
                cx.notify();
            }
            Action::Quit => cx.quit(),
        }
    }

    fn document_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
        let modifiers = key.modifiers;
        match key.key.as_str() {
            "enter" | "return" if !modifiers.control && !modifiers.platform && !modifiers.alt => {
                self.editor.newline();
            }
            "backspace" if !modifiers.control && !modifiers.platform => self.editor.backspace(),
            "delete" if !modifiers.control && !modifiers.platform => self.editor.delete(),
            "left" if !modifiers.control && !modifiers.platform => self.editor.left(),
            "right" if !modifiers.control && !modifiers.platform => self.editor.right(),
            "up" if !modifiers.control && !modifiers.platform => self.editor.up(),
            "down" if !modifiers.control && !modifiers.platform => self.editor.down(),
            "home" if !modifiers.control && !modifiers.platform => self.editor.home(),
            "end" if !modifiers.control && !modifiers.platform => self.editor.end(),
            "tab" if !modifiers.control && !modifiers.platform => self.editor.insert("\t"),
            _ => {
                if key.is_ime_in_progress() || modifiers.platform {
                    return;
                }
                if modifiers.control && !modifiers.alt {
                    return;
                }
                if let Some(text) = key.key_char.as_deref().or_else(|| {
                    (!modifiers.control && key.key.chars().count() == 1).then_some(key.key.as_str())
                }) {
                    self.editor.insert(text);
                } else {
                    return;
                }
            }
        }
        self.notice = None;
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        if let Some(action) = self.keymap.action_for(key) {
            // Ctrl+Enter is Kea's execute action in Document mode. In Direct mode it
            // belongs to the child and must retain its distinct terminal encoding.
            if action != Action::Execute || self.input_mode == InputMode::Document {
                self.dispatch_action(action, cx);
                cx.stop_propagation();
                return;
            }
        }
        if !self.session.input_allowed() {
            cx.stop_propagation();
            return;
        }
        match self.input_mode {
            InputMode::Document => self.document_key(key, cx),
            InputMode::Direct => {
                if let Some(bytes) = input::encode(key, self.session.application_cursor()) {
                    let result = self.session.send(bytes);
                    self.result(result, cx);
                }
            }
        }
        cx.stop_propagation();
    }
}

impl Render for KeaView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 24.0).max(18.0);
        let input_height = if self.session.input_allowed() {
            104.0
        } else {
            0.0
        };
        let height = (f32::from(viewport.height) - 150.0 - input_height).max(20.0);
        let columns = (width / CELL_WIDTH).floor().clamp(2.0, 512.0) as u16;
        let rows = (height / LINE_HEIGHT).floor().clamp(1.0, 256.0) as u16;
        if let Ok(size) = kea_core::Size::new(columns, rows) {
            if let Err(error) = self.session.resize(size) {
                self.notice = Some(error.to_string());
            }
        }
        let screen = self.session.screen();
        let duration = self.session.recording().duration();
        let position = self.session.position();
        let fraction = if duration == 0 {
            0.0
        } else {
            position as f32 / duration as f32
        };
        let terminal_mode = if self.session.is_history() {
            "HISTORY - READ ONLY"
        } else if self.session.is_running() {
            "LIVE"
        } else {
            "ENDED / REPLAY"
        };
        let input_mode = match self.input_mode {
            InputMode::Document => "DOCUMENT INPUT",
            InputMode::Direct => "DIRECT PTY",
        };
        let execute_label = self.keymap.label(Action::Execute);
        let toggle_label = self.keymap.label(Action::ToggleDirect);
        let live_label = self.keymap.label(Action::GoLive);
        let status = self
            .session
            .warning
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.session.input_allowed() && self.input_mode == InputMode::Document {
                    format!(
                        "Editor input is local. Enter = newline; {execute_label} = execute; {toggle_label} = Direct PTY."
                    )
                } else if self.session.input_allowed() {
                    format!(
                        "Direct PTY compatibility mode. {toggle_label} returns to document input."
                    )
                } else {
                    format!("Historical output is read-only. {live_label} returns to live when available.")
                }
            });
        let footer = format!(
            "{:.3}s / {:.3}s   |   event {} / {}   |   commands submitted {}",
            position as f64 / 1_000_000.0,
            duration as f64 / 1_000_000.0,
            self.session.end(),
            self.session.recording().events().len(),
            self.submitted_commands
        );
        let editor_display = if self.editor.is_empty() {
            "▏".to_string()
        } else {
            self.editor.rendered()
        };
        let input_panel = if self.session.input_allowed() {
            let body = match self.input_mode {
                InputMode::Document => editor_display,
                InputMode::Direct => format!(
                    "Keystrokes are going directly to the PTY. Press {toggle_label} to return to the document editor."
                ),
            };
            div()
                .h(px(input_height))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(rgb(0x171d24))
                .border_1()
                .border_color(rgb(0x303844))
                .child(div().text_color(rgb(0x98c379)).child(format!(
                    "{input_mode}   ·   {execute_label} execute   ·   Enter newline"
                )))
                .child(div().flex_1().overflow_hidden().child(body))
        } else {
            div().h(px(0.0)).flex_shrink_0()
        };
        div()
            .id("kea")
            .size_full()
            .flex()
            .flex_col()
            .p_3()
            .gap_2()
            .bg(rgb(0x11151a))
            .text_color(rgb(0xd7dae0))
            .text_size(px(13.0))
            .font_family("monospace")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| window.focus(&this.focus)),
            )
            .child(
                div()
                    .h(px(32.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().font_weight(FontWeight::BOLD).child("KEA"))
                    .child(button("previous", "< F6").on_click(cx.listener(Self::previous)))
                    .child(button("next", "F7 >").on_click(cx.listener(Self::next)))
                    .child(
                        button(
                            "play",
                            if self.session.is_playing() {
                                "Pause F8"
                            } else {
                                "Play F8"
                            },
                        )
                        .on_click(cx.listener(Self::play)),
                    )
                    .child(button("live", "LIVE F9").on_click(cx.listener(Self::live)))
                    .child(
                        button(
                            "input-mode",
                            if self.input_mode == InputMode::Document {
                                "DOCUMENT"
                            } else {
                                "DIRECT PTY"
                            },
                        )
                        .on_click(cx.listener(Self::toggle_input_mode)),
                    )
                    .child(
                        button("copy", "Copy screen")
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(div().ml_2().text_color(rgb(0x98c379)).child(terminal_mode)),
            )
            .child(
                div()
                    .h(px(20.0))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(status),
            )
            .child(
                div().flex_1().min_h_0().overflow_hidden().child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| paint_screen(&screen, bounds, window, cx),
                    )
                    .size_full(),
                ),
            )
            .child(input_panel)
            .child(
                div()
                    .id("timeline")
                    .h(px(14.0))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.seek_x(event.position.x, width, cx)
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.seek_x(event.position.x, width, cx);
                        }
                    }))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                window.paint_quad(fill(bounds, rgb(0x303844)));
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
            .child(div().h(px(20.0)).flex_shrink_0().child(footer))
    }
}

fn button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(0x252d37))
        .cursor_pointer()
        .child(label)
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
                .shape_line(cell.text.clone().into(), px(14.0), &[run], None);
        let _ = shaped.paint(origin, px(LINE_HEIGHT), window, cx);
        if cell.underline {
            window.paint_quad(fill(
                Bounds::new(
                    origin + point(px(0.0), px(LINE_HEIGHT - 2.0)),
                    size(
                        px(if cell.wide {
                            CELL_WIDTH * 2.0
                        } else {
                            CELL_WIDTH
                        }),
                        px(1.0),
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
                px(row as f32 * LINE_HEIGHT + LINE_HEIGHT - 2.0),
            );
        if origin.x < bounds.right() && origin.y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(origin, size(px(CELL_WIDTH), px(2.0))),
                rgb(0x61afef),
            ));
        }
    }
}
