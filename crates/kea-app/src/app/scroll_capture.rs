//! Explicit scroll capture: snapshot a selected screen rectangle while the user
//! scrolls the application, then show the stitched text for review.
//! Capture changes no input routing: wheel and keys reach the app as usual.
use super::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Sizable as _,
};
use kea_app::terminal::stitch::{self, Region};

pub(super) struct ScrollCapture {
    region: Region,
    snapshots: Vec<Vec<String>>,
    full: bool,
}

impl ScrollCapture {
    pub(super) fn snapshots(&self) -> usize {
        self.snapshots.len()
    }

    pub(super) fn full(&self) -> bool {
        self.full
    }
}

impl KeaView {
    /// Start capturing the selected rectangle of the live screen.
    pub(super) fn start_scroll_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A Kea-owned drag freezes the view; the selection on that frozen view
        // defines the region, and capture then follows the live screen.
        if !self.visible || !self.session.is_running() || self.session.is_history() {
            self.notice = Some("Scroll capture needs a live terminal.".into());
            cx.notify();
            return;
        }
        let screen = self.session.screen();
        let Some(region) = stitch::selected_region(&screen) else {
            self.notice = Some(
                "Select the text area to capture first (a block selection excludes side panels)."
                    .into(),
            );
            cx.notify();
            return;
        };
        // The selection would only be invalidated by scrolling; the region is kept.
        self.session.clear_terminal_selection();
        if self.session.is_frozen() {
            // The first clear above acted on the frozen view; this one on live.
            self.session.go_live();
            self.prompt_line.invalidate();
            self.pending_run = None;
            self.session.clear_terminal_selection();
        }
        let Some(first) = stitch::region_rows(&self.session.screen(), &region) else {
            self.notice = Some("Scroll capture did not start: the terminal was resized.".into());
            cx.notify();
            return;
        };
        self.scroll_capture = Some(ScrollCapture {
            region,
            snapshots: vec![first],
            full: false,
        });
        self.notice = None;
        window.focus(&self.focus);
        cx.notify();
    }

    /// Record the region whenever the visible screen changed. Runs on render.
    pub(super) fn observe_scroll_capture(&mut self) {
        let live = self.visible && self.session.input_allowed();
        let Some(capture) = &mut self.scroll_capture else {
            return;
        };
        if !live {
            self.scroll_capture = None;
            self.notice = Some("Scroll capture cancelled: the terminal is no longer live.".into());
            return;
        }
        let screen = self.session.screen();
        let Some(rows) = stitch::region_rows(&screen, &capture.region) else {
            self.scroll_capture = None;
            self.notice = Some("Scroll capture cancelled: the terminal was resized.".into());
            return;
        };
        if capture.snapshots.last() == Some(&rows) || capture.full {
            return;
        }
        if capture.snapshots.len() >= stitch::MAX_SNAPSHOTS {
            capture.full = true;
            return;
        }
        capture.snapshots.push(rows);
    }

    pub(super) fn cancel_scroll_capture(&mut self, cx: &mut Context<Self>) {
        if self.scroll_capture.take().is_some() {
            cx.notify();
        }
    }

    /// Stitch the snapshots and show them for review; nothing is copied yet.
    pub(super) fn finish_scroll_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = self.scroll_capture.take() else {
            return;
        };
        let stitched = stitch::stitch(&capture.snapshots);
        cx.notify();
        let title = format!("Scroll capture · {} snapshot(s)", capture.snapshots.len());
        text_viewer::open(
            title,
            text_viewer::TextSource::Reconstructed {
                gaps: stitched.gaps,
            },
            stitched.text,
            window,
            cx,
        );
    }
}

impl KeaView {
    pub(super) fn scroll_capture_controls(
        &self,
        selection_available: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(capture) = &self.scroll_capture else {
            if !selection_available || !self.session.is_running() || self.session.is_history() {
                return div().into_any_element();
            }
            let button = Button::new("start-scroll-capture")
                .label("Capture")
                .tooltip("Record the selected area while you scroll the app, then review the rebuilt text before copying")
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.start_scroll_capture(window, cx)));
            // Shrinks (and clips) before it could push other header controls.
            return div()
                .min_w_0()
                .overflow_hidden()
                .child(button)
                .into_any_element();
        };
        let status = if capture.full() {
            format!("Full · {}", capture.snapshots())
        } else {
            format!("● Capturing · {}", capture.snapshots())
        };
        div()
            .id("scroll-capture")
            .flex()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .px_1()
            .rounded_sm()
            .bg(cx.theme().warning)
            .text_color(cx.theme().warning_foreground)
            .child(div().whitespace_nowrap().child(status))
            .child(
                Button::new("finish-scroll-capture")
                    .label("Done")
                    .small()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.finish_scroll_capture(window, cx)),
                    ),
            )
            .child(
                Button::new("cancel-scroll-capture")
                    .label("Cancel")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_scroll_capture(cx))),
            )
            .into_any_element()
    }
}
