use super::*;

fn key(spec: &str) -> Keystroke {
    Keystroke::parse(spec).unwrap()
}

#[test]
fn explicit_default_focus_terminal_does_not_change_effective_keymap() {
    for platform in [Platform::Other, Platform::Mac] {
        let defaults = Keymap::defaults_for(platform);
        let value = defaults.focus_terminal[0].specification();
        let explicit =
            Keymap::parse_overrides(platform, &format!("focus_terminal = {value}")).unwrap();
        assert_eq!(defaults, explicit);
    }
}

#[test]
fn one_way_terminal_focus_binding_is_composer_only_without_focus_history() {
    let keymap = Keymap::parse("focus_terminal = alt-t").unwrap();
    let map = gpui::Keymap::new(keymap.gpui_bindings());
    for (names, expected) in [
        (vec!["Kea", "KeaCommand", "Input"], true),
        (vec!["Kea", "KeaTerminal"], false),
        (vec!["Dialog", "Input"], false),
    ] {
        let context = names
            .iter()
            .map(|s| gpui::KeyContext::parse(s).unwrap())
            .collect::<Vec<_>>();
        let bindings = map.bindings_for_input(&[key("alt-t")], &context).0;
        assert_eq!(
            bindings.iter().any(|binding| binding
                .action()
                .as_any()
                .downcast_ref::<Invoke>()
                .is_some_and(|invoke| invoke.action == Action::FocusEditor)),
            expected,
            "{names:?}"
        );
    }
}

#[test]
fn settings_snapshot_round_trips_all_bindings_and_unbound_actions() {
    for platform in [Platform::Other, Platform::Mac] {
        let original = Keymap::parse_overrides(platform,
            "focus_editor = alt-l\nfocus_terminal = alt-l\nrun_shell = alt-enter, alt-f12\nundo = none\nprevious_draft = alt-up\nnext_draft = alt-down").unwrap();
        assert_eq!(original.settings_entries().len(), 27);
        assert_eq!(
            Keymap::parse_overrides(platform, &original.to_config()).unwrap(),
            original
        );
    }
}

#[test]
fn saved_keybindings_replace_atomically_and_failure_preserves_destination() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("nested/keybindings.conf");
    let original = Keymap::parse("focus_editor = alt-l").unwrap();
    original.save_to(&path).unwrap();
    assert_eq!(
        Keymap::parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        original
    );
    let changed = Keymap::parse("focus_editor = alt-f12\nundo = none").unwrap();
    changed.save_to(&path).unwrap();
    assert_eq!(
        Keymap::parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        changed
    );
    let destination = root.path().join("blocked");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("keep"), b"original").unwrap();
    assert!(changed.save_to(&destination).is_err());
    assert_eq!(
        std::fs::read(destination.join("keep")).unwrap(),
        b"original"
    );
}

#[test]
fn hunt_custom_actions_are_masked_only_in_live_terminal() {
    let keymap = Keymap::parse_overrides(
        Platform::Other,
        "run_shell = alt-enter\ncopy = alt-c\ntoggle_blocks = alt-space\nfocus_editor = alt-l",
    )
    .unwrap();
    let map = gpui::Keymap::new(keymap.gpui_bindings());
    let terminal = ["Kea", "KeaTerminal"].map(|s| gpui::KeyContext::parse(s).unwrap());
    let composer = ["Kea", "KeaCommand", "Input"].map(|s| gpui::KeyContext::parse(s).unwrap());
    for spec in ["alt-enter", "alt-c", "alt-space"] {
        assert!(
            map.bindings_for_input(&[key(spec)], &terminal).0.is_empty(),
            "captured {spec}"
        );
        assert!(
            !map.bindings_for_input(&[key(spec)], &composer).0.is_empty(),
            "lost {spec}"
        );
    }
    assert!(!map
        .bindings_for_input(&[key("alt-l")], &terminal)
        .0
        .is_empty());
}

#[test]
fn hunt_focus_escape_can_reuse_a_remapped_default_shortcut() {
    for platform in [Platform::Other, Platform::Mac] {
        let keymap =
            Keymap::parse_overrides(platform, "focus_editor = ctrl-r\nreverse_search = alt-r")
                .unwrap();
        assert_eq!(keymap.action_for(&key("ctrl-r")), Some(Action::FocusEditor));
        let map = gpui::Keymap::new(keymap.gpui_bindings());
        for names in [
            vec!["Kea", "KeaCommand", "Input"],
            vec!["Kea", "KeaTerminal"],
        ] {
            let context = names
                .iter()
                .map(|s| gpui::KeyContext::parse(s).unwrap())
                .collect::<Vec<_>>();
            let bindings = map.bindings_for_input(&[key("ctrl-r")], &context).0;
            assert!(
                bindings.iter().any(|b| b
                    .action()
                    .as_any()
                    .downcast_ref::<Invoke>()
                    .is_some_and(|a| a.action == Action::FocusEditor)),
                "accepted focus escape is unavailable in {names:?}"
            );
        }
    }
}

