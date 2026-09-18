use alacritty_terminal::{
    event::{Event as TerminalEvent, EventListener},
    grid::Dimensions,
    term::Config,
    vte::ansi::Processor,
    Term,
};
use kea_core::{Projection, Recording, Size};
use std::sync::{Arc, Mutex};

use crate::TERMINAL_SCROLLBACK_LINES;

#[derive(Clone, Default)]
pub(crate) struct Listener {
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
    pub(crate) terminal: Term<Listener>,
    pub(crate) parser: Processor,
    replies: Option<Arc<Mutex<Vec<String>>>>,
    pub(crate) size: Size,
    pub(crate) selection_anchor: Option<alacritty_terminal::index::Point>,
    pub(crate) selection_head: Option<alacritty_terminal::index::Point>,
    pub(crate) selection_block: bool,
    pub(crate) selection_explicit: bool,
    pub(crate) selection_invalidated: bool,
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

    pub fn drain_replies(&mut self) -> Vec<String> {
        self.replies
            .as_ref()
            .map(|q| std::mem::take(&mut *q.lock().unwrap_or_else(|e| e.into_inner())))
            .unwrap_or_default()
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

#[cfg(test)]
#[path = "../tests/unit/engine.rs"]
mod tests;
