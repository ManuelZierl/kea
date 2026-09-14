use anyhow::{Context as _, Result};
use gpui::Keystroke;
use std::{collections::HashMap, env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    Copy,
    Paste,
    Interrupt,
    Execute,
    ToggleDirect,
    PreviousEvent,
    NextEvent,
    BackFiveSeconds,
    ForwardFiveSeconds,
    PlayPause,
    GoLive,
    Quit,
}

impl Action {
    const ALL: [Self; 12] = [
        Self::Copy,
        Self::Paste,
        Self::Interrupt,
        Self::Execute,
        Self::ToggleDirect,
        Self::PreviousEvent,
        Self::NextEvent,
        Self::BackFiveSeconds,
        Self::ForwardFiveSeconds,
        Self::PlayPause,
        Self::GoLive,
        Self::Quit,
    ];

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "copy" => Some(Self::Copy),
            "paste" => Some(Self::Paste),
            "interrupt" => Some(Self::Interrupt),
            "execute" | "submit" => Some(Self::Execute),
            "toggle_direct" | "direct" => Some(Self::ToggleDirect),
            "previous_event" | "previous" => Some(Self::PreviousEvent),
            "next_event" | "next" => Some(Self::NextEvent),
            "back_five_seconds" | "back_5s" => Some(Self::BackFiveSeconds),
            "forward_five_seconds" | "forward_5s" => Some(Self::ForwardFiveSeconds),
            "play_pause" | "play" => Some(Self::PlayPause),
            "go_live" | "live" => Some(Self::GoLive),
            "quit" => Some(Self::Quit),
            _ => None,
        }
    }

    fn config_name(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::Interrupt => "interrupt",
            Self::Execute => "execute",
            Self::ToggleDirect => "toggle_direct",
            Self::PreviousEvent => "previous_event",
            Self::NextEvent => "next_event",
            Self::BackFiveSeconds => "back_5s",
            Self::ForwardFiveSeconds => "forward_5s",
            Self::PlayPause => "play_pause",
            Self::GoLive => "go_live",
            Self::Quit => "quit",
        }
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
        let value = value.trim().to_ascii_lowercase();
        let key = Keystroke::parse(&value)
            .map_err(|error| anyhow::anyhow!("invalid shortcut `{value}`: {error}"))?;
        Ok(Self::from_keystroke(&key))
    }

    fn from_keystroke(key: &Keystroke) -> Self {
        Self {
            key: key.key.to_ascii_lowercase(),
            shift: key.modifiers.shift,
            alt: key.modifiers.alt,
            control: key.modifiers.control,
            platform: key.modifiers.platform,
        }
    }

    fn matches(&self, key: &Keystroke) -> bool {
        self == &Self::from_keystroke(key)
    }

    fn display(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.platform {
            parts.push("Cmd".into());
        }
        if self.control {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        parts.push(match self.key.as_str() {
            "enter" | "return" => "Enter".to_string(),
            "space" => "Space".to_string(),
            value if value.starts_with('f') => value.to_ascii_uppercase(),
            value if value.len() == 1 => value.to_ascii_uppercase(),
            value => value.to_string(),
        });
        parts.join("+")
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
    config_path: Option<PathBuf>,
}

impl Keymap {
    pub fn load() -> (Self, Option<String>) {
        let platform = Platform::current();
        let defaults = Self::defaults_for(platform);
        let Some(path) = config_path() else {
            return (defaults, None);
        };
        if !path.exists() {
            let mut defaults = defaults;
            defaults.config_path = Some(path);
            return (defaults, None);
        }
        match fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))
            .and_then(|text| Self::parse_overrides(platform, &text, Some(path.clone())))
        {
            Ok(keymap) => (keymap, None),
            Err(error) => {
                let mut defaults = defaults;
                defaults.config_path = Some(path);
                (
                    defaults,
                    Some(format!(
                        "KEYBINDINGS CONFIG IGNORED: {error}. Using OS defaults."
                    )),
                )
            }
        }
    }

    fn defaults_for(platform: Platform) -> Self {
        let mut bindings = HashMap::new();
        let specs: &[(Action, &str)] = match platform {
            Platform::Mac => &[
                (Action::Copy, "cmd-c"),
                (Action::Paste, "cmd-v"),
                (Action::Interrupt, "ctrl-c"),
                (Action::Execute, "ctrl-enter"),
                (Action::ToggleDirect, "ctrl-shift-space"),
                (Action::PreviousEvent, "f6"),
                (Action::NextEvent, "f7"),
                (Action::BackFiveSeconds, "shift-f6"),
                (Action::ForwardFiveSeconds, "shift-f7"),
                (Action::PlayPause, "f8"),
                (Action::GoLive, "f9"),
                (Action::Quit, "cmd-q"),
            ],
            Platform::Other => &[
                (Action::Copy, "ctrl-c"),
                (Action::Paste, "ctrl-v"),
                (Action::Interrupt, "ctrl-shift-c"),
                (Action::Execute, "ctrl-enter"),
                (Action::ToggleDirect, "ctrl-shift-space"),
                (Action::PreviousEvent, "f6"),
                (Action::NextEvent, "f7"),
                (Action::BackFiveSeconds, "shift-f6"),
                (Action::ForwardFiveSeconds, "shift-f7"),
                (Action::PlayPause, "f8"),
                (Action::GoLive, "f9"),
                (Action::Quit, "ctrl-shift-q"),
            ],
        };
        for (action, spec) in specs {
            bindings.insert(*action, vec![Shortcut::parse(spec).expect("default keybinding")]);
        }
        Self {
            bindings,
            config_path: config_path(),
        }
    }

    fn parse_overrides(platform: Platform, text: &str, path: Option<PathBuf>) -> Result<Self> {
        let mut keymap = Self::defaults_for(platform);
        keymap.config_path = path;
        for (index, raw_line) in text.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let (name, value) = line
                .split_once('=')
                .with_context(|| format!("line {} needs `action = shortcut`", index + 1))?;
            let action = Action::parse(name)
                .with_context(|| format!("line {} has unknown action `{}`", index + 1, name.trim()))?;
            let value = value.trim();
            let shortcuts = if value.eq_ignore_ascii_case("none") {
                Vec::new()
            } else {
                value
                    .split(',')
                    .map(Shortcut::parse)
                    .collect::<Result<Vec<_>>>()?
            };
            keymap.bindings.insert(action, shortcuts);
        }
        keymap.validate()?;
        Ok(keymap)
    }

    fn validate(&self) -> Result<()> {
        let mut seen: HashMap<&Shortcut, Action> = HashMap::new();
        for action in Action::ALL {
            if let Some(shortcuts) = self.bindings.get(&action) {
                for shortcut in shortcuts {
                    if let Some(previous) = seen.insert(shortcut, action) {
                        anyhow::bail!(
                            "shortcut {} is assigned to both `{}` and `{}`",
                            shortcut.display(),
                            previous.config_name(),
                            action.config_name()
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn action_for(&self, key: &Keystroke) -> Option<Action> {
        Action::ALL.into_iter().find(|action| {
            self.bindings
                .get(action)
                .is_some_and(|shortcuts| shortcuts.iter().any(|shortcut| shortcut.matches(key)))
        })
    }

    pub fn label(&self, action: Action) -> String {
        self.bindings
            .get(&action)
            .and_then(|shortcuts| shortcuts.first())
            .map_or_else(|| "unbound".into(), Shortcut::display)
    }
}

fn config_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("KEA_KEYBINDINGS") {
        return Some(PathBuf::from(path));
    }
    if cfg!(target_os = "windows") {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Kea").join("keybindings.conf"));
    }
    if cfg!(target_os = "macos") {
        return env::var_os("HOME").map(PathBuf::from).map(|path| {
            path.join("Library")
                .join("Application Support")
                .join("Kea")
                .join("keybindings.conf")
        });
    }
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from).map(|path| path.join(".config")))
        .map(|path| path.join("kea").join("keybindings.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_defaults_make_copy_native_and_interrupt_explicit() {
        let keymap = Keymap::defaults_for(Platform::Other);
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-c").unwrap()),
            Some(Action::Copy)
        );
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-shift-c").unwrap()),
            Some(Action::Interrupt)
        );
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-enter").unwrap()),
            Some(Action::Execute)
        );
    }

    #[test]
    fn mac_defaults_keep_ctrl_c_for_interrupt() {
        let keymap = Keymap::defaults_for(Platform::Mac);
        assert_eq!(
            keymap.action_for(&Keystroke::parse("cmd-c").unwrap()),
            Some(Action::Copy)
        );
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-c").unwrap()),
            Some(Action::Interrupt)
        );
    }

    #[test]
    fn config_can_restore_traditional_terminal_copy_interrupt() {
        let keymap = Keymap::parse_overrides(
            Platform::Other,
            "copy = ctrl-shift-c\ninterrupt = ctrl-c\nexecute = alt-enter\n",
            None,
        )
        .unwrap();
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-c").unwrap()),
            Some(Action::Interrupt)
        );
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-shift-c").unwrap()),
            Some(Action::Copy)
        );
        assert_eq!(
            keymap.action_for(&Keystroke::parse("alt-enter").unwrap()),
            Some(Action::Execute)
        );
    }

    #[test]
    fn duplicate_shortcuts_are_rejected_and_actions_can_be_unbound() {
        assert!(Keymap::parse_overrides(
            Platform::Other,
            "copy = ctrl-c\ninterrupt = ctrl-c\n",
            None,
        )
        .is_err());
        let keymap = Keymap::parse_overrides(Platform::Other, "copy = none\n", None).unwrap();
        assert_eq!(
            keymap.action_for(&Keystroke::parse("ctrl-c").unwrap()),
            None
        );
    }
}
