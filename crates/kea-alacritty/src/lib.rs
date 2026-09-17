//! Alacritty adapter. Historical engines cannot emit external effects.
use alacritty_terminal::{
    event::{Event as TerminalEvent, EventListener},
    grid::{Dimensions, Scroll},
    index::{Column, Direction, Point as GridPoint, Side},
    selection::{Selection, SelectionType},
    term::{
        cell::{Flags, LineLength},
        Config, TermMode,
    },
    vi_mode::{ViModeCursor, ViMotion},
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
    selection_anchor: Option<GridPoint>,
    selection_head: Option<GridPoint>,
    selection_block: bool,
    selection_explicit: bool,
    selection_invalidated: bool,
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
    pub local_caret: Option<(usize, usize)>,
    pub selection_invalidated: bool,
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
pub enum SelectionMotion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
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
            selection_anchor: None,
            selection_head: None,
            selection_block: false,
            selection_explicit: false,
            selection_invalidated: false,
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
        let had_selection = self.has_selection();
        self.terminal.scroll_display(Scroll::Delta(lines));
        self.reconcile_selection(had_selection, None);
    }
    pub fn scroll_bottom(&mut self) {
        let had_selection = self.has_selection();
        self.terminal.scroll_display(Scroll::Bottom);
        self.reconcile_selection(had_selection, None);
    }
    pub fn begin_selection(&mut self, point: TerminalPoint) {
        self.begin_selection_kind(point, false);
    }
    pub fn update_selection(&mut self, point: TerminalPoint) {
        self.extend_selection(point);
    }
    pub fn extend_selection(&mut self, point: TerminalPoint) {
        let point = self.grid_point(point);
        let anchor = self.selection_anchor.or(self.selection_head);
        if let Some(anchor) = anchor {
            self.selection_anchor = Some(anchor);
            self.selection_head = Some(point);
            self.apply_selection();
        }
    }
    pub fn clear_selection(&mut self) {
        self.terminal.selection = None;
        self.selection_anchor = None;
        self.selection_head = None;
        self.selection_explicit = false;
        self.selection_invalidated = false;
        self.selection_block = false;
    }
    pub fn local_selection_active(&self) -> bool {
        self.has_selection() || self.selection_head.is_some()
    }
    pub fn explicit_selection_active(&self) -> bool {
        self.selection_explicit && self.local_selection_active()
    }
    pub fn enter_local_selection(&mut self) {
        if self.local_selection_active() {
            return;
        }
        let point = if self.terminal.mode().contains(TermMode::SHOW_CURSOR) {
            let cursor = self.terminal.grid().cursor.point;
            alacritty_terminal::term::point_to_viewport(self.display_offset(), cursor)
                .filter(|point| {
                    point.line < usize::from(self.size.rows)
                        && point.column.0 < usize::from(self.size.columns)
                })
                .map(|_| cursor)
                .unwrap_or_else(|| self.grid_point(TerminalPoint { row: 0, column: 0 }))
        } else {
            self.grid_point(TerminalPoint { row: 0, column: 0 })
        };
        self.selection_head = Some(point);
        self.selection_explicit = true;
        self.selection_invalidated = false;
        self.selection_block = false;
    }
    pub fn begin_selection_kind(&mut self, point: TerminalPoint, block: bool) {
        let point = self.grid_point(point);
        self.selection_anchor = Some(point);
        self.selection_head = Some(point);
        self.selection_block = block;
        // An explicit interaction may own this drag; clearing the range does
        // not implicitly give ownership back to the child.
        self.selection_invalidated = false;
        self.terminal.selection = Some(Selection::new(
            if block {
                SelectionType::Block
            } else {
                SelectionType::Simple
            },
            point,
            Side::Left,
        ));
    }
    pub fn set_selection_block(&mut self, block: bool) {
        self.selection_block = block;
        if self.has_selection() {
            self.apply_selection();
        }
    }
    pub fn place_selection_caret(&mut self, point: TerminalPoint) {
        self.terminal.selection = None;
        self.selection_anchor = None;
        self.selection_head = Some(self.grid_point(point));
        self.selection_explicit = true;
        self.selection_invalidated = false;
        self.selection_block = false;
    }
    pub fn selection_focus_lost(&mut self) {
        if !self.has_selection() {
            self.clear_selection();
        }
    }
    pub fn move_selection(&mut self, motion: SelectionMotion, extend: bool) {
        if !self.local_selection_active() {
            return;
        }
        let Some(current) = self.selection_head else {
            return;
        };
        let had_range = self.has_selection();
        if !extend && had_range && matches!(motion, SelectionMotion::Left | SelectionMotion::Right)
        {
            let range = self
                .terminal
                .selection
                .as_ref()
                .and_then(|selection| selection.to_range(&self.terminal));
            let Some(range) = range else { return };
            let point = if motion == SelectionMotion::Left {
                range.start
            } else {
                range.end
            };
            self.terminal.selection = None;
            self.selection_anchor = None;
            self.selection_head = Some(point);
            self.selection_explicit = true;
            self.selection_block = false;
            self.selection_invalidated = false;
            self.scroll_to_caret();
            return;
        }
        if !extend && had_range {
            self.terminal.selection = None;
            self.selection_anchor = None;
        }
        let point = self.moved_point(current, motion);
        self.selection_head = Some(point);
        self.selection_explicit = true;
        if extend {
            self.selection_anchor.get_or_insert(current);
            self.apply_selection();
        } else {
            self.terminal.selection = None;
            self.selection_anchor = None;
            self.selection_block = false;
        }
        self.selection_invalidated = false;
        self.scroll_to_caret();
    }
    pub fn selection_invalidated(&self) -> bool {
        self.selection_invalidated
    }
    pub fn selection_text(&self) -> Option<String> {
        self.terminal
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }
    pub fn has_selection(&self) -> bool {
        self.terminal.selection.as_ref().is_some_and(|selection| {
            !selection.is_empty() && selection.to_range(&self.terminal).is_some()
        })
    }
    fn apply_selection(&mut self) {
        let (Some(anchor), Some(head)) = (self.selection_anchor, self.selection_head) else {
            return;
        };
        let ty = if self.selection_block {
            SelectionType::Block
        } else {
            SelectionType::Simple
        };
        let mut selection = Selection::new(ty, anchor, Side::Left);
        selection.update(head, Side::Right);
        selection.include_all();
        self.terminal.selection = Some(selection);
    }
    fn moved_point(&mut self, current: GridPoint, motion: SelectionMotion) -> GridPoint {
        match motion {
            SelectionMotion::Home => GridPoint::new(current.line, Column(0)),
            SelectionMotion::End => {
                let row = &self.terminal.grid()[current.line];
                let column = row.line_length().saturating_sub(1);
                self.terminal.expand_wide(
                    GridPoint::new(current.line, Column(column)),
                    Direction::Left,
                )
            }
            SelectionMotion::PageUp | SelectionMotion::PageDown => {
                let lines = usize::from(self.size.rows) as i32;
                let lines = if motion == SelectionMotion::PageUp {
                    lines
                } else {
                    -lines
                };
                let cursor = ViModeCursor::new(current).scroll(&self.terminal, lines);
                self.terminal.scroll_display(Scroll::Delta(lines));
                self.terminal.scroll_to_point(cursor.point);
                cursor.point
            }
            SelectionMotion::Left => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Left)
                    .point
            }
            SelectionMotion::Right => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Right)
                    .point
            }
            SelectionMotion::Up => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Up)
                    .point
            }
            SelectionMotion::Down => {
                ViModeCursor::new(current)
                    .motion(&mut self.terminal, ViMotion::Down)
                    .point
            }
        }
    }
    fn scroll_to_caret(&mut self) {
        if let Some(point) = self.selection_head {
            self.terminal.scroll_to_point(point);
            if !self.has_selection() {
                self.clamp_caret_to_viewport();
            }
        }
    }
    fn clamp_caret_to_viewport(&mut self) {
        let Some(mut point) = self.selection_head else {
            return;
        };
        let top = -(self.display_offset() as i32);
        let bottom = top + i32::from(self.size.rows) - 1;
        point.line.0 = point.line.0.clamp(top, bottom);
        point.column.0 = point
            .column
            .0
            .min(usize::from(self.size.columns).saturating_sub(1));
        self.selection_head = Some(point);
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
            local_caret: self.selection_head.and_then(|point| {
                alacritty_terminal::term::point_to_viewport(display_offset, point)
                    .filter(|point| {
                        point.line < usize::from(self.size.rows)
                            && point.column.0 < usize::from(self.size.columns)
                    })
                    .map(|point| (point.line, point.column.0))
            }),
            selection_invalidated: self.selection_invalidated,
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
        let had_selection = self.has_selection();
        let old_snapshot = self.selection_snapshot();
        self.parser.advance(&mut self.terminal, bytes);
        self.reconcile_selection(had_selection, old_snapshot);
    }
    fn resize(&mut self, size: Size) {
        let had_selection = self.has_selection();
        let snapshot = self.selection_snapshot();
        self.terminal.resize(Geometry(size));
        self.size = size;
        self.reconcile_selection(had_selection, snapshot);
    }
}

