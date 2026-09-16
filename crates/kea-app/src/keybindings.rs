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
    ToggleBlocks,
    PreviousEvent,
    NextEvent,
    BackFiveSeconds,
    ForwardFiveSeconds,
    PlayPause,
    GoLive,
    Quit,
}

impl Action {
    const ALL: [Self; 22] = [
        Self::Copy, Self::Cut, Self::Paste, Self::Undo, Self::Redo, Self::SelectAll,
        Self::Find, Self::CopyDocument, Self::FocusEditor, Self::Interrupt, Self::RunShell,
        Self::SendApplication, Self::Newline, Self::Complete, Self::ToggleBlocks,
        Self::PreviousEvent, Self::NextEvent, Self::BackFiveSeconds, Self::ForwardFiveSeconds,
        Self::PlayPause, Self::GoLive, Self::Quit,
    ];

    fn config_name(self) -> &'static str {
        match self {
            Self::Copy => "copy", Self::Cut => "cut", Self::Paste => "paste",
            Self::Undo => "undo", Self::Redo => "redo", Self::SelectAll => "select_all",
            Self::Find => "find", Self::CopyDocument => "copy_document",
            Self::FocusEditor => "focus_editor", Self::Interrupt => "interrupt",
            Self::RunShell => "run_shell", Self::SendApplication => "send_application",
            Self::Newline => "newline", Self::Complete => "complete",
            Self::ToggleBlocks => "toggle_blocks", Self::PreviousEvent => "previous_event",
            Self::NextEvent => "next_event", Self::BackFiveSeconds => "back_5s",
            Self::ForwardFiveSeconds => "forward_5s", Self::PlayPause => "play_pause",
            Self::GoLive => "go_live", Self::Quit => "quit",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase().replace('-', "_");
        let canonical = match name.as_str() {
            "execute" | "submit" => "run_shell",
            "send_text" => "send_application",
            "direct" | "toggle_direct" | "toggle_input_target" => "toggle_blocks",
            "previous" => "previous_event", "next" => "next_event",
            "back_five_seconds" => "back_5s", "forward_five_seconds" => "forward_5s",
            "play" => "play_pause", "live" => "go_live", other => other,
        };
        Self::ALL.into_iter().find(|action| action.config_name() == canonical)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Shortcut { key: String, shift: bool, alt: bool, control: bool, platform: bool }

impl Shortcut {
    fn parse(value: &str) -> Result<Self> {
        let text = value.trim().to_ascii_lowercase();
        let key = Keystroke::parse(&text)
            .map_err(|error| anyhow::anyhow!("invalid shortcut `{text}`: {error}"))?;
        Ok(Self::from_keystroke(&key))
    }
    fn from_keystroke(key: &Keystroke) -> Self {
        Self { key: if key.key == "return" { "enter".into() } else { key.key.to_ascii_lowercase() },
            shift: key.modifiers.shift, alt: key.modifiers.alt, control: key.modifiers.control,
            platform: key.modifiers.platform }
    }
    fn specification(&self) -> String {
        let mut parts = Vec::new();
        if self.platform { parts.push("cmd"); } if self.control { parts.push("ctrl"); }
        if self.alt { parts.push("alt"); } if self.shift { parts.push("shift"); }
        parts.push(&self.key); parts.join("-")
    }
    fn display(&self) -> String {
        self.specification().split('-').map(|part| match part {
            "cmd" => "Cmd".into(), "ctrl" => "Ctrl".into(), "alt" => "Alt".into(),
            "shift" => "Shift".into(), "enter" => "Enter".into(), "space" => "Space".into(),
            other => other.to_ascii_uppercase(),
        }).collect::<Vec<String>>().join("+")
    }
}

#[derive(Clone, Copy)] enum Platform { Mac, Other }
impl Platform { fn current() -> Self { if cfg!(target_os = "macos") { Self::Mac } else { Self::Other } } }

pub struct Keymap {
    bindings: HashMap<Action, Vec<Shortcut>>,
    previous_draft: Vec<Shortcut>,
    next_draft: Vec<Shortcut>,
}

impl Keymap {
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = config_path() else { return (Self::defaults_for(Platform::current()), None); };
        match fs::read_to_string(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Self::defaults_for(Platform::current()), None),
            result => match result.map_err(anyhow::Error::from).and_then(|text| Self::parse_overrides(Platform::current(), &text)) {
                Ok(keymap) => (keymap, None),
                Err(error) => (Self::defaults_for(Platform::current()), Some(format!("Keybindings {} ignored: {error}. Using OS/editor defaults.", path.display()))),
            },
        }
    }

