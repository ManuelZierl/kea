mod input;
use anyhow::{Context as _, Result};
use gpui::{prelude::*, *};
use kea_alacritty::Screen;
use kea_session::Session;
use std::{ffi::OsString, fs::File, path::PathBuf, time::Duration};

const CELL_WIDTH: f32 = 9.0;
const LINE_HEIGHT: f32 = 20.0;

fn main() {
    if let Err(error) = run() {
        eprintln!("kea: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (mut demo, mut replay, mut record) = (false, None, None);
    let mut command: Vec<OsString> = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--" {
            command.extend(args);
            break;
        }
        if arg == "--demo" {
            demo = true;
        } else if arg == "--replay" {
            replay = Some(PathBuf::from(args.next().context("--replay needs a path")?));
        } else if arg == "--record" {
            record = Some(PathBuf::from(
                args.next().context("--record needs a new file path")?,
            ));
        } else if arg == "--help" || arg == "-h" {
            println!("Kea - a terminal with rewindable screen history\n\nkea [--record NEW.kea] [-- PROGRAM ARG...]\nkea --replay SESSION.kea\nkea --demo\n\nF6/F7: previous/next event | F8: play/pause | F9: live\nShift+F6/F7: back/forward 5 seconds | Ctrl+Shift+C: copy screen\nCtrl+V: paste | Ctrl+Shift+Q: quit\n\nHistory stays in memory unless --record is supplied. Output can contain secrets.");
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
            cx.new(|cx| KeaView::new(session, window, cx))
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
    focus: FocusHandle,
    notice: Option<String>,
    _pump: Task<()>,
}
impl KeaView {
    fn new(session: Session, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            focus,
            notice: None,
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
    fn copy(&self, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(self.session.screen().text()));
    }
    fn seek_x(&mut self, x: Pixels, width: f32, cx: &mut Context<Self>) {
        let fraction = ((f32::from(x) - 12.0) / width).clamp(0.0, 1.0);
        let at = (fraction as f64 * self.session.recording().duration() as f64) as u64;
        let result = self.session.seek_time(at);
        self.result(result, cx);
    }
    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let m = key.modifiers;
        match key.key.as_str() {
            "f6" if m.shift => {
                let r = self
                    .session
                    .seek_time(self.session.position().saturating_sub(5_000_000));
                self.result(r, cx);
            }
            "f7" if m.shift => {
                let r = self
                    .session
                    .seek_time(self.session.position().saturating_add(5_000_000));
                self.result(r, cx);
            }
            "f6" => {
                let r = self.session.step(-1);
                self.result(r, cx);
            }
            "f7" => {
                let r = self.session.step(1);
                self.result(r, cx);
            }
            "f8" => {
                self.session.toggle_playback();
                cx.notify();
            }
            "f9" => {
                self.session.go_live();
                self.notice = None;
                cx.notify();
            }
            "q" if (m.control && m.shift) || m.platform => cx.quit(),
            "c" if (m.control && m.shift) || m.platform => self.copy(cx),
            "v" if m.control || m.platform => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let result = input::paste(&text, self.session.bracketed_paste())
                        .and_then(|bytes| self.session.send(bytes));
                    self.result(result, cx);
                }
            }
            _ => {
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
        let height = (f32::from(viewport.height) - 150.0).max(20.0);
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
        let mode = if self.session.is_history() {
            "HISTORY - READ ONLY"
        } else if self.session.is_running() {
            "LIVE"
        } else {
            "ENDED / REPLAY"
        };
        let status = self
            .session
            .warning
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                "Output history is retained. Raw keystrokes are not recorded. F9 returns to live."
                    .into()
            });
        let footer = format!(
            "{:.3}s / {:.3}s   |   event {} / {}   |   F6/F7 step   F8 play/pause   F9 live",
            position as f64 / 1_000_000.0,
            duration as f64 / 1_000_000.0,
            self.session.end(),
            self.session.recording().events().len()
        );
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
                        button("copy", "Copy screen")
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(div().ml_2().text_color(rgb(0x98c379)).child(mode)),
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