impl Engine {
    fn selection_snapshot(&self) -> Option<String> {
        self.terminal.selection_to_string()
    }
    fn invalidate_selection(&mut self) {
        self.terminal.selection = None;
        self.selection_anchor = None;
        self.selection_head = Some(self.grid_point(TerminalPoint { row: 0, column: 0 }));
        self.selection_explicit = true;
        self.selection_invalidated = true;
        self.selection_block = false;
        self.clamp_caret_to_viewport();
    }
    fn reconcile_selection(&mut self, had_selection: bool, old_snapshot: Option<String>) {
        if had_selection && self.has_selection() {
            if old_snapshot.is_some() && old_snapshot != self.selection_snapshot() {
                self.invalidate_selection();
                return;
            }
            if let Some(range) = self
                .terminal
                .selection
                .as_ref()
                .and_then(|selection| selection.to_range(&self.terminal))
            {
                let Some((anchor, head)) = self.selection_anchor.zip(self.selection_head) else {
                    return;
                };
                if self.selection_block {
                    let row_forward = anchor.line <= head.line;
                    let column_forward = anchor.column <= head.column;
                    self.selection_anchor = Some(GridPoint::new(
                        if row_forward {
                            range.start.line
                        } else {
                            range.end.line
                        },
                        if column_forward {
                            range.start.column
                        } else {
                            range.end.column
                        },
                    ));
                    self.selection_head = Some(GridPoint::new(
                        if row_forward {
                            range.end.line
                        } else {
                            range.start.line
                        },
                        if column_forward {
                            range.end.column
                        } else {
                            range.start.column
                        },
                    ));
                } else if anchor <= head {
                    self.selection_anchor = Some(range.start);
                    self.selection_head = Some(range.end);
                } else {
                    self.selection_anchor = Some(range.end);
                    self.selection_head = Some(range.start);
                }
            }
        } else if had_selection {
            self.invalidate_selection();
        } else if self.selection_head.is_some() {
            self.clamp_caret_to_viewport();
            if self.selection_anchor.is_some() {
                // An unextended pointer anchor is still empty, not a range to
                // resurrect at pre-resize or evicted coordinates.
                self.selection_anchor = self.selection_head;
                self.terminal.selection = None;
            }
        }
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

    /// Manual acceptance: select, scroll the viewport, then copy. Scrolling
    /// must neither move nor lose the content-anchored selection, and further
    /// drags must extend from the current viewport offset.
    #[test]
    fn scrolling_preserves_and_extends_a_scrollback_selection() {
        let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
        engine.output(b"L1\r\nL2\r\nL3\r\nL4\r\nL5");
        // Viewport shows L2..L5; select the bottom line.
        engine.begin_selection(TerminalPoint { row: 3, column: 0 });
        engine.update_selection(TerminalPoint { row: 3, column: 1 });
        assert_eq!(engine.selection_text().as_deref(), Some("L5"));

        // Scroll two lines up: the selection stays glued to L5 content even
        // though L5 has left the viewport.
        engine.scroll_lines(2);
        assert!(engine.display_offset() > 0);
        assert_eq!(engine.selection_text().as_deref(), Some("L5"));

        // Extend from the new viewport top (now showing L1) while the button is
        // still held: the anchor stays at the original press, so the selection
        // grows back across the scrolled content.
        engine.update_selection(TerminalPoint { row: 0, column: 1 });
        assert_eq!(engine.selection_text().as_deref(), Some("1\nL2\nL3\nL4\nL"));
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
    fn block_selection_and_local_caret_are_distinct() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"abcd\r\nefgh");
        engine.begin_selection_kind(TerminalPoint { row: 0, column: 1 }, true);
        engine.update_selection(TerminalPoint { row: 1, column: 2 });
        assert_eq!(engine.selection_text().as_deref(), Some("bc\nfg"));
        assert!(!engine.explicit_selection_active());

        engine.clear_selection();
        engine.enter_local_selection();
        assert!(engine.local_selection_active());
        assert!(engine.screen().local_caret.is_some());
        assert!(engine.selection_text().is_none());
    }

