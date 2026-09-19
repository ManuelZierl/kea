use gpui::{prelude::*, *};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    ActiveTheme as _, Sizable as _,
};
use kea_app::config::keybindings::Keymap;
use std::path::PathBuf;

struct BindingRow {
    name: &'static str,
    label: &'static str,
    input: Entity<InputState>,
}

pub(super) struct KeybindingEditor {
    rows: Vec<BindingRow>,
    path: Option<PathBuf>,
    status: String,
    error: bool,
    _subscriptions: Vec<Subscription>,
    capture_focus: FocusHandle,
    recording: Option<usize>,
}

impl KeybindingEditor {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (keymap, warning) = Keymap::load();
        let mut this = Self::from_keymap(&keymap, Keymap::path(), window, cx);
        if let Some(warning) = warning {
            this.status = format!("{warning} Saving will replace that file with the values below.");
            this.error = true;
        }
        this
    }

    fn from_keymap(
        keymap: &Keymap,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let capture_focus = cx.focus_handle();
        let weak = cx.entity().downgrade();
        let mut subscriptions = vec![cx.intercept_keystrokes(move |event, window, cx| {
            let _ = weak.update(cx, |this, cx| this.record_key(&event.keystroke, window, cx));
        })];
        subscriptions.push(cx.on_blur(&capture_focus, window, |this, _, cx| {
            this.recording = None;
            cx.notify();
        }));
        let rows = keymap.settings_entries().into_iter().map(|(name, label, value)| {
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
            subscriptions.push(cx.subscribe(&input, |this: &mut Self, _, event: &InputEvent, cx| {
                match event {
                    InputEvent::Change => {
                        this.status = "Unsaved keybinding changes. Save, then restart Kea to activate them.".into();
                        this.error = false;
                        cx.notify();
                    }
                    // Single-line shortcut fields have no newline to insert:
                    // Enter saves the whole snapshot instead of doing nothing.
                    InputEvent::PressEnter { .. } => this.save(cx),
                    _ => {}
                }
            }));
            BindingRow { name, label, input }
        }).collect();
        Self {
            rows, path, _subscriptions: subscriptions, capture_focus, recording: None,
            status: "Keybindings shown are for the next launch. Save changes explicitly; restart Kea to activate them.".into(),
            error: false,
        }
    }

    fn begin_recording(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.recording = Some(index);
        window.focus(&self.capture_focus);
        self.status = "Press the shortcut. Escape cancels recording. No shortcut is executed while recording.".into();
        self.error = false;
        cx.notify();
    }

    fn record_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capture_focus.is_focused(window) {
            return;
        }
        let Some(index) = self.recording else {
            return;
        };
        cx.stop_propagation();
        if matches!(
            key.key.as_str(),
            "control" | "ctrl" | "shift" | "alt" | "super" | "cmd" | "fn"
        ) {
            return;
        }
        self.recording = None;
        let input = self.rows[index].input.clone();
        if key.key == "escape" {
            self.status = "Shortcut recording cancelled; the field was not changed.".into();
        } else if key.modifiers.function {
            self.status =
                "This platform's Fn modifier cannot be saved as a distinct shortcut.".into();
            self.error = true;
        } else {
            match Keymap::captured_shortcut(key) {
                Ok(shortcut) => {
                    input.update(cx, |input, cx| input.set_value(shortcut, window, cx));
                    self.status =
                        "Shortcut captured. Save keybindings to keep it; restart to activate."
                            .into();
                    self.error = false;
                }
                Err(error) => {
                    self.status = error.to_string();
                    self.error = true;
                }
            }
        }
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let result = (|| -> anyhow::Result<()> {
            let mut text = String::new();
            for row in &self.rows {
                let value = row.input.read(cx).value();
                anyhow::ensure!(
                    !value.contains(['\n', '\r', '#', '=']),
                    "{}: enter shortcuts separated by commas, or none",
                    row.name
                );
                text.push_str(&format!("{} = {}\n", row.name, value));
            }
            let keymap = Keymap::parse(&text)?;
            let path = self
                .path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("keybindings path unavailable"))?;
            keymap.save_to(path)
        })();
        match result {
            Ok(()) => {
                self.status = "Keybindings saved. Restart Kea to activate them; current shortcuts are unchanged until then.".into();
                self.error = false;
            }
            Err(error) => {
                self.status = format!("Not saved: {error:#}. Your edits are still here.");
                self.error = true;
            }
        }
        cx.notify();
    }
}

impl Render for KeybindingEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().track_focus(&self.capture_focus).w_full().min_w_0().flex().flex_col().gap_3()
            .child(div().text_sm().child("Switch terminal ⇄ composer is the one Kea shortcut reserved from the child. Focus terminal is an optional composer-only alternative. Remap or set the switch to none if your terminal app needs that key."))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("Type shortcut names (ctrl-l, alt-up, f10) separated by commas, or none. Alternatively choose Record and press the combination. Recording never executes shortcuts. Press Enter in a text field or choose Save keybindings."))
            .child(div().id("keybinding-status").w_full().min_w_0().text_sm()
                .text_color(if self.error { cx.theme().danger } else { cx.theme().muted_foreground })
                .child(self.status.clone()))
            .child(Button::new("save-keybindings").label("Save keybindings").primary()
                .on_click(cx.listener(|this, _, _, cx| this.save(cx))))
            .children(self.rows.iter().enumerate().map(|(index, row)| {
                div().w_full().min_w_0().flex().flex_col().gap_1().pb_2()
                    .child(div().flex().items_center().justify_between()
                        .child(row.label)
                        .child(Button::new(("record-shortcut", index)).small()
                            .label(if self.recording == Some(index) { "Press shortcut…" } else { "Record" })
                            .on_click(cx.listener(move |this, _, window, cx| this.begin_recording(index, window, cx))))
                    )
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(row.name))
                    .child(Input::new(&row.input).small().w_full())
            }))
            .child(div().w_full().min_w_0().text_xs().text_color(cx.theme().muted_foreground)
                .child("Saving replaces keybindings.conf with all values shown here. Unsubmitted edits are discarded when Settings closes."))
            .child(div().w_full().min_w_0().text_xs().text_color(cx.theme().muted_foreground)
                .child(self.path.as_ref().map(|p| format!("File: {}", p.display())).unwrap_or_else(|| "Keybindings path unavailable".into())))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app/keybinding_settings.rs"]
mod tests;
