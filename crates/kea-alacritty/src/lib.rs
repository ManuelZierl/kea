//! Alacritty adapter. Historical engines cannot emit external effects.
use alacritty_terminal::{
    event::{Event as TerminalEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Point as GridPoint, Side},
    selection::{Selection, SelectionType},
    term::{cell::Flags, Config, TermMode},
    vte::ansi::{Color, NamedColor, Processor},
    Term,
};
use kea_core::{Projection, Recording, Size};
use std::sync::{Arc, Mutex};

pub const TERMINAL_SCROLLBACK_LINES: usize = 10_000;

#[derive(Clone, Default)]
struct Listener {
    replies: Option<Arc<Mutex<Vec<String>>>>,
}
impl EventListener for Listener {
    fn send_event(&self, event: TerminalEvent) {
        // No title changes, clipboard access, bell, URL opening, or PTY resize
        // requests are executed. Only live terminal protocol replies are allowed.
        if let (Some(replies), TerminalEvent::PtyWrite(text)) = (&self.replies, event) {
            let mut replies = replies.lock().unwrap_or_else(|e| e.into_inner());
            if replies.len() < 256 {
                replies.push(text);
            }
        }
    }
}

struct Geometry(Size);
impl Dimensions for Geometry {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }
    fn screen_lines(&self) -> usize {
        usize::from(self.0.rows)
    }
    fn columns(&self) -> usize {
        usize::from(self.0.columns)
    }
}

