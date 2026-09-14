//! Alacritty adapter. Historical engines cannot emit external effects.
use std::sync::{Arc, Mutex};
use alacritty_terminal::{
    event::{Event as TerminalEvent, EventListener},
    grid::Dimensions,
    index::{Column, Line},
    term::{cell::Flags, Config, TermMode},
    vte::ansi::{Color, NamedColor, Processor},
    Term,
};
use kea_core::{Projection, Recording, Size};

#[derive(Clone, Default)]
struct Listener { replies: Option<Arc<Mutex<Vec<String>>>> }
impl EventListener for Listener {
    fn send_event(&self, event: TerminalEvent) {
        // No title changes, clipboard access, bell, URL opening, or PTY resize
        // requests are executed. Only live terminal protocol replies are allowed.
        if let (Some(replies), TerminalEvent::PtyWrite(text)) = (&self.replies, event) {
            let mut replies = replies.lock().unwrap_or_else(|e| e.into_inner());
            if replies.len() < 256 { replies.push(text); }
        }
    }
}

struct Geometry(Size);
impl Dimensions for Geometry {
    fn total_lines(&self) -> usize { self.screen_lines() }
    fn screen_lines(&self) -> usize { usize::from(self.0.rows) }
    fn columns(&self) -> usize { usize::from(self.0.columns) }
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
}

#[derive(Clone, Debug)]
pub struct Screen {
    pub size: Size,
    pub cells: Vec<Cell>,
    pub cursor: Option<(usize, usize)>,
}
impl Screen {
    pub fn text(&self) -> String {
        self.cells.chunks(usize::from(self.size.columns)).map(|row| {
            row.iter().filter(|c| !c.spacer).map(|c| c.text.as_str()).collect::<String>().trim_end().to_string()
        }).collect::<Vec<_>>().join("\n")
    }
}

impl Engine {
    pub fn new(size: Size, live: bool) -> Self {
        let replies = live.then(|| Arc::new(Mutex::new(Vec::new())));
        let listener = Listener { replies: replies.clone() };
        let config = Config { scrolling_history: 0, ..Config::default() };
        Self { terminal: Term::new(config, &Geometry(size), listener), parser: Processor::new(), replies, size }
    }
    pub fn at(recording: &Recording, end: usize) -> std::io::Result<Self> {
        let mut engine = Self::new(recording.initial_size(), false);
        kea_core::replay(recording, end, &mut engine)?;
        Ok(engine)
    }
    pub fn size(&self) -> Size { self.size }
    pub fn application_cursor(&self) -> bool { self.terminal.mode().contains(TermMode::APP_CURSOR) }
    pub fn bracketed_paste(&self) -> bool { self.terminal.mode().contains(TermMode::BRACKETED_PASTE) }
    pub fn drain_replies(&mut self) -> Vec<String> {
        self.replies.as_ref().map(|q| std::mem::take(&mut *q.lock().unwrap_or_else(|e| e.into_inner()))).unwrap_or_default()
    }
    pub fn screen(&self) -> Screen {
        let mut cells = Vec::with_capacity(usize::from(self.size.rows) * usize::from(self.size.columns));
        for row in 0..self.size.rows {
            for col in 0..self.size.columns {
                let cell = &self.terminal.grid()[Line(i32::from(row))][Column(usize::from(col))];
                let mut text = cell.c.to_string();
                if let Some(extra) = cell.zerowidth() { text.extend(extra); }
                let mut foreground = self.color(cell.fg);
                let mut background = self.color(cell.bg);
                if cell.flags.contains(Flags::INVERSE) { std::mem::swap(&mut foreground, &mut background); }
                if cell.flags.contains(Flags::HIDDEN) { foreground = background; }
                cells.push(Cell {
                    text, foreground, background,
                    bold: cell.flags.contains(Flags::BOLD),
                    italic: cell.flags.contains(Flags::ITALIC),
                    underline: cell.flags.contains(Flags::UNDERLINE),
                    wide: cell.flags.contains(Flags::WIDE_CHAR),
                    spacer: cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
                });
            }
        }
        let point = self.terminal.grid().cursor.point;
        let cursor = self.terminal.mode().contains(TermMode::SHOW_CURSOR).then_some((point.line.0.max(0) as usize, point.column.0));
        Screen { size: self.size, cells, cursor }
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
        const ANSI: [u32; 16] = [0x171b20,0xe06c75,0x98c379,0xe5c07b,0x61afef,0xc678dd,0x56b6c2,0xd7dae0,0x5c6370,0xff7a85,0xb5e890,0xffdb96,0x82cfff,0xe9a0ff,0x7de0ec,0xffffff];
        match index {
            0..=15 => ANSI[index],
            16..=231 => {
                let n = index - 16;
                let channel = |v: usize| if v == 0 { 0 } else { 55 + 40 * v } as u32;
                (channel(n / 36) << 16) | (channel(n / 6 % 6) << 8) | channel(n % 6)
            }
            232..=255 => { let v = (8 + 10 * (index - 232)) as u32; (v << 16) | (v << 8) | v }
            n if n == NamedColor::Background as usize => 0x11151a,
            _ => 0xd7dae0,
        }
    }
}
impl Projection for Engine {
    fn output(&mut self, bytes: &[u8]) { self.parser.advance(&mut self.terminal, bytes); }
    fn resize(&mut self, size: Size) { self.terminal.resize(Geometry(size)); self.size = size; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kea_core::Kind;
    fn log() -> Recording { Recording::new(Size::new(40, 6).unwrap()).unwrap() }
    #[test]
    fn overwritten_error_is_recoverable() {
        let mut log = log();
        log.append(1, Kind::Output(b"ERROR: connection failed".to_vec())).unwrap();
        log.append(2, Kind::Output(b"\r\x1b[2KReady".to_vec())).unwrap();
        assert!(Engine::at(&log, 1).unwrap().screen().text().contains("ERROR"));
        assert!(!Engine::at(&log, 2).unwrap().screen().text().contains("ERROR"));
    }
    #[test]
    fn alternate_screen_is_preserved_in_history() {
        let mut log = log();
        log.append(1, Kind::Output(b"shell".to_vec())).unwrap();
        log.append(2, Kind::Output(b"\x1b[?1049h\x1b[Htransient".to_vec())).unwrap();
        log.append(3, Kind::Output(b"\x1b[?1049l".to_vec())).unwrap();
        assert!(Engine::at(&log, 2).unwrap().screen().text().contains("transient"));
        assert!(Engine::at(&log, 3).unwrap().screen().text().contains("shell"));
    }
    #[test]
    fn split_utf8_and_escape_sequences_survive_record_boundaries() {
        let mut log = log();
        for (i, byte) in "\x1b[31mKea 🦜\x1b[0m".as_bytes().iter().enumerate() {
            log.append(i as u64, Kind::Output(vec![*byte])).unwrap();
        }
        assert!(Engine::at(&log, log.events().len()).unwrap().screen().text().contains("Kea 🦜"));
    }
    #[test]
    fn replay_is_silent_and_resize_is_replayed() {
        let mut log = log();
        log.append(1, Kind::Resize(Size::new(20, 4).unwrap())).unwrap();
        log.append(2, Kind::Output(b"\x1b[6n\x1b]52;c;YQ==\x07".to_vec())).unwrap();
        let mut replay = Engine::at(&log, 2).unwrap();
        assert!(replay.drain_replies().is_empty());
        assert_eq!(replay.screen().size, Size::new(20, 4).unwrap());
        let mut live = Engine::new(log.initial_size(), true);
        live.output(b"\x1b[6n");
        assert!(!live.drain_replies().is_empty());
    }
}
