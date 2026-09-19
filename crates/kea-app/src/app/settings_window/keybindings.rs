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
        let mut subscriptions = Vec::new();
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
            rows, path, _subscriptions: subscriptions,
            status: "Keybindings shown are for the next launch. Save changes explicitly; restart Kea to activate them.".into(),
            error: false,
        }
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
        div().w_full().min_w_0().flex().flex_col().gap_3()
            .child(div().text_sm().child("Switch terminal ⇄ composer is the one Kea shortcut reserved from the child. Focus terminal is an optional composer-only alternative. Remap or set the switch to none if your terminal app needs that key."))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("Use ctrl-l, ctrl-shift-enter, alt-up, f10, etc. Separate alternatives with commas; none disables a binding. Type the shortcut name as text; pressing the chord itself runs the live shortcut instead. Press Enter in any field or choose Save keybindings below."))
            .child(div().id("keybinding-status").w_full().min_w_0().text_sm()
                .text_color(if self.error { cx.theme().danger } else { cx.theme().muted_foreground })
                .child(self.status.clone()))
            .child(Button::new("save-keybindings").label("Save keybindings").primary()
                .on_click(cx.listener(|this, _, _, cx| this.save(cx))))
            .children(self.rows.iter().map(|row| {
                div().w_full().min_w_0().flex().flex_col().gap_1().pb_2()
                    .child(div().child(row.label))
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
