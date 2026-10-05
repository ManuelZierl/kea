use alacritty_terminal::{
    event::{Event as TerminalEvent, EventListener},
    grid::Dimensions,
    term::{ClipboardType, Config},
    vte::ansi::{Handler, NamedPrivateMode, Processor},
    Term,
};
use kea_core::{Projection, Recording, Size};
use std::sync::{Arc, Mutex};

use crate::TERMINAL_SCROLLBACK_LINES;

const MAX_PENDING_TERMINAL_EVENTS: usize = 256;

#[derive(Clone, Default)]
pub(crate) struct Listener {
    replies: Option<Arc<Mutex<Vec<String>>>>,
    clipboard_stores: Option<Arc<Mutex<Vec<String>>>>,
}

fn push_bounded(queue: &Option<Arc<Mutex<Vec<String>>>>, value: String) {
    let Some(queue) = queue else {
        return;
    };
    let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
    if queue.len() < MAX_PENDING_TERMINAL_EVENTS {
        queue.push(value);
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: TerminalEvent) {
        // Live engines may return protocol replies and request writes to the
        // platform clipboard. Clipboard reads, title changes, bells, URL
        // opening and PTY resize requests remain ignored. Historical engines
        // have no event queues, so replay cannot perform either live effect.
        match event {
            TerminalEvent::PtyWrite(text) => push_bounded(&self.replies, text),
            TerminalEvent::ClipboardStore(ClipboardType::Clipboard, text) => {
                push_bounded(&self.clipboard_stores, text);
            }
            _ => {}
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
    clipboard_stores: Option<Arc<Mutex<Vec<String>>>>,
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
        let clipboard_stores = live.then(|| Arc::new(Mutex::new(Vec::new())));
        let listener = Listener {
            replies: replies.clone(),
            clipboard_stores: clipboard_stores.clone(),
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
            clipboard_stores,
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

    /// Copy only the already-rendered terminal grid for an immutable local view.
    /// This is deliberately not a parser/replay checkpoint and has no live reply
    /// channel; callers must never feed output or input to the returned engine.
    pub fn frozen_grid(&self) -> Self {
        let mut frozen = Self::new(self.size, false);
        *frozen.terminal.grid_mut() = self.terminal.grid().clone();
        frozen.terminal.selection = None;
        if !self
            .terminal
            .mode()
            .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR)
        {
            frozen
                .terminal
                .unset_private_mode(NamedPrivateMode::ShowCursor.into());
        }
        frozen
            .terminal
            .set_cursor_style(Some(self.terminal.cursor_style()));
        for index in 0..alacritty_terminal::term::color::COUNT {
            if let Some(color) = self.terminal.colors()[index] {
                frozen.terminal.set_color(index, color);
            }
        }
        frozen
    }

    pub fn drain_replies(&mut self) -> Vec<String> {
        self.replies
            .as_ref()
            .map(|q| std::mem::take(&mut *q.lock().unwrap_or_else(|e| e.into_inner())))
            .unwrap_or_default()
    }

    pub fn drain_clipboard_stores(&mut self) -> Vec<String> {
        self.clipboard_stores
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