pub struct Engine {
    terminal: Term<Listener>,
    parser: Processor,
    replies: Option<Arc<Mutex<Vec<String>>>>,
    size: Size,
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub text: String,
    pub foreground: u32,
    pub background: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub wide: bool,
    pub spacer: bool,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct Screen {
    pub size: Size,
    pub cells: Vec<Cell>,
    pub cursor: Option<(usize, usize)>,
    pub display_offset: usize,
    pub history_size: usize,
}
impl Screen {
    pub fn text(&self) -> String {
        self.cells
            .chunks(usize::from(self.size.columns))
            .map(|row| {
                row.iter()
                    .filter(|c| !c.spacer)
                    .map(|c| c.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalPoint {
    pub row: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseEncoding {
    Legacy,
    Utf8,
    Sgr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseTracking {
    Click,
    Drag,
    Motion,
}

impl Engine {
    pub fn new(size: Size, live: bool) -> Self {
        let replies = live.then(|| Arc::new(Mutex::new(Vec::new())));
        let listener = Listener {
            replies: replies.clone(),
        };
        let config = Config {
            scrolling_history: TERMINAL_SCROLLBACK_LINES,
            // Let applications explicitly negotiate the Kitty/CSI-u keyboard protocol.
            // Classic encoding remains authoritative until a mode is actually enabled.
            kitty_keyboard: true,
            ..Config::default()
        };
        Self {
            terminal: Term::new(config, &Geometry(size), listener),
            parser: Processor::new(),
            replies,
            size,
        }
    }
    pub fn at(recording: &Recording, end: usize) -> std::io::Result<Self> {
        let mut engine = Self::new(recording.initial_size(), false);
        kea_core::replay(recording, end, &mut engine)?;
        Ok(engine)
    }
    pub fn size(&self) -> Size {
        self.size
    }
    pub fn application_cursor(&self) -> bool {
        self.terminal.mode().contains(TermMode::APP_CURSOR)
    }
    pub fn bracketed_paste(&self) -> bool {
        self.terminal.mode().contains(TermMode::BRACKETED_PASTE)
    }
    pub fn extended_keyboard(&self) -> bool {
        self.terminal
            .mode()
            .intersects(TermMode::KITTY_KEYBOARD_PROTOCOL)
    }
    pub fn focus_reporting(&self) -> bool {
        self.terminal.mode().contains(TermMode::FOCUS_IN_OUT)
    }
    pub fn mouse_tracking(&self) -> Option<MouseTracking> {
        let mode = self.terminal.mode();
        if mode.contains(TermMode::MOUSE_MOTION) {
            Some(MouseTracking::Motion)
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            Some(MouseTracking::Drag)
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            Some(MouseTracking::Click)
        } else {
            None
        }
    }
    pub fn mouse_reporting(&self) -> bool {
        self.mouse_tracking().is_some()
    }
    pub fn mouse_encoding(&self) -> Option<MouseEncoding> {
        let mode = self.terminal.mode();
        if self.mouse_tracking().is_none() {
            None
        } else if mode.contains(TermMode::SGR_MOUSE) {
            Some(MouseEncoding::Sgr)
        } else if mode.contains(TermMode::UTF8_MOUSE) {
            Some(MouseEncoding::Utf8)
        } else {
            Some(MouseEncoding::Legacy)
        }
    }
    pub fn display_offset(&self) -> usize {
        self.terminal.grid().display_offset()
    }
    pub fn history_size(&self) -> usize {
        self.terminal.grid().history_size()
    }
    pub fn scroll_lines(&mut self, lines: i32) {
        self.terminal.scroll_display(Scroll::Delta(lines));
    }
    pub fn scroll_bottom(&mut self) {
        self.terminal.scroll_display(Scroll::Bottom);
    }
    pub fn begin_selection(&mut self, point: TerminalPoint) {
        let point = self.grid_point(point);
        self.terminal.selection = Some(Selection::new(SelectionType::Simple, point, Side::Left));
    }
    pub fn update_selection(&mut self, point: TerminalPoint) {
        let point = self.grid_point(point);
        if let Some(selection) = &mut self.terminal.selection {
            selection.update(point, Side::Right);
            selection.include_all();
        }
    }
    pub fn clear_selection(&mut self) {
        self.terminal.selection = None;
    }
    pub fn selection_text(&self) -> Option<String> {
        self.terminal
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }
    pub fn has_selection(&self) -> bool {
        self.terminal
            .selection
            .as_ref()
            .and_then(|selection| selection.to_range(&self.terminal))
            .is_some()
    }
    fn grid_point(&self, point: TerminalPoint) -> GridPoint {
        let row = point.row.min(usize::from(self.size.rows).saturating_sub(1));
        let column = point
            .column
            .min(usize::from(self.size.columns).saturating_sub(1));
        alacritty_terminal::term::viewport_to_point(
            self.display_offset(),
            GridPoint::new(row, Column(column)),
        )
    }
    pub fn drain_replies(&mut self) -> Vec<String> {
        self.replies
            .as_ref()
            .map(|q| std::mem::take(&mut *q.lock().unwrap_or_else(|e| e.into_inner())))
            .unwrap_or_default()
    }
    pub fn screen(&self) -> Screen {
        let history_size = self.history_size();
        let content = self.terminal.renderable_content();
        let selection = content.selection;
        let cursor_shape = content.cursor.shape;
        let cursor_point = content.cursor.point;
        let display_offset = content.display_offset;
        let mut cells =
            Vec::with_capacity(usize::from(self.size.rows) * usize::from(self.size.columns));
        for indexed in content.display_iter {
            let cell = indexed.cell;
            let mut text = cell.c.to_string();
            if let Some(extra) = cell.zerowidth() {
                text.extend(extra);
            }
            let mut foreground = self.color(cell.fg);
            let mut background = self.color(cell.bg);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut foreground, &mut background);
            }
            if cell.flags.contains(Flags::HIDDEN) {
                foreground = background;
            }
            cells.push(Cell {
                text,
                foreground,
                background,
                bold: cell.flags.contains(Flags::BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.contains(Flags::UNDERLINE),
                wide: cell.flags.contains(Flags::WIDE_CHAR),
                spacer: cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
                selected: selection.is_some_and(|selection| {
                    selection.contains_cell(&indexed, cursor_point, cursor_shape)
                }),
            });
        }
        let cursor = (display_offset == 0 && self.terminal.mode().contains(TermMode::SHOW_CURSOR))
            .then(|| {
                let point = self.terminal.grid().cursor.point;
                (point.line.0.max(0) as usize, point.column.0)
            });
        Screen {
            size: self.size,
            cells,
            cursor,
            display_offset,
            history_size,
        }
    }
    fn color(&self, color: Color) -> u32 {
        match color {
            Color::Spec(c) => (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b),
            Color::Named(c) => self.indexed(c as usize),
            Color::Indexed(c) => self.indexed(usize::from(c)),
        }
    }
    fn indexed(&self, index: usize) -> u32 {
        if let Some(c) = self.terminal.colors()[index] {
            return (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b);
        }
        const ANSI: [u32; 16] = [
            0x171b20, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd7dae0,
            0x5c6370, 0xff7a85, 0xb5e890, 0xffdb96, 0x82cfff, 0xe9a0ff, 0x7de0ec, 0xffffff,
        ];
        match index {
            0..=15 => ANSI[index],
            16..=231 => {
                let n = index - 16;
                let channel = |v: usize| if v == 0 { 0 } else { 55 + 40 * v } as u32;
                (channel(n / 36) << 16) | (channel(n / 6 % 6) << 8) | channel(n % 6)
            }
            232..=255 => {
                let v = (8 + 10 * (index - 232)) as u32;
                (v << 16) | (v << 8) | v
            }
            n if n == NamedColor::Background as usize => 0x11151a,
            _ => 0xd7dae0,
        }
    }
}
impl Projection for Engine {
    fn output(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
    }
    fn resize(&mut self, size: Size) {
        self.terminal.resize(Geometry(size));
        self.size = size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kea_core::Kind;
    fn log() -> Recording {
        Recording::new(Size::new(40, 6).unwrap()).unwrap()
    }
    #[test]
    fn overwritten_error_is_recoverable() {
        let mut log = log();
        log.append(1, Kind::Output(b"ERROR: connection failed".to_vec()))
            .unwrap();
        log.append(2, Kind::Output(b"\r\x1b[2KReady".to_vec()))
            .unwrap();
        assert!(Engine::at(&log, 1)
            .unwrap()
            .screen()
            .text()
            .contains("ERROR"));
        assert!(!Engine::at(&log, 2)
            .unwrap()
            .screen()
            .text()
            .contains("ERROR"));
    }
    #[test]
    fn alternate_screen_is_preserved_in_history() {
        let mut log = log();
        log.append(1, Kind::Output(b"shell".to_vec())).unwrap();
        log.append(2, Kind::Output(b"\x1b[?1049h\x1b[Htransient".to_vec()))
            .unwrap();
        log.append(3, Kind::Output(b"\x1b[?1049l".to_vec()))
            .unwrap();
        assert!(Engine::at(&log, 2)
            .unwrap()
            .screen()
            .text()
            .contains("transient"));
        assert!(Engine::at(&log, 3)
            .unwrap()
            .screen()
            .text()
            .contains("shell"));
    }
    #[test]
    fn split_utf8_and_escape_sequences_survive_record_boundaries() {
        let mut log = log();
        for (i, byte) in "\x1b[31mKea 🦜\x1b[0m".as_bytes().iter().enumerate() {
            log.append(i as u64, Kind::Output(vec![*byte])).unwrap();
        }
        assert!(Engine::at(&log, log.events().len())
            .unwrap()
            .screen()
            .text()
            .contains("Kea 🦜"));
    }
    #[test]
    fn replay_is_silent_and_resize_is_replayed() {
        let mut log = log();
        log.append(1, Kind::Resize(Size::new(20, 4).unwrap()))
            .unwrap();
        log.append(2, Kind::Output(b"\x1b[6n\x1b]52;c;YQ==\x07".to_vec()))
            .unwrap();
        let mut replay = Engine::at(&log, 2).unwrap();
        assert!(replay.drain_replies().is_empty());
        assert_eq!(replay.screen().size, Size::new(20, 4).unwrap());
        let mut live = Engine::new(log.initial_size(), true);
        live.output(b"\x1b[6n");
        assert!(!live.drain_replies().is_empty());
    }

    #[test]
    fn scrollback_projects_the_selected_viewport_and_returns_to_tail() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"one\r\ntwo\r\nthree\r\nfour");

        assert_eq!(engine.display_offset(), 0);
        assert!(engine.history_size() > 0);
        assert!(engine.screen().text().contains("four"));

        engine.scroll_lines(2);
        let scrolled = engine.screen();
        assert!(scrolled.display_offset > 0);
        assert!(scrolled.text().contains("one"));

        engine.scroll_bottom();
        assert_eq!(engine.display_offset(), 0);
        assert!(engine.screen().text().contains("four"));
    }

    #[test]
    fn terminal_selection_uses_viewport_coordinates_and_copies_only_selection() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"hello");
        engine.begin_selection(TerminalPoint { row: 0, column: 1 });
        engine.update_selection(TerminalPoint { row: 0, column: 3 });

        assert_eq!(engine.selection_text().as_deref(), Some("ell"));
        let screen = engine.screen();
        let selected = screen
            .cells
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| cell.selected.then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(selected, vec![1, 2, 3]);

        engine.begin_selection(TerminalPoint { row: 0, column: 3 });
        engine.update_selection(TerminalPoint { row: 0, column: 1 });
        assert_eq!(engine.selection_text().as_deref(), Some("ell"));

        engine.clear_selection();
        assert!(engine.selection_text().is_none());
        assert!(engine.screen().cells.iter().all(|cell| !cell.selected));
    }

    #[test]
    fn terminal_selection_works_in_a_mouse_reporting_alternate_screen() {
        let mut engine = Engine::new(Size::new(16, 3).unwrap(), false);
        engine.output(b"\x1b[?1049h\x1b[H\x1b[?1000hOpenCode output");
        assert!(engine.mouse_reporting());

        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 7 });

        assert_eq!(engine.selection_text().as_deref(), Some("OpenCode"));
        assert!(engine.screen().cells.iter().any(|cell| cell.selected));
    }

    #[test]
    fn output_does_not_return_a_scrolled_reader_to_the_tail() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"one\r\ntwo\r\nthree\r\nfour");
        engine.scroll_lines(1);
        let before = engine.display_offset();

