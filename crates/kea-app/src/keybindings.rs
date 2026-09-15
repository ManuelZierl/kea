//! Configurable semantic actions. Text input and navigation stay in the editor.
use anyhow::{Context as _, Result};
use gpui::{KeyBinding, Keystroke, NoAction};
use serde::Deserialize;
use std::{collections::HashMap, env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize)]
pub enum Action {
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    SelectAll,
    Find,
    CopyDocument,
    FocusEditor,
    Interrupt,
    Execute,
    ToggleDirect,
    ToggleBlocks,
    Complete,
    PreviousEvent,
    NextEvent,
    BackFiveSeconds,
    ForwardFiveSeconds,
    PlayPause,
    GoLive,
    Quit,
}
impl Action {
    const ALL: [Self; 21] = [
        Self::Copy,
        Self::Cut,
        Self::Paste,
        Self::Undo,
        Self::Redo,
        Self::SelectAll,
        Self::Find,
        Self::CopyDocument,
        Self::FocusEditor,
        Self::Interrupt,
        Self::Execute,
        Self::ToggleDirect,
        Self::ToggleBlocks,
        Self::Complete,
        Self::PreviousEvent,
        Self::NextEvent,
        Self::BackFiveSeconds,
        Self::ForwardFiveSeconds,
        Self::PlayPause,
        Self::GoLive,
        Self::Quit,
    ];
    fn config_name(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::SelectAll => "select_all",
            Self::Find => "find",
            Self::CopyDocument => "copy_document",
            Self::FocusEditor => "focus_editor",
            Self::Interrupt => "interrupt",
            Self::Execute => "execute",
            Self::ToggleDirect => "toggle_direct",
            Self::ToggleBlocks => "toggle_blocks",
            Self::Complete => "complete",
            Self::PreviousEvent => "previous_event",
            Self::NextEvent => "next_event",
            Self::BackFiveSeconds => "back_5s",
            Self::ForwardFiveSeconds => "forward_5s",
            Self::PlayPause => "play_pause",
            Self::GoLive => "go_live",
            Self::Quit => "quit",
        }
    }
    fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase().replace('-', "_");
        let name = match name.as_str() {
            "submit" => "execute",
            "direct" | "toggle_input_target" => "toggle_direct",
            "previous" => "previous_event",
            "next" => "next_event",
            "back_five_seconds" => "back_5s",
            "forward_five_seconds" => "forward_5s",
            "play" => "play_pause",
            "live" => "go_live",
            name => name,
        };
        Self::ALL
            .into_iter()
            .find(|action| action.config_name() == name)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Shortcut {
    key: String,
    shift: bool,
    alt: bool,
    control: bool,
    platform: bool,
}
impl Shortcut {
    fn parse(value: &str) -> Result<Self> {
        let text = value.trim().to_ascii_lowercase();
        let key = Keystroke::parse(&text)
            .map_err(|error| anyhow::anyhow!("invalid shortcut `{text}`: {error}"))?;
        Ok(Self::from_keystroke(&key))
    }
    fn from_keystroke(key: &Keystroke) -> Self {
        Self {
            key: if key.key == "return" {
                "enter".into()
            } else {
                key.key.to_ascii_lowercase()
            },
            shift: key.modifiers.shift,
            alt: key.modifiers.alt,
            control: key.modifiers.control,
            platform: key.modifiers.platform,
        }
    }
    fn specification(&self) -> String {
        let mut parts = Vec::new();
        if self.platform {
            parts.push("cmd");
        }
        if self.control {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        parts.push(&self.key);
        parts.join("-")
    }
    fn display(&self) -> String {
        self.specification()
            .split('-')
            .map(|part| match part {
                "cmd" => "Cmd".into(),
                "ctrl" => "Ctrl".into(),
                "alt" => "Alt".into(),
                "shift" => "Shift".into(),
                "enter" => "Enter".into(),
                "space" => "Space".into(),
                other => other.to_ascii_uppercase(),
            })
            .collect::<Vec<String>>()
            .join("+")
    }
}

#[derive(Clone, Copy)]
enum Platform {
    Mac,
    Other,
}
impl Platform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}

