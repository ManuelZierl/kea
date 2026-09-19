use super::rendering::{
    terminal_font_metrics, terminal_point, terminal_scroll_lines, terminal_scroll_units,
};
use super::*;
use anyhow::Result;
use gpui::prelude::*;
use kea_alacritty::{MouseTracking, TerminalPoint};
use kea_app::terminal::{
    mouse as terminal_mouse,
    selection::{self, KeyModifiers, MouseOwner},
};

impl KeaView {
    pub(super) fn terminal_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus);
        let metrics = terminal_font_metrics(window, cx);
        let Some(point) = terminal_point(
            event.position,
            self.terminal_bounds,
            &metrics,
            self.session.terminal_size(),
        ) else {
            return;
        };
        let reporting = self.session.terminal_mouse_reporting();
        let owner = selection::mouse_owner(
            reporting,
            self.settings.shift_mouse_selects_locally,
            self.session.terminal_explicit_selection_active(),
            event.modifiers.shift,
            event.modifiers.alt,
        );
        let gesture = selection::Gesture::new(
            point,
            owner,
            self.session.terminal_explicit_selection_active(),
            event.modifiers.shift,
            self.session.terminal_has_selection(),
        );
        self.terminal_gesture = Some(gesture);
        self.terminal_gesture_bounds = self.terminal_bounds;
        match owner {
            MouseOwner::LocalSimple | MouseOwner::LocalBlock => {
                if gesture.shift_extend {
                    self.session.extend_terminal_selection(point);
                } else if gesture.explicit {
                    self.session.place_terminal_selection_caret(point);
                } else {
                    self.session.begin_terminal_selection(point);
                }
                self.notice = None;
                cx.notify();
            }
            MouseOwner::Forward => {
                let result = self.forward_pointer(
                    terminal_mouse::PointerEvent::Press,
                    point,
                    KeyModifiers {
                        shift: event.modifiers.shift,
                        alt: event.modifiers.alt,
                        control: event.modifiers.control,
                        ..Default::default()
                    },
                    true,
                    cx,
                );
                if result.is_err() {
                    self.terminal_gesture = None;
                    self.terminal_gesture_bounds = None;
                }
                self.result(result, cx);
                cx.stop_propagation();
            }
        }
    }

    pub(super) fn terminal_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut gesture) = self.terminal_gesture {
            if !event.dragging() {
                return;
            }
            let metrics = terminal_font_metrics(window, cx);
            let bounds = self.terminal_gesture_bounds.or(self.terminal_bounds);
            if let Some(point) = terminal_point(
                event.position,
                bounds,
                &metrics,
                self.session.terminal_size(),
            ) {
                if gesture.owner != MouseOwner::Forward {
                    self.update_local_gesture(&mut gesture, point);
                    cx.notify();
                } else {
                    let tracking = self.session.terminal_mouse_tracking();
                    let should_report =
                        matches!(tracking, Some(MouseTracking::Drag | MouseTracking::Motion))
                            && (event.pressed_button == Some(MouseButton::Left)
                                || matches!(tracking, Some(MouseTracking::Motion)));
                    if should_report {
                        let result = self.forward_pointer(
                            terminal_mouse::PointerEvent::Motion,
                            point,
                            KeyModifiers {
                                shift: event.modifiers.shift,
                                alt: event.modifiers.alt,
                                control: event.modifiers.control,
                                ..Default::default()
                            },
                            false,
                            cx,
                        );
                        self.result(result, cx);
                        cx.stop_propagation();
                    }
                }
                self.terminal_gesture = Some(gesture);
            }
            return;
        }

        if event.pressed_button.is_some()
            || self.session.terminal_mouse_tracking() != Some(MouseTracking::Motion)
        {
            return;
        }
        if selection::hover_is_local(
            self.session.terminal_mouse_reporting(),
            self.settings.shift_mouse_selects_locally,
            self.session.terminal_explicit_selection_active(),
            event.modifiers.shift,
        ) {
            cx.stop_propagation();
            return;
        }
        let metrics = terminal_font_metrics(window, cx);
        let Some(point) = terminal_point(
            event.position,
            self.terminal_bounds,
            &metrics,
            self.session.terminal_size(),
        ) else {
            return;
        };
        let Some(encoding) = self.session.terminal_mouse_encoding() else {
            return;
        };
        let result = terminal_mouse::encode_pointer(
            encoding,
            terminal_mouse::PointerEvent::Motion,
            None,
            point,
            event.modifiers.shift,
            event.modifiers.alt,
            event.modifiers.control,
        )
        .map_err(anyhow::Error::msg)
        .and_then(|bytes| self.session.send(bytes));
        if result.is_ok() {
            self.input_context.invalidate();
            self.pending_run = None;
            self.dismiss_completion();
        } else {
            self.result(result, cx);
        }
        cx.stop_propagation();
    }

    pub(super) fn terminal_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mut gesture) = self.terminal_gesture.take() {
            let bounds = self.terminal_gesture_bounds.take().or(self.terminal_bounds);
            let metrics = terminal_font_metrics(window, cx);
            let point = terminal_point(
                event.position,
                bounds,
                &metrics,
                self.session.terminal_size(),
            );
            if gesture.owner != MouseOwner::Forward {
                if let Some(point) = point {
                    self.update_local_gesture(&mut gesture, point);
                    if !gesture.moved && !gesture.preserve_click && !gesture.explicit {
                        self.session.clear_terminal_selection();
                    }
                }
                cx.notify();
            } else if let Some(point) = point {
                let result = self.forward_pointer(
                    terminal_mouse::PointerEvent::Release,
                    point,
                    KeyModifiers {
                        shift: event.modifiers.shift,
                        alt: event.modifiers.alt,
                        control: event.modifiers.control,
                        ..Default::default()
                    },
                    false,
                    cx,
                );
                self.result(result, cx);
                cx.stop_propagation();
            }
        }
    }

    fn update_local_gesture(&mut self, gesture: &mut selection::Gesture, point: TerminalPoint) {
        let first_move = !gesture.moved;
        if !gesture.moved_to(point) {
            return;
        }
        if first_move && gesture.potential_block {
            self.session.set_terminal_selection_block(true);
        }
        self.session.update_terminal_selection(point);
    }

    fn forward_pointer(
        &mut self,
        event: terminal_mouse::PointerEvent,
        point: TerminalPoint,
        modifiers: KeyModifiers,
        input_effects: bool,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if event == terminal_mouse::PointerEvent::Press {
            self.session.clear_terminal_selection();
        }
        let Some(encoding) = self.session.terminal_mouse_encoding() else {
            return Ok(());
        };
        let button = Some(terminal_mouse::PointerButton::Left);
        let bytes = terminal_mouse::encode_pointer(
            encoding,
            event,
            button,
            point,
            modifiers.shift,
            modifiers.alt,
            modifiers.control,
        )
        .map_err(anyhow::Error::msg)?;
        let result = self.session.send(bytes);
        if result.is_ok() {
            if input_effects {
                self.note_forwarded_terminal_input();
            } else {
                self.input_context.invalidate();
                self.pending_run = None;
                self.dismiss_completion();
            }
        }
        if result.is_err() {
            cx.notify();
        }
        result
    }

    pub(super) fn terminal_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let metrics = terminal_font_metrics(window, cx);
        if let Some(encoding) = self.session.terminal_mouse_encoding() {
            if !event.modifiers.shift {
                self.terminal_scroll_remainder = 0.;
                let reports = terminal_scroll_units(
                    event.delta,
                    metrics.line_height,
                    &mut self.terminal_mouse_scroll_remainder,
                    MAX_MOUSE_WHEEL_REPORTS_PER_EVENT,
                );
                if reports != 0 {
                    let Some(point) = terminal_point(
                        event.position,
                        self.terminal_bounds,
                        &metrics,
                        self.session.terminal_size(),
                    ) else {
                        cx.stop_propagation();
                        return;
                    };
                    let direction = if reports > 0 {
                        terminal_mouse::WheelDirection::Up
                    } else {
                        terminal_mouse::WheelDirection::Down
                    };
                    let result = terminal_mouse::encode_wheel(
                        encoding,
                        direction,
                        point,
                        event.modifiers.alt,
                        event.modifiers.control,
                    )
                    .map_err(anyhow::Error::msg)
                    .and_then(|report| {
                        let mut bytes =
                            Vec::with_capacity(report.len() * reports.unsigned_abs() as usize);
                        for _ in 0..reports.unsigned_abs() {
                            bytes.extend_from_slice(&report);
                        }
                        self.session.send(bytes)
                    });
                    if result.is_ok() {
                        self.note_forwarded_terminal_input();
                    }
                    self.result(result, cx);
                }
                cx.stop_propagation();
                return;
            }
        }
        self.terminal_mouse_scroll_remainder = 0.;
        let lines = terminal_scroll_lines(
            event.delta,
            metrics.line_height,
            &mut self.terminal_scroll_remainder,
        );
        if lines != 0 {
            self.session.scroll_lines(lines);
            self.notice = None;
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(super) fn paste_terminal(&mut self, cx: &mut Context<Self>) {
        self.session.clear_terminal_selection();
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            cx.notify();
            return;
        };
        let result = input::paste(&text, self.session.bracketed_paste())
            .and_then(|bytes| self.session.send(bytes));
        if result.is_ok() {
            self.note_forwarded_terminal_input();
        }
        self.result(result, cx);
    }
}