    fn defaults_for(platform: Platform) -> Self {
        let modifier = if matches!(platform, Platform::Mac) { "cmd" } else { "ctrl" };
        let mut bindings = HashMap::new();
        for (action, key) in [(Action::Copy,"c"),(Action::Cut,"x"),(Action::Paste,"v"),(Action::Undo,"z"),(Action::Redo,"shift-z"),(Action::SelectAll,"a"),(Action::Find,"f"),(Action::FocusEditor,"l")] {
            bindings.insert(action, vec![Shortcut::parse(&format!("{modifier}-{key}")).unwrap()]);
        }
        if matches!(platform, Platform::Other) { bindings.get_mut(&Action::Redo).unwrap().push(Shortcut::parse("ctrl-y").unwrap()); }
        for (action,key) in [(Action::CopyDocument,"f10"),(Action::RunShell,"ctrl-enter"),(Action::SendApplication,"ctrl-shift-enter"),(Action::Complete,"tab"),(Action::ToggleBlocks,"ctrl-shift-space"),(Action::PreviousEvent,"f6"),(Action::NextEvent,"f7"),(Action::BackFiveSeconds,"shift-f6"),(Action::ForwardFiveSeconds,"shift-f7"),(Action::PlayPause,"f8"),(Action::GoLive,"f9"),(Action::Interrupt,if matches!(platform,Platform::Mac){"ctrl-c"}else{"ctrl-shift-c"}),(Action::Quit,if matches!(platform,Platform::Mac){"cmd-q"}else{"ctrl-shift-q"})] {
            bindings.insert(action, vec![Shortcut::parse(key).unwrap()]);
        }
        bindings.insert(Action::Newline, Vec::new());
        Self {
            bindings,
            previous_draft: vec![Shortcut::parse("ctrl-up").unwrap()],
            next_draft: vec![Shortcut::parse("ctrl-down").unwrap()],
        }
    }

    fn parse_overrides(platform: Platform, text: &str) -> Result<Self> {
        let mut keymap = Self::defaults_for(platform);
        for (number,line) in text.lines().enumerate() {
            let line=line.split('#').next().unwrap_or_default().trim(); if line.is_empty(){continue;}
            let (name,value)=line.split_once('=').with_context(||format!("line {} needs action = shortcut",number+1))?;
            let values=if value.trim().eq_ignore_ascii_case("none"){vec![]}else{value.split(',').map(Shortcut::parse).collect::<Result<Vec<_>>>()?};
            match name.trim().to_ascii_lowercase().replace('-', "_").as_str() {
                "previous_draft" => keymap.previous_draft = values,
                "next_draft" => keymap.next_draft = values,
                _ => {
                    let action=Action::parse(name).with_context(||format!("unknown action `{}`",name.trim()))?;
                    keymap.bindings.insert(action,values);
                }
            }
        }

        let mut seen: HashMap<Shortcut, String> = HashMap::new();
        for action in Action::ALL {
            for shortcut in &keymap.bindings[&action] {
                if let Some(previous)=seen.insert(shortcut.clone(),action.config_name().into()) {
                    anyhow::bail!("{} is assigned to both {} and {}",shortcut.display(),previous,action.config_name());
                }
            }
        }
        for (name, shortcuts) in [("previous_draft", &keymap.previous_draft), ("next_draft", &keymap.next_draft)] {
            for shortcut in shortcuts {
                if let Some(previous)=seen.insert(shortcut.clone(),name.into()) {
                    anyhow::bail!("{} is assigned to both {} and {}",shortcut.display(),previous,name);
                }
            }
        }
        Ok(keymap)
    }