pub struct Keymap {
    bindings: HashMap<Action, Vec<Shortcut>>,
}
impl Keymap {
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = config_path() else {
            return (Self::defaults_for(Platform::current()), None);
        };
        match fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Self::defaults_for(Platform::current()), None)
            }
            result => match result
                .map_err(anyhow::Error::from)
                .and_then(|text| Self::parse_overrides(Platform::current(), &text))
            {
                Ok(keymap) => (keymap, None),
                Err(error) => (
                    Self::defaults_for(Platform::current()),
                    Some(format!(
                        "Keybindings {} ignored: {error}. Using OS defaults.",
                        path.display()
                    )),
                ),
            },
        }
    }
    fn defaults_for(platform: Platform) -> Self {
        let modifier = if matches!(platform, Platform::Mac) {
            "cmd"
        } else {
            "ctrl"
        };
        let mut bindings = HashMap::new();
        for (action, key) in [
            (Action::Copy, "c"),
            (Action::Cut, "x"),
            (Action::Paste, "v"),
            (Action::Undo, "z"),
            (Action::Redo, "shift-z"),
            (Action::SelectAll, "a"),
            (Action::Find, "f"),
            (Action::FocusEditor, "l"),
        ] {
            bindings.insert(
                action,
                vec![Shortcut::parse(&format!("{modifier}-{key}")).unwrap()],
            );
        }
        if matches!(platform, Platform::Other) {
            bindings
                .get_mut(&Action::Redo)
                .unwrap()
                .push(Shortcut::parse("ctrl-y").unwrap());
        }
        for (action, key) in [
            (Action::CopyDocument, "f10"),
            (Action::Execute, "ctrl-enter"),
            (Action::ToggleDirect, "ctrl-shift-space"),
            (Action::ToggleBlocks, "ctrl-shift-b"),
            (Action::Complete, "ctrl-space"),
            (Action::PreviousEvent, "f6"),
            (Action::NextEvent, "f7"),
            (Action::BackFiveSeconds, "shift-f6"),
            (Action::ForwardFiveSeconds, "shift-f7"),
            (Action::PlayPause, "f8"),
            (Action::GoLive, "f9"),
            (
                Action::Interrupt,
                if matches!(platform, Platform::Mac) {
                    "ctrl-c"
                } else {
                    "ctrl-shift-c"
                },
            ),
            (
                Action::Quit,
                if matches!(platform, Platform::Mac) {
                    "cmd-q"
                } else {
                    "ctrl-shift-q"
                },
            ),
        ] {
            bindings.insert(action, vec![Shortcut::parse(key).unwrap()]);
        }
        Self { bindings }
    }
    fn parse_overrides(platform: Platform, text: &str) -> Result<Self> {
        let mut keymap = Self::defaults_for(platform);
        for (number, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let (name, value) = line
                .split_once('=')
                .with_context(|| format!("line {} needs action = shortcut", number + 1))?;
            let action =
                Action::parse(name).with_context(|| format!("unknown action `{}`", name.trim()))?;
            let values = if value.trim().eq_ignore_ascii_case("none") {
                vec![]
            } else {
                value
                    .split(',')
                    .map(Shortcut::parse)
                    .collect::<Result<Vec<_>>>()?
            };
            keymap.bindings.insert(action, values);
        }
        let mut seen = HashMap::new();
        for action in Action::ALL {
            for shortcut in &keymap.bindings[&action] {
                if let Some(previous) = seen.insert(shortcut, action) {
                    anyhow::bail!(
                        "{} is assigned to both {} and {}",
                        shortcut.display(),
                        previous.config_name(),
                        action.config_name()
                    );
                }
            }
        }
        Ok(keymap)
    }
    pub fn action_for(&self, key: &Keystroke) -> Option<Action> {
        let shortcut = Shortcut::from_keystroke(key);
        Action::ALL
            .into_iter()
            .find(|action| self.bindings[action].contains(&shortcut))
    }
    pub fn label(&self, action: Action) -> String {
        self.bindings
            .get(&action)
            .and_then(|values| values.first())
            .map_or_else(|| "unbound".into(), Shortcut::display)
    }
    pub fn install(&self, cx: &mut gpui::App) {
        cx.bind_keys(self.gpui_bindings());
    }

    /// Overlay bindings at the component's context depth; actions precede raw keys.
    pub fn gpui_bindings(&self) -> Vec<KeyBinding> {
        let defaults = Self::defaults_for(Platform::current());
        // Root's Tab traversal must not steal a terminal child's input.
        let mut result = vec![
            KeyBinding::new("tab", NoAction, Some("KeaTerminal")),
            KeyBinding::new("shift-tab", NoAction, Some("KeaTerminal")),
        ];
        for action in [
            Action::Copy,
            Action::Cut,
            Action::Paste,
            Action::Undo,
            Action::Redo,
            Action::SelectAll,
            Action::Find,
        ] {
            for shortcut in &defaults.bindings[&action] {
                result.push(KeyBinding::new(
                    &shortcut.specification(),
                    NoAction,
                    Some("Kea > Input"),
                ));
            }
        }
        for action in Action::ALL {
            let contexts: &[&str] = match action {
                Action::Execute | Action::Complete => &["KeaCommand > Input"],
                Action::Copy | Action::Paste => &["Kea > Input"],
                Action::Cut | Action::Undo | Action::Redo | Action::SelectAll | Action::Find => {
                    &["Kea > Input"]
                }
                Action::FocusEditor => &["KeaDocument", "KeaDocument > Input"],
                _ => &["Kea", "Kea > Input"],
            };
            for shortcut in &self.bindings[&action] {
                for context in contexts {
                    result.push(KeyBinding::new(
                        &shortcut.specification(),
                        Invoke { action },
                        Some(context),
                    ));
                }
            }
        }
        // Terminal focus is an application-owned input surface, not an editor
        // context. Mask every Kea accelerator (including user overrides) at this
        // deeper context so the raw key handler receives it. Toolbar controls
        // remain clickable; OS/window-manager shortcuts remain outside our control.
        for keymap in [&defaults, self] {
            for shortcuts in keymap.bindings.values() {
                for shortcut in shortcuts {
                    result.push(KeyBinding::new(
                        &shortcut.specification(),
                        NoAction,
                        Some("KeaTerminal"),
                    ));
                }
            }
        }
        result
    }
}