#[test]
fn default_config_files_are_created_but_never_overwritten() {
    let root = std::env::temp_dir().join(format!("kea-config-test-{}", std::process::id()));
    let path = root.join("nested").join("kea").join("keybindings.conf");
    let _ = std::fs::remove_dir_all(&root);
    assert!(ensure_default_file(&path, DEFAULT_KEYBINDINGS_CONF).unwrap());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        DEFAULT_KEYBINDINGS_CONF
    );
    assert!(!ensure_default_file(&path, "focus_editor = none\n").unwrap());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        DEFAULT_KEYBINDINGS_CONF
    );
    let map = Keymap::parse_overrides(Platform::Other, DEFAULT_KEYBINDINGS_CONF).unwrap();
    assert_eq!(map.action_for(&key("ctrl-l")), Some(Action::FocusEditor));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn defaults_are_editor_native_and_submission_actions_are_distinct() {
    let map = Keymap::defaults_for(Platform::Other);
    assert_eq!(map.action_for(&key("ctrl-enter")), Some(Action::RunShell));
    assert_eq!(
        map.action_for(&key("ctrl-shift-enter")),
        Some(Action::SendApplication)
    );
    assert_eq!(map.action_for(&key("tab")), Some(Action::Complete));
    assert_eq!(
        map.previous_draft,
        vec![Shortcut::parse("ctrl-up").unwrap()]
    );
    assert_eq!(map.next_draft, vec![Shortcut::parse("ctrl-down").unwrap()]);
    assert_eq!(
        map.focus_terminal,
        vec![Shortcut::parse("ctrl-shift-l").unwrap()]
    );
    assert_eq!(map.action_for(&key("enter")), None);
    assert_eq!(map.action_for(&key("shift-enter")), None);
}

#[test]
fn terminal_like_enter_policy_is_configurable() {
    let map = Keymap::parse_overrides(Platform::Other, "run_shell = enter\nnewline = shift-enter")
        .unwrap();
    assert_eq!(map.action_for(&key("enter")), Some(Action::RunShell));
    assert_eq!(map.action_for(&key("shift-enter")), Some(Action::Newline));
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
}

#[test]
fn configuration_remaps_unbinds_and_rejects_ambiguity() {
    let map = Keymap::parse_overrides(
        Platform::Other,
        "copy = ctrl-shift-c\ninterrupt = ctrl-c\nrun_shell = alt-enter\nprevious_draft = alt-up\nnext_draft = alt-down\nfocus_terminal = alt-l\nundo = none",
    )
    .unwrap();
    assert_eq!(map.action_for(&key("ctrl-c")), Some(Action::Interrupt));
    assert_eq!(map.action_for(&key("ctrl-shift-c")), Some(Action::Copy));
    assert_eq!(map.action_for(&key("alt-enter")), Some(Action::RunShell));
    assert_eq!(map.previous_draft, vec![Shortcut::parse("alt-up").unwrap()]);
    assert_eq!(map.next_draft, vec![Shortcut::parse("alt-down").unwrap()]);
    assert_eq!(map.focus_terminal, vec![Shortcut::parse("alt-l").unwrap()]);
    assert_eq!(map.action_for(&key("ctrl-z")), None);
    assert!(
        Keymap::parse_overrides(Platform::Other, "copy = ctrl-up\nprevious_draft = ctrl-up")
            .is_err()
    );
    let toggle = Keymap::parse_overrides(Platform::Other, "focus_terminal = ctrl-l").unwrap();
    assert_eq!(
        toggle.focus_terminal,
        vec![Shortcut::parse("ctrl-l").unwrap()]
    );
    assert!(Keymap::parse_overrides(Platform::Other, "focus_terminal = ctrl-up").is_err());
}

#[test]
fn live_terminal_has_only_the_explicit_composer_escape() {
    let map = Keymap::defaults_for(Platform::current());
    let mut gpui_map = gpui::Keymap::new(vec![KeyBinding::new(
        "tab",
        gpui_component::input::MoveDown,
        Some("Root"),
    )]);
    gpui_map.add_bindings(map.gpui_bindings());
    let context = [
        gpui::KeyContext::parse("Root").unwrap(),
        gpui::KeyContext::parse("Kea").unwrap(),
        gpui::KeyContext::parse("KeaTerminal").unwrap(),
    ];
    let focus = if cfg!(target_os = "macos") {
        "cmd-l"
    } else {
        "ctrl-l"
    };
    assert!(!gpui_map
        .bindings_for_input(&[key(focus)], &context)
        .0
        .is_empty());
    for spec in [
        "ctrl-c",
        "ctrl-v",
        "ctrl-shift-v",
        "ctrl-z",
        "ctrl-enter",
        "ctrl-shift-enter",
        "ctrl-shift-l",
        "ctrl-r",
        "ctrl-shift-space",
        "f4",
        "f6",
        "f7",
        "f8",
        "f9",
        "f10",
        "tab",
        "shift-tab",
    ] {
        assert!(
            gpui_map
                .bindings_for_input(&[key(spec)], &context)
                .0
                .is_empty(),
            "captured {spec}"
        );
    }
}