    pub fn action_for(&self,key:&Keystroke)->Option<Action>{let shortcut=Shortcut::from_keystroke(key);Action::ALL.into_iter().find(|action|self.bindings[action].contains(&shortcut))}
    pub fn label(&self,action:Action)->String{self.bindings.get(&action).and_then(|v|v.first()).map_or_else(||"unbound".into(),Shortcut::display)}
    pub fn label_or(&self,action:Action,fallback:&str)->String{self.bindings.get(&action).and_then(|v|v.first()).map_or_else(||fallback.into(),Shortcut::display)}

    pub fn install(&self,cx:&mut gpui::App){
        cx.bind_keys(self.gpui_bindings());
        let previous = self.previous_draft.clone();
        let next = self.next_draft.clone();
        let subscription = cx.intercept_keystrokes(move |event, window, cx| {
            let shortcut = Shortcut::from_keystroke(&event.keystroke);
            let direction = if previous.contains(&shortcut) {
                Some(crate::command_editor::HistoryDirection::Previous)
            } else if next.contains(&shortcut) {
                Some(crate::command_editor::HistoryDirection::Next)
            } else {
                None
            };
            if direction.is_some_and(|direction| crate::command_editor::navigate_submitted_drafts(direction, window, cx)) {
                cx.stop_propagation();
            }
        });
        crate::command_editor::retain_history_interceptor(subscription, cx);
    }

    /// The child owns ordinary terminal input, but `focus_editor` is Kea's explicit
    /// configurable host escape. Without it, submitting a draft strands a keyboard-only
    /// user in the terminal. Users whose child needs Ctrl/Cmd+L can remap or unbind it.
    pub fn gpui_bindings(&self)->Vec<KeyBinding>{
        let defaults=Self::defaults_for(Platform::current());
        let mut result=vec![KeyBinding::new("tab",NoAction,Some("KeaTerminal")),KeyBinding::new("shift-tab",NoAction,Some("KeaTerminal"))];
        for action in [Action::Copy,Action::Cut,Action::Paste,Action::Undo,Action::Redo,Action::SelectAll,Action::Find] { for shortcut in &defaults.bindings[&action] { result.push(KeyBinding::new(&shortcut.specification(),NoAction,Some("Kea > Input"))); } }
        for action in Action::ALL {
            let contexts:&[&str]=match action { Action::RunShell|Action::SendApplication|Action::Newline|Action::Complete=>&["KeaCommand > Input"], Action::FocusEditor=>&["Kea > Input","KeaChrome","KeaTerminal"], _=>&["Kea > Input","KeaChrome"] };
            for shortcut in &self.bindings[&action] { for context in contexts { result.push(KeyBinding::new(&shortcut.specification(),Invoke{action},Some(context))); } }
        }
        for keymap in [&defaults,self] { for (action,shortcuts) in &keymap.bindings { if *action==Action::FocusEditor {continue;} for shortcut in shortcuts { result.push(KeyBinding::new(&shortcut.specification(),NoAction,Some("KeaTerminal"))); } } }
        result
    }
}

#[derive(gpui::Action,Clone,PartialEq,Eq,Deserialize)] #[action(namespace=kea,no_json)] pub struct Invoke{pub action:Action}

pub(crate) fn config_path()->Option<PathBuf>{
    if let Some(path)=env::var_os("KEA_KEYBINDINGS"){return Some(path.into());}
    if cfg!(target_os="windows"){return env::var_os("APPDATA").map(PathBuf::from).map(|p|p.join("Kea/keybindings.conf"));}
    if cfg!(target_os="macos"){return env::var_os("HOME").map(PathBuf::from).map(|p|p.join("Library/Application Support/Kea/keybindings.conf"));}
    env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(||env::var_os("HOME").map(PathBuf::from).map(|p|p.join(".config"))).map(|p|p.join("kea/keybindings.conf"))
}