        engine.output(b"\r\nfive");

        assert!(engine.display_offset() >= before);
        assert_ne!(engine.display_offset(), 0);
    }

    #[test]
    fn appended_output_preserves_a_scrollback_selection() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"one\r\ntwo\r\nthree\r\nfour");
        engine.scroll_lines(1);
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        let selected = engine.selection_text();

        engine.output(b"\r\nfive");

        assert_eq!(engine.selection_text(), selected);
        assert!(engine.screen().cells.iter().any(|cell| cell.selected));
    }

    #[test]
    fn scrollback_retention_and_mouse_reporting_are_explicitly_bounded() {
        let mut engine = Engine::new(Size::new(4, 2).unwrap(), false);
        for _ in 0..TERMINAL_SCROLLBACK_LINES + 20 {
            engine.output(b"x\r\n");
        }
        assert_eq!(engine.history_size(), TERMINAL_SCROLLBACK_LINES);

        assert_eq!(engine.mouse_encoding(), None);
        engine.output(b"\x1b[?1000h");
        assert_eq!(engine.mouse_tracking(), Some(MouseTracking::Click));
        assert_eq!(engine.mouse_encoding(), Some(MouseEncoding::Legacy));
        engine.output(b"\x1b[?1002h");
        assert_eq!(engine.mouse_tracking(), Some(MouseTracking::Drag));
        engine.output(b"\x1b[?1003h");
        assert_eq!(engine.mouse_tracking(), Some(MouseTracking::Motion));
        engine.output(b"\x1b[?1005h");
        assert_eq!(engine.mouse_encoding(), Some(MouseEncoding::Utf8));
        engine.output(b"\x1b[?1006h");
        assert_eq!(engine.mouse_encoding(), Some(MouseEncoding::Sgr));
        engine.output(b"\x1b[?1003l\x1b[?1002l\x1b[?1000l");
        assert_eq!(engine.mouse_encoding(), None);
    }

    #[test]
    fn focus_reporting_mode_is_tracked_by_the_terminal_engine() {
        let mut engine = Engine::new(Size::new(4, 2).unwrap(), false);
        assert!(!engine.focus_reporting());
        engine.output(b"\x1b[?1004h");
        assert!(engine.focus_reporting());
        engine.output(b"\x1b[?1004l");
        assert!(!engine.focus_reporting());
    }
}