#[test]
fn focus_editor_escape_can_be_remapped_or_disabled() {
    let remap = Keymap::parse_overrides(
        Platform::Other,
        "focus_editor = alt-l\nfocus_terminal = alt-shift-l",
    )
    .unwrap();
    assert_eq!(remap.action_for(&key("alt-l")), Some(Action::FocusEditor));
    assert_eq!(
        remap.focus_terminal,
        vec![Shortcut::parse("alt-shift-l").unwrap()]
    );
    let disabled = Keymap::parse_overrides(
        Platform::Other,
        "focus_editor = none\nfocus_terminal = none",
    )
    .unwrap();
    assert_eq!(disabled.action_for(&key("ctrl-l")), None);
    assert!(disabled.focus_terminal.is_empty());
}

#[test]
fn selection_entry_is_bound_outside_the_live_terminal() {
    let mut map = gpui::Keymap::new(vec![]);
    map.add_bindings(Keymap::defaults_for(Platform::current()).gpui_bindings());
    for context in [vec!["Kea", "Input"], vec!["Kea", "KeaChrome"]] {
        let context = context
            .into_iter()
            .map(|context| gpui::KeyContext::parse(context).unwrap())
            .collect::<Vec<_>>();
        let bindings = map.bindings_for_input(&[key("f4")], &context).0;
        assert!(bindings.iter().any(|binding| binding
            .action()
            .as_any()
            .downcast_ref::<Invoke>()
            .is_some_and(|action| action.action == Action::SelectTerminalText)));
    }
}

#[test]
fn reverse_search_is_configurable_and_never_captures_the_live_terminal() {
    for platform in [Platform::Other, Platform::Mac] {
        assert_eq!(
            Keymap::defaults_for(platform).action_for(&key("ctrl-r")),
            Some(Action::ReverseSearch)
        );
        let remapped = Keymap::parse_overrides(platform, "reverse_search = alt-r").unwrap();
        assert_eq!(remapped.action_for(&key("ctrl-r")), None);
        assert_eq!(
            remapped.action_for(&key("alt-r")),
            Some(Action::ReverseSearch)
        );
        let map = gpui::Keymap::new(remapped.gpui_bindings());
        let terminal = [
            gpui::KeyContext::parse("Kea").unwrap(),
            gpui::KeyContext::parse("KeaTerminal").unwrap(),
        ];
        for shortcut in ["ctrl-r", "alt-r"] {
            assert!(map
                .bindings_for_input(&[key(shortcut)], &terminal)
                .0
                .is_empty());
        }
        let compose = [
            gpui::KeyContext::parse("Kea").unwrap(),
            gpui::KeyContext::parse("KeaCommand").unwrap(),
            gpui::KeyContext::parse("Input").unwrap(),
        ];
        assert!(!map
            .bindings_for_input(&[key("alt-r")], &compose)
            .0
            .is_empty());
        let unbound = Keymap::parse_overrides(platform, "reverse_search = none").unwrap();
        assert_eq!(unbound.action_for(&key("ctrl-r")), None);
    }
}

#[test]
fn focus_editor_switch_binds_in_terminal_and_composer() {
    // The single switch key must dispatch in the live terminal
    // (`KeaTerminal`) and in the composer (`KeaCommand > Input` via the
    // generic `Kea > Input` descendant predicate).
    let mut map = gpui::Keymap::new(vec![]);
    map.add_bindings(Keymap::defaults_for(Platform::current()).gpui_bindings());
    let focus = if cfg!(target_os = "macos") {
        "cmd-l"
    } else {
        "ctrl-l"
    };
    for context in [
        vec!["Kea", "KeaTerminal"],
        vec!["Kea", "Input"],
        vec!["Kea", "KeaCommand", "Input"],
    ] {
        let context = context
            .into_iter()
            .map(|context| gpui::KeyContext::parse(context).unwrap())
            .collect::<Vec<_>>();
        let bindings = map.bindings_for_input(&[key(focus)], &context).0;
        assert!(
            bindings.iter().any(|binding| binding
                .action()
                .as_any()
                .downcast_ref::<Invoke>()
                .is_some_and(|action| action.action == Action::FocusEditor)),
            "focus switch missing in {context:?}"
        );
    }
}
