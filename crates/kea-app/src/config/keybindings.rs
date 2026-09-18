//! Configurable semantic actions. Ordinary text input/navigation stay with the editor.
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
    RunShell,
    SendApplication,
    Newline,
    Complete,
    ReverseSearch,
    ToggleBlocks,
    PreviousEvent,
    NextEvent,
    BackFiveSeconds,
    ForwardFiveSeconds,
    PlayPause,
    GoLive,
    Quit,
    SelectTerminalText,
}

impl Action {
    const ALL: [Self; 24] = [
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
        Self::RunShell,
        Self::SendApplication,
        Self::Newline,
        Self::Complete,
        Self::ReverseSearch,
        Self::ToggleBlocks,
        Self::PreviousEvent,
        Self::NextEvent,
        Self::BackFiveSeconds,
        Self::ForwardFiveSeconds,
        Self::PlayPause,
        Self::GoLive,
        Self::Quit,
        Self::SelectTerminalText,
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
            Self::RunShell => "run_shell",
            Self::SendApplication => "send_application",
            Self::Newline => "newline",
            Self::Complete => "complete",
            Self::ReverseSearch => "reverse_search",
            Self::ToggleBlocks => "toggle_blocks",
            Self::PreviousEvent => "previous_event",
            Self::NextEvent => "next_event",
            Self::BackFiveSeconds => "back_5s",
            Self::ForwardFiveSeconds => "forward_5s",
            Self::PlayPause => "play_pause",
            Self::GoLive => "go_live",
            Self::Quit => "quit",
            Self::SelectTerminalText => "select_terminal_text",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase().replace('-', "_");
        let canonical = match name.as_str() {
            "execute" | "submit" => "run_shell",
            "send_text" => "send_application",
            "direct" | "toggle_direct" | "toggle_input_target" => "toggle_blocks",
            "previous" => "previous_event",
            "next" => "next_event",
            "back_five_seconds" => "back_5s",
            "forward_five_seconds" => "forward_5s",
            "play" => "play_pause",
            "live" => "go_live",
            other => other,
        };
        Self::ALL
            .into_iter()
            .find(|action| action.config_name() == canonical)
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
    previous_draft: Vec<Shortcut>,
    next_draft: Vec<Shortcut>,
    focus_terminal: Vec<Shortcut>,
}

impl Keymap {
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = config_path() else {
            return (Self::defaults_for(Platform::current()), None);
        };
        match fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Manual acceptance A3: users had to hand-create the config
                // directory and file before they could remap anything. Leave a
                // commented starting point behind instead. Never overwrite.
                if let Err(error) = ensure_default_file(&path, DEFAULT_KEYBINDINGS_CONF) {
                    return (
                        Self::defaults_for(Platform::current()),
                        Some(format!(
                            "Keybindings {} unavailable: {error}. Using OS/editor defaults.",
                            path.display()
                        )),
                    );
                }
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
                        "Keybindings {} ignored: {error}. Using OS/editor defaults.",
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
        // Clipboard paste via Ctrl+Shift+V works in the composer and, through
        // the terminal key handler, as explicit terminal paste. Plain Ctrl+V
        // stays ordinary child input while the live terminal owns the keyboard.
        bindings
            .get_mut(&Action::Paste)
            .unwrap()
            .push(Shortcut::parse("ctrl-shift-v").unwrap());
        for (action, key) in [
            (Action::CopyDocument, "f10"),
            (Action::RunShell, "ctrl-enter"),
            (Action::SendApplication, "ctrl-shift-enter"),
            (Action::Complete, "tab"),
            (Action::ReverseSearch, "ctrl-r"),
            (Action::ToggleBlocks, "ctrl-shift-space"),
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
            (Action::SelectTerminalText, "f4"),
        ] {
            bindings.insert(action, vec![Shortcut::parse(key).unwrap()]);
        }
        bindings.insert(Action::Newline, Vec::new());
        Self {
            bindings,
            previous_draft: vec![Shortcut::parse("ctrl-up").unwrap()],
            next_draft: vec![Shortcut::parse("ctrl-down").unwrap()],
            focus_terminal: vec![Shortcut::parse(&format!("{modifier}-shift-l")).unwrap()],
        }
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
            let values = if value.trim().eq_ignore_ascii_case("none") {
                vec![]
            } else {
                value
                    .split(',')
                    .map(Shortcut::parse)
                    .collect::<Result<Vec<_>>>()?
            };
            match name.trim().to_ascii_lowercase().replace('-', "_").as_str() {
                "previous_draft" => keymap.previous_draft = values,
                "next_draft" => keymap.next_draft = values,
                "focus_terminal" => keymap.focus_terminal = values,
                _ => {
                    let action = Action::parse(name)
                        .with_context(|| format!("unknown action `{}`", name.trim()))?;
                    keymap.bindings.insert(action, values);
                }
            }
        }

