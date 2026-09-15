//! OS committed text is distinct from terminal control-key encoding. In
//! particular, Windows emits WM_CHAR *after* an unconsumed key-down event.
//! This is a composition bridge, not an editable model of the terminal screen.
use super::{InputMode, KeaView, CELL_WIDTH, LINE_HEIGHT};
use gpui::*;
use std::ops::Range;

fn replace_utf16(text: &str, range: Range<usize>, replacement: &str) -> Option<String> {
    let units: Vec<_> = text.encode_utf16().collect();
    if range.start > range.end || range.end > units.len() { return None; }
    let before = String::from_utf16(&units[..range.start]).ok()?;
    let after = String::from_utf16(&units[range.end..]).ok()?;
    Some(format!("{before}{replacement}{after}"))
}

impl EntityInputHandler for KeaView {
    fn text_for_range(&mut self, range: Range<usize>, adjusted: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let text = self.terminal_preedit.as_deref().unwrap_or("");
        let units: Vec<_> = text.encode_utf16().collect();
        let slice = units.get(range.clone())?;
        let result = String::from_utf16(slice).ok()?;
        *adjusted = Some(range);
        Some(result)
    }
    fn selected_text_range(&mut self, _: bool, window: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        if self.input_mode != InputMode::Direct || !self.session.input_allowed() || !self.focus.is_focused(window) { return None; }
        let end = self.terminal_preedit.as_deref().unwrap_or("").encode_utf16().count();
        Some(UTF16Selection { range: end..end, reversed: false })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.terminal_preedit.as_ref().map(|text| 0..text.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.terminal_preedit = None;
        cx.notify();
    }
    fn replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.terminal_preedit.take().unwrap_or_default();
        if self.input_mode != InputMode::Direct || !self.session.input_allowed() || !self.focus.is_focused(window) { return; }
        // Only the uncommitted composition has replaceable text. Do not emulate
        // arbitrary edits to already-sent terminal input with guessed backspaces.
        let committed = if let Some(range) = range {
            let Some(value) = replace_utf16(&previous, range, text) else { return; };
            value
        } else { text.to_string() };
        if !committed.is_empty() {
            let result = self.session.send(committed.into_bytes());
            self.result(result, cx);
        }
        cx.notify();
    }
    fn replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        if self.input_mode != InputMode::Direct || !self.session.input_allowed() || !self.focus.is_focused(window) { return; }
        let composed = if let Some(range) = range {
            let Some(value) = replace_utf16(self.terminal_preedit.as_deref().unwrap_or(""), range, text) else { return; };
            value
        } else { text.to_string() };
        if composed.len() <= 16 * 1024 {
            self.terminal_preedit = (!composed.is_empty()).then_some(composed);
        }
        cx.notify();
    }
    fn bounds_for_range(&mut self, _: Range<usize>, bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let screen = self.session.screen();
        let (row, col) = screen.cursor.unwrap_or((0, 0));
        Some(Bounds::new(bounds.origin + point(px(col as f32 * CELL_WIDTH), px(row as f32 * LINE_HEIGHT)), size(px(CELL_WIDTH), px(LINE_HEIGHT))))
    }
    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> { Some(0) }
}
