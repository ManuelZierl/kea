//! Opt-in confirmation on the terminal input path, after local Copy routing.
use super::*;
use gpui_component::ActiveTheme as _;
use kea_app::terminal::interrupt::{InterruptGuard, KeyLatch};

#[derive(Default)]
pub(super) struct InterruptState {
    pub gate: InterruptGuard,
    pub key_latch: KeyLatch,
    pub override_enabled: Option<bool>,
    pub swallowed: Vec<String>,
    origin: Option<FocusHandle>,
}

impl InterruptState {
    pub fn cancel(&mut self) {
        self.gate.cancel();
        self.origin = None;
    }
}

pub(super) fn is_ctrl_c(key: &Keystroke) -> bool {
    key.key.eq_ignore_ascii_case("c")
        && key.modifiers.control
        && !key.modifiers.alt
        && !key.modifiers.platform
        && !key.modifiers.function
}

impl KeaView {
    pub(super) fn request_interrupt(
        &mut self,
        bytes: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pump_session(cx);
        if !self.visible || !self.session.input_allowed() {
            self.interrupt.cancel();
            self.notice = Some("Return to a live terminal before sending Ctrl-C.".into());
            cx.notify();
            return;
        }
        let protected = self
            .interrupt
            .override_enabled
            .unwrap_or(self.settings.confirm_ctrl_c);
        if !protected {
            self.deliver_interrupt(bytes, cx);
            return;
        }
        self.pending_run = None;
        self.dismiss_completion();
        self.composer_workflow.menu = None;
        if !self.interrupt.gate.is_pending() {
            self.interrupt.origin = window.focused(cx);
            self.interrupt
                .gate
                .arm(self.input_context.generation(), bytes);
        }
        cx.notify();
    }

    fn deliver_interrupt(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let result = self.session.send(bytes);
        if result.is_ok() {
            self.note_forwarded_terminal_input();
        }
        self.result(result, cx);
    }

    pub(super) fn validate_interrupt(&mut self, window: &Window, _cx: &App) {
        let active = self.visible
            && self.session.input_allowed()
            && window.is_window_active()
            && self
                .interrupt
                .origin
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window));
        self.interrupt
            .gate
            .validate(self.input_context.generation(), active);
        if !self.interrupt.gate.is_pending() {
            self.interrupt.origin = None;
        }
    }

    /// Called by the window interceptor before either editor or terminal routing.
    pub(super) fn interrupt_keystroke(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = &event.keystroke;
        let physical = if key.key == "return" {
            "enter"
        } else {
            key.key.as_str()
        };
        if self.interrupt.swallowed.iter().any(|held| held == physical) {
            cx.stop_propagation();
            return true;
        }
        let enter = matches!(key.key.as_str(), "enter" | "return");
        let fresh = !enter || self.interrupt.key_latch.press(&key.key);
        if !self.interrupt.gate.is_pending() {
            return false;
        }
        // Process queued context/lifecycle events before authorizing the old target.
        self.pump_session(cx);
        self.validate_interrupt(window, cx);
        if !self.interrupt.gate.is_pending() {
            // A key confirming a stale interrupt must not become child input,
            // including subsequent repeats before its physical release.
            if enter || key.key == "escape" {
                self.interrupt.swallowed.push(physical.to_owned());
                cx.stop_propagation();
                cx.notify();
                return true;
            }
            return false;
        }
        let plain = !key.modifiers.control
            && !key.modifiers.alt
            && !key.modifiers.shift
            && !key.modifiers.platform
            && !key.modifiers.function;
        if enter && plain {
            if let Some(bytes) = self
                .interrupt
                .gate
                .confirm(self.input_context.generation(), fresh)
            {
                self.interrupt.origin = None;
                self.deliver_interrupt(bytes, cx);
            }
        } else if key.key == "escape" {
            self.interrupt.cancel();
        } else if is_ctrl_c(key) {
            // Repetition neither confirms nor accumulates another interrupt.
        } else if matches!(
            key.key.as_str(),
            "control" | "ctrl" | "shift" | "alt" | "cmd" | "super"
        ) {
            return false;
        } else {
            self.interrupt.cancel();
            cx.notify();
            return false;
        }
        self.interrupt.swallowed.push(physical.to_owned());
        cx.stop_propagation();
        cx.notify();
        true
    }

    pub(super) fn interrupt_controls(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div().id("interrupt-controls").flex_shrink_0().when(self.interrupt.gate.is_pending(), |row| {
            row.px_2().py_1().text_sm().text_color(cx.theme().danger)
                .child("Send Ctrl-C to this terminal? This may interrupt the current operation. Enter: send · Escape: cancel. Nothing has been sent.")
        })
    }

    pub(super) fn interrupt_toggle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let label = match self.interrupt.override_enabled {
            Some(true) => "Ctrl-C: confirm",
            Some(false) => "Ctrl-C: direct",
            None if self.settings.confirm_ctrl_c => "Ctrl-C: confirm (default)",
            None => "Ctrl-C: direct (default)",
        };
        button("interrupt-policy", label).on_click(cx.listener(|this, _, _, cx| {
            this.interrupt.cancel();
            this.interrupt.override_enabled = match this.interrupt.override_enabled {
                None => Some(!this.settings.confirm_ctrl_c),
                Some(value) if value != this.settings.confirm_ctrl_c => {
                    Some(this.settings.confirm_ctrl_c)
                }
                Some(_) => None,
            };
            cx.notify();
        }))
    }
}