#[derive(gpui::Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = kea, no_json)]
pub struct Invoke {
    pub action: Action,
}

pub(crate) fn config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("KEA_KEYBINDINGS") {
        return Some(path.into());
    }
    if cfg!(target_os = "windows") {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Kea/keybindings.conf"));
    }
    if cfg!(target_os = "macos") {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support/Kea/keybindings.conf"));
    }
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".config"))
        })
        .map(|path| path.join("kea/keybindings.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(spec: &str) -> Keystroke {
        Keystroke::parse(spec).unwrap()
    }
    #[test]
    fn platform_defaults_separate_copy_from_interrupt() {
        let linux = Keymap::defaults_for(Platform::Other);
        let mac = Keymap::defaults_for(Platform::Mac);
        assert_eq!(linux.action_for(&key("ctrl-c")), Some(Action::Copy));
        assert_eq!(
            linux.action_for(&key("ctrl-shift-c")),
            Some(Action::Interrupt)
        );
        assert_eq!(mac.action_for(&key("cmd-c")), Some(Action::Copy));
        assert_eq!(mac.action_for(&key("ctrl-c")), Some(Action::Interrupt));
        assert_eq!(linux.action_for(&key("ctrl-enter")), Some(Action::Execute));
    }
    #[test]
    fn configuration_remaps_unbinds_and_rejects_ambiguity() {
        let map = Keymap::parse_overrides(
            Platform::Other,
            "copy = ctrl-shift-c\ninterrupt = ctrl-c\nexecute = alt-enter\nundo = none",
        )
        .unwrap();
        assert_eq!(map.action_for(&key("ctrl-c")), Some(Action::Interrupt));
        assert_eq!(map.action_for(&key("ctrl-shift-c")), Some(Action::Copy));
        assert_eq!(map.action_for(&key("alt-enter")), Some(Action::Execute));
        assert_eq!(map.action_for(&key("ctrl-z")), None);
        assert!(
            Keymap::parse_overrides(Platform::Other, "copy = ctrl-c\ninterrupt = ctrl-c").is_err()
        );
    }
    #[test]
    fn editing_bindings_do_not_capture_direct_terminal_controls() {
        // Stand-ins exercise the same parent-context precedence as Root's private
        // focus traversal actions without coupling to their private Rust types.
        let mut map = gpui::Keymap::new(vec![
            KeyBinding::new("tab", gpui_component::input::MoveDown, Some("Root")),
            KeyBinding::new("shift-tab", gpui_component::input::MoveUp, Some("Root")),
        ]);
        map.add_bindings(Keymap::defaults_for(Platform::current()).gpui_bindings());
        let context = [
            gpui::KeyContext::parse("Root").unwrap(),
            gpui::KeyContext::parse("Kea").unwrap(),
            gpui::KeyContext::parse("KeaTerminal").unwrap(),
        ];
        for spec in [
            "ctrl-z",
            "ctrl-enter",
            "ctrl-l",
            "ctrl-c",
            "ctrl-v",
            "ctrl-shift-q",
            "ctrl-shift-space",
            "ctrl-space",
            "f6",
            "f7",
            "f8",
            "f9",
            "f10",
            "tab",
            "shift-tab",
        ] {
            assert!(
                map.bindings_for_input(&[key(spec)], &context).0.is_empty(),
                "captured {spec}"
            );
        }
    }
    #[test]
    fn unbinding_copy_really_masks_the_component_default() {
        let modifier = if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        };
        let mut map = gpui::Keymap::new(vec![KeyBinding::new(
            modifier,
            gpui_component::input::Copy,
            Some("Input"),
        )]);
        let override_map = Keymap::parse_overrides(Platform::current(), "copy = none").unwrap();
        map.add_bindings(override_map.gpui_bindings());
        let context = [
            gpui::KeyContext::parse("Kea").unwrap(),
            gpui::KeyContext::parse("Input").unwrap(),
        ];
        assert!(map
            .bindings_for_input(&[key(modifier)], &context)
            .0
            .is_empty());
    }
}