    #[test]
    fn pointer_press_is_empty_until_extended_and_remains_valid_after_resize() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"abcdefgh");
        engine.begin_selection(TerminalPoint { row: 2, column: 7 });
        assert!(!engine.has_selection());
        assert!(engine.selection_text().is_none());
        engine.resize(Size::new(3, 1).unwrap());
        engine.update_selection(TerminalPoint { row: 0, column: 0 });
        assert!(engine.has_selection());
        assert_eq!(engine.screen().local_caret, Some((0, 0)));
    }

    #[test]
    fn caret_and_selection_head_stay_visible_during_page_navigation() {
        let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
        engine.output(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine");
        engine.place_selection_caret(TerminalPoint { row: 2, column: 1 });
        engine.move_selection(SelectionMotion::PageUp, true);
        assert_eq!(engine.display_offset(), 4);
        assert_eq!(engine.screen().local_caret, Some((2, 0)));
        assert!(engine.has_selection());
        engine.move_selection(SelectionMotion::Left, false);
        assert!(engine.screen().local_caret.is_some());
        engine.move_selection(SelectionMotion::PageDown, false);
        assert_eq!(engine.display_offset(), 0);
        assert!(engine.screen().local_caret.is_some());
        assert!(!engine.has_selection());
    }

    #[test]
    fn focus_loss_clears_caret_but_preserves_a_range() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"text");
        engine.enter_local_selection();
        engine.selection_focus_lost();
        assert!(!engine.local_selection_active());
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        engine.selection_focus_lost();
        assert_eq!(engine.selection_text().as_deref(), Some("tex"));
        engine.move_selection(SelectionMotion::Right, false);
        engine.selection_focus_lost();
        assert!(!engine.local_selection_active());
    }

    #[test]
    fn selection_uses_alacritty_wide_and_wrapped_cell_semantics() {
        let mut engine = Engine::new(Size::new(4, 3).unwrap(), false);
        engine.output("ab界d".as_bytes());
        engine.begin_selection(TerminalPoint { row: 0, column: 1 });
        engine.update_selection(TerminalPoint { row: 1, column: 1 });
        assert_eq!(engine.selection_text().as_deref(), Some("b界d"));
    }

    #[test]
    fn navigation_collapses_then_extends_and_pages() {
        let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
        engine.output(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
        engine.begin_selection_kind(TerminalPoint { row: 1, column: 0 }, false);
        engine.update_selection(TerminalPoint { row: 1, column: 2 });
        engine.move_selection(SelectionMotion::Left, false);
        assert!(engine.explicit_selection_active());
        assert!(!engine.has_selection());
        engine.move_selection(SelectionMotion::Right, true);
        assert!(engine.has_selection());
        engine.move_selection(SelectionMotion::PageDown, false);
        assert!(!engine.has_selection());
        assert!(engine.screen().local_caret.is_some());
    }

    #[test]
    fn collapse_does_not_take_an_extra_step_and_vertical_collapse_moves_head() {
        let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
        engine.output(b"zero\r\none\r\ntwo");
        engine.begin_selection(TerminalPoint { row: 1, column: 1 });
        engine.update_selection(TerminalPoint { row: 1, column: 3 });
        engine.move_selection(SelectionMotion::Left, false);
        assert_eq!(engine.screen().local_caret, Some((1, 1)));
        engine.move_selection(SelectionMotion::Left, false);
        assert_eq!(engine.screen().local_caret, Some((1, 0)));

        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 1, column: 2 });
        engine.move_selection(SelectionMotion::Down, false);
        assert_eq!(engine.screen().local_caret, Some((2, 2)));
    }

    #[test]
    fn home_and_end_stop_at_visual_row_boundaries() {
        let mut engine = Engine::new(Size::new(4, 3).unwrap(), false);
        engine.output(b"abcdef");
        engine.place_selection_caret(TerminalPoint { row: 0, column: 1 });
        engine.move_selection(SelectionMotion::End, false);
        assert_eq!(engine.screen().local_caret, Some((0, 3)));
        engine.move_selection(SelectionMotion::Home, false);
        assert_eq!(engine.screen().local_caret, Some((0, 0)));
    }

    #[test]
    fn caret_extension_creates_a_simple_range_and_preserves_explicit_ownership() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"abcdef");
        engine.enter_local_selection();
        engine.place_selection_caret(TerminalPoint { row: 0, column: 1 });
        engine.extend_selection(TerminalPoint { row: 0, column: 3 });
        assert_eq!(engine.selection_text().as_deref(), Some("bcd"));
        assert!(engine.explicit_selection_active());
        engine.clear_selection();
        engine.enter_local_selection();
        engine.begin_selection_kind(TerminalPoint { row: 0, column: 1 }, false);
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        assert!(engine.explicit_selection_active());
    }

    #[test]
    fn reverse_block_axes_remain_independent_when_scrolled() {
        let mut engine = Engine::new(Size::new(8, 4).unwrap(), false);
        engine.output(b"abcdefgh\r\nijklmnop\r\nqrstuvwx\r\nyzABCDEF\r\nGHIJKLMN");
        engine.begin_selection_kind(TerminalPoint { row: 2, column: 5 }, true);
        engine.update_selection(TerminalPoint { row: 1, column: 7 });
        engine.scroll_lines(1);
        engine.update_selection(TerminalPoint { row: 0, column: 6 });
        assert_eq!(engine.selection_text().as_deref(), Some("fg\nno\nvw\nDE"));
    }

    #[test]
    fn invalidated_selection_becomes_a_caret() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"old text");
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        engine.output(b"\x1b[2J");
        assert!(!engine.has_selection());
        assert!(engine.local_selection_active());
        assert!(engine.selection_invalidated());
        assert!(engine.screen().local_caret.is_some());
    }

    #[test]
    fn overwriting_selected_cells_invalidates_even_if_alacritty_retains_range() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"old text");
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        engine.output(b"\x1b[1;1HX");
        assert!(!engine.has_selection());
        assert!(engine.selection_invalidated());
        assert!(engine.screen().local_caret.is_some());
    }

    #[test]
    fn buffer_switch_and_eviction_never_resurrect_a_stale_selection() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"selected");
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        engine.output(b"\x1b[?1049h\x1b[2Jalt\x1b[?1049l");
        assert!(!engine.has_selection());
        assert!(engine.selection_invalidated());
        assert!(engine.screen().local_caret.is_some());

        engine.clear_selection();
        engine.output(b"primary\r\n");
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 2 });
        for _ in 0..TERMINAL_SCROLLBACK_LINES + 2 {
            engine.output(b"x\r\n");
        }
        assert!(!engine.has_selection());
        assert!(engine.selection_invalidated());
        assert!(engine.screen().local_caret.is_some());
    }

    #[test]
    fn caret_is_clamped_after_resize_and_history_eviction() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"caret");
        engine.place_selection_caret(TerminalPoint { row: 2, column: 7 });
        engine.resize(Size::new(3, 1).unwrap());
        assert_eq!(engine.screen().local_caret, Some((0, 2)));
        for _ in 0..TERMINAL_SCROLLBACK_LINES + 2 {
            engine.output(b"x\r\n");
        }
        let caret = engine.screen().local_caret.expect("caret remains visible");
        assert!(caret.0 < 1 && caret.1 < 3);
    }

    #[test]
    fn resizing_columns_invalidates_selection_but_resize_preserves_rows() {
        let mut engine = Engine::new(Size::new(8, 3).unwrap(), false);
        engine.output(b"wide content");
        engine.begin_selection(TerminalPoint { row: 0, column: 0 });
        engine.update_selection(TerminalPoint { row: 0, column: 3 });
        engine.resize(Size::new(10, 3).unwrap());
        assert!(!engine.has_selection());
        assert!(engine.selection_invalidated());
        assert!(engine.screen().local_caret.is_some());
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