#[cfg(test)] mod tests {
    use super::*; fn key(spec:&str)->Keystroke{Keystroke::parse(spec).unwrap()}
    #[test] fn defaults_are_editor_native_and_submission_actions_are_distinct(){let map=Keymap::defaults_for(Platform::Other);assert_eq!(map.action_for(&key("ctrl-enter")),Some(Action::RunShell));assert_eq!(map.action_for(&key("ctrl-shift-enter")),Some(Action::SendApplication));assert_eq!(map.action_for(&key("tab")),Some(Action::Complete));assert_eq!(map.previous_draft,vec![Shortcut::parse("ctrl-up").unwrap()]);assert_eq!(map.next_draft,vec![Shortcut::parse("ctrl-down").unwrap()]);assert_eq!(map.action_for(&key("enter")),None);assert_eq!(map.action_for(&key("shift-enter")),None);}
    #[test] fn terminal_like_enter_policy_is_configurable(){let map=Keymap::parse_overrides(Platform::Other,"run_shell = enter\nnewline = shift-enter").unwrap();assert_eq!(map.action_for(&key("enter")),Some(Action::RunShell));assert_eq!(map.action_for(&key("shift-enter")),Some(Action::Newline));}
    #[test] fn platform_defaults_separate_copy_from_interrupt(){let linux=Keymap::defaults_for(Platform::Other);let mac=Keymap::defaults_for(Platform::Mac);assert_eq!(linux.action_for(&key("ctrl-c")),Some(Action::Copy));assert_eq!(linux.action_for(&key("ctrl-shift-c")),Some(Action::Interrupt));assert_eq!(mac.action_for(&key("cmd-c")),Some(Action::Copy));assert_eq!(mac.action_for(&key("ctrl-c")),Some(Action::Interrupt));}
    #[test] fn configuration_remaps_unbinds_and_rejects_ambiguity(){let map=Keymap::parse_overrides(Platform::Other,"copy = ctrl-shift-c\ninterrupt = ctrl-c\nrun_shell = alt-enter\nprevious_draft = alt-up\nnext_draft = alt-down\nundo = none").unwrap();assert_eq!(map.action_for(&key("ctrl-c")),Some(Action::Interrupt));assert_eq!(map.action_for(&key("ctrl-shift-c")),Some(Action::Copy));assert_eq!(map.action_for(&key("alt-enter")),Some(Action::RunShell));assert_eq!(map.previous_draft,vec![Shortcut::parse("alt-up").unwrap()]);assert_eq!(map.next_draft,vec![Shortcut::parse("alt-down").unwrap()]);assert_eq!(map.action_for(&key("ctrl-z")),None);assert!(Keymap::parse_overrides(Platform::Other,"copy = ctrl-up\nprevious_draft = ctrl-up").is_err());}
    #[test] fn live_terminal_has_only_the_explicit_composer_escape(){
        let map=Keymap::defaults_for(Platform::current()); let mut gpui_map=gpui::Keymap::new(vec![KeyBinding::new("tab",gpui_component::input::MoveDown,Some("Root"))]); gpui_map.add_bindings(map.gpui_bindings());
        let context=[gpui::KeyContext::parse("Root").unwrap(),gpui::KeyContext::parse("Kea").unwrap(),gpui::KeyContext::parse("KeaTerminal").unwrap()];
        let focus=if cfg!(target_os="macos"){"cmd-l"}else{"ctrl-l"};
        assert!(!gpui_map.bindings_for_input(&[key(focus)],&context).0.is_empty());
        for spec in ["ctrl-c","ctrl-v","ctrl-z","ctrl-enter","ctrl-shift-enter","ctrl-shift-space","f6","f7","f8","f9","f10","tab","shift-tab"] { assert!(gpui_map.bindings_for_input(&[key(spec)],&context).0.is_empty(),"captured {spec}"); }
    }
    #[test] fn focus_editor_escape_can_be_remapped_or_disabled(){let remap=Keymap::parse_overrides(Platform::Other,"focus_editor = alt-l").unwrap();assert_eq!(remap.action_for(&key("alt-l")),Some(Action::FocusEditor));let disabled=Keymap::parse_overrides(Platform::Other,"focus_editor = none").unwrap();assert_eq!(disabled.action_for(&key("ctrl-l")),None);}
}
