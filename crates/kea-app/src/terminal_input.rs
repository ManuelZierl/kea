//! Terminal text-service bridge. The reusable component owns composition/ranges;
//! only committed text is forwarded to the child. No input log is kept.
use super::*;
use std::ops::Range;

impl KeaView {
    fn terminal_replacement_fits(
        &mut self,
        range: Option<&Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.terminal_composition.update(cx, |state, cx| {
            let effective_range = range
                .cloned()
                .or_else(|| state.marked_text_range(window, cx))
                .or_else(|| {
                    state
                        .selected_text_range(true, window, cx)
                        .map(|selection| selection.range)
                })
                .unwrap_or_default();
            let mut adjusted = None;
            let replaced_bytes = state
                .text_for_range(effective_range, &mut adjusted, window, cx)
                .map_or(0, |replaced| replaced.len());
            input::terminal_replacement_fits(state.value().len(), replaced_bytes, text.len())
        })
    }

    fn reject_large_terminal_text(&mut self, cx: &mut Context<Self>) {
        self.notice =
            Some("Terminal text input exceeds 64 KiB; use explicit paste for large input.".into());
        cx.notify();
    }
}

impl EntityInputHandler for KeaView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.terminal_composition.update(cx, |state, cx| {
            state.text_for_range(range, adjusted, window, cx)
        })
    }
    fn selected_text_range(
        &mut self,
        ignore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.terminal_composition.update(cx, |state, cx| {
            state.selected_text_range(ignore, window, cx)
        })
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.terminal_composition
            .update(cx, |state, cx| state.marked_text_range(window, cx))
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_composition.update(cx, |state, cx| {
            state.unmark_text(window, cx);
            state.set_value("", window, cx);
        });
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) || !self.session.input_allowed() {
            return;
        }
        if !self.terminal_replacement_fits(range.as_ref(), text, window, cx) {
            self.reject_large_terminal_text(cx);
            return;
        }
        if !text.is_empty() {
            self.session.clear_terminal_selection();
        }
        let text = self.terminal_composition.update(cx, |state, cx| {
            state.replace_text_in_range(range, text, window, cx);
            state.value().to_string()
        });
        self.terminal_composition = cx.new(|cx| InputState::new(window, cx));
        if !text.is_empty() {
            // A committed text event is child input, never a local selection
            // command. End local interaction before attempting delivery.
            self.session.clear_terminal_selection();
            let result = self.session.send(text.as_bytes().to_vec());
            if result.is_ok() {
                self.session.scroll_bottom();
                self.session.clear_terminal_selection();
                self.pending_run = None;
                self.prompt_line.note_text(&text);
                self.document.note_terminal_input();
            }
            self.result(result, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) || !self.session.input_allowed() {
            return;
        }
        if !self.terminal_replacement_fits(range.as_ref(), text, window, cx) {
            self.reject_large_terminal_text(cx);
            return;
        }
        self.terminal_composition.update(cx, |state, cx| {
            state.replace_and_mark_text_in_range(range, text, selection, window, cx)
        });
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let screen = self.session.screen();
        let (row, col) = screen.local_caret.or(screen.cursor).unwrap_or((0, 0));
        let row = row.min(usize::from(screen.size.rows).saturating_sub(1));
        let col = col.min(usize::from(screen.size.columns).saturating_sub(1));
        let metrics = terminal_font_metrics(window, cx);
        Some(Bounds::new(
            bounds.origin
                + point(
                    metrics.cell_width * col as f32,
                    metrics.line_height * row as f32,
                ),
            size(metrics.cell_width, metrics.line_height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
