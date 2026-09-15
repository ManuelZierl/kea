//! Terminal text-service bridge. The reusable component owns composition/ranges;
//! only committed text is forwarded to the child. No input log is kept.
use super::*;
use std::ops::Range;
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
        if text.len() > 16 * 1024 {
            self.notice = Some(
                "Terminal text input exceeds 16 KiB; use explicit paste for large input.".into(),
            );
            cx.notify();
            return;
        }
        let text = self.terminal_composition.update(cx, |state, cx| {
            state.replace_text_in_range(range, text, window, cx);
            state.value().to_string()
        });
        self.terminal_composition = cx.new(|cx| InputState::new(window, cx));
        if !text.is_empty() {
            let result = self.session.send(text.into_bytes());
            if result.is_ok() {
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
        if text.len() + self.terminal_composition.read(cx).value().len() > 16 * 1024 {
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
        let (row, col) = self.session.screen().cursor.unwrap_or((0, 0));
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
        Some(0)
    }
}