        let mut seen: HashMap<Shortcut, String> = HashMap::new();
        for action in Action::ALL {
            for shortcut in &keymap.bindings[&action] {
                if let Some(previous) = seen.insert(shortcut.clone(), action.config_name().into()) {
                    anyhow::bail!(
                        "{} is assigned to both {} and {}",
                        shortcut.display(),
                        previous,
                        action.config_name()
                    );
                }
            }
        }
        for (name, shortcuts) in [
            ("previous_draft", &keymap.previous_draft),
            ("next_draft", &keymap.next_draft),
            ("focus_terminal", &keymap.focus_terminal),
        ] {
            for shortcut in shortcuts {
                // `focus_terminal` acts only while the composer owns focus, while
                // `focus_editor` is the terminal escape that now toggles back from
                // the composer. Sharing one chord (e.g. Ctrl+L both ways) is an
                // intentional single switch key across disjoint focus contexts,
                // not an ambiguous double binding.
                if name == "focus_terminal"
                    && keymap.bindings[&Action::FocusEditor].contains(shortcut)
                {
                    continue;
                }
                if let Some(previous) = seen.insert(shortcut.clone(), name.into()) {
                    anyhow::bail!(
                        "{} is assigned to both {} and {}",
                        shortcut.display(),
                        previous,
                        name
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
            .and_then(|v| v.first())
            .map_or_else(|| "unbound".into(), Shortcut::display)
    }
    pub fn label_or(&self, action: Action, fallback: &str) -> String {
        self.bindings
            .get(&action)
            .and_then(|v| v.first())
            .map_or_else(|| fallback.into(), Shortcut::display)
    }

    pub fn install(&self, cx: &mut gpui::App) {
        cx.bind_keys(self.gpui_bindings());
        let previous = self.previous_draft.clone();
        let next = self.next_draft.clone();
        let focus_terminal = self.focus_terminal.clone();
        let subscription = cx.intercept_keystrokes(move |event, window, cx| {
            // When a real terminal/TUI owns input, remember that exact focus handle but
            // do not reserve any extra child key. The explicit focus_terminal shortcut
            // acts only when the Kea command editor owns focus.
            crate::editor::command::remember_external_focus(window, cx);
            let shortcut = Shortcut::from_keystroke(&event.keystroke);
            if focus_terminal.contains(&shortcut)
                && crate::editor::command::focus_last_external(window, cx)
            {
                cx.stop_propagation();
                return;
            }
            let direction = if previous.contains(&shortcut) {
                Some(crate::editor::command::HistoryDirection::Previous)
            } else if next.contains(&shortcut) {
                Some(crate::editor::command::HistoryDirection::Next)
            } else {
                None
            };
            if direction.is_some_and(|direction| {
                crate::editor::command::navigate_submitted_drafts(direction, window, cx)
            }) {
                cx.stop_propagation();
            }
        });
        crate::editor::command::retain_history_interceptor(subscription, cx);
    }

    /// The child owns ordinary terminal input, but `focus_editor` is Kea's explicit
    /// configurable host escape and single switch key: from the live terminal it
    /// focuses the composer, and from the composer the same chord returns to the
    /// terminal. Without it, submitting a draft strands a keyboard-only user in
    /// the terminal. It is the only Kea accelerator reserved while the terminal
    /// owns input; the reverse `focus_terminal` shortcut acts only from composer
    /// focus and never steals child keys. Users whose child needs Ctrl/Cmd+L can
    /// remap or unbind it.
    pub fn gpui_bindings(&self) -> Vec<KeyBinding> {
        let defaults = Self::defaults_for(Platform::current());
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
                Action::RunShell | Action::SendApplication | Action::Newline | Action::Complete => {
                    &["KeaCommand > Input"]
                }
                Action::FocusEditor => &["Kea > Input", "KeaChrome", "KeaTerminal"],
                Action::SelectTerminalText => &["Kea > Input", "KeaChrome"],
                Action::ReverseSearch => &[
                    "KeaCommand > Input",
                    "KeaReverseSearch",
                    "KeaReverseSearch > Input",
                ],
                _ => &["Kea > Input", "KeaChrome"],
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
        for keymap in [&defaults, self] {
            for (action, shortcuts) in &keymap.bindings {
                if *action == Action::FocusEditor {
                    continue;
                }
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
#[action(namespace=kea,no_json)]
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
            .map(|p| p.join("Kea/keybindings.conf"));
    }
    if cfg!(target_os = "macos") {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .map(|p| p.join("Library/Application Support/Kea/keybindings.conf"));
    }
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".config"))
        })
        .map(|p| p.join("kea/keybindings.conf"))
}

/// Commented starting point written next to `settings.conf` when neither file
/// exists yet. Every line is a comment, so a fresh file behaves exactly like
/// built-in defaults; uncomment a line to override it. Restart Kea after
/// editing for now.
const DEFAULT_KEYBINDINGS_CONF: &str = r#"# Kea keybindings: one `action = shortcut, ...` per line, `#` starts a comment.
# Shortcuts look like ctrl-l, ctrl-shift-enter, alt-up or f10. `none` unbinds.
# Unknown actions or ambiguous shortcuts fall back to defaults with a warning.
# focus_editor = ctrl-l
#   Single switch key: terminal -> composer, and composer -> terminal.
# focus_terminal = ctrl-shift-l
#   Explicit composer -> terminal alternative; never reserved while a TUI owns input.
# run_shell = ctrl-enter
# send_application = ctrl-shift-enter
# previous_draft = ctrl-up
# next_draft = ctrl-down
# reverse_search = ctrl-r
# copy_document = f10
# select_terminal_text = f4
"#;

/// Starting point for `settings.conf`, kept next to the keybindings file.
pub(crate) const DEFAULT_SETTINGS_CONF: &str = r#"# Kea settings: one `key = value` per line, `#` starts a comment.
# post_submit_focus = editor
# theme = system
# soft_wrap = true
# show_blocks = false
# persist_history = false
#   Keep submitted drafts across restarts. The history file is plaintext and
#   owner-only on Unix. Submissions can contain secrets; persistence is opt-in.
# history_persistence = false
#   Separate Ctrl-R search history persistence; takes effect on next launch.
#   Explicit named memories persist independently of this setting.
# shift_mouse_selects_locally = true
#   Shift owns local selection gestures while the child reports mouse input.
#   Set false to forward them; the Select text button remains available.
# animate_logo = true
#   Set false to keep the decorative composer bird still while typing.
"#;

/// Create the config directory and write a default file when nothing exists
/// yet. Returns `true` when the file was created. Existing files are never
/// touched; any other failure is reported so callers can fall back visibly.
pub(crate) fn ensure_default_file(path: &std::path::Path, template: &str) -> std::io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // `create_new` keeps this a no-op when another Kea instance wins the race.
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write as _;
            file.write_all(template.as_bytes())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/keybindings.rs"]
mod tests;
