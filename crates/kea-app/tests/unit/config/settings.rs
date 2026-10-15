use super::*;

#[test]
fn defaults_are_composer_first_and_overrides_are_explicit() {
    assert_eq!(Settings::default().appearance, Appearance::System);
    assert_eq!(
        Settings::default().post_submit_focus,
        PostSubmitFocus::Editor
    );
    assert!(Settings::default().font_family.is_none());
    assert!(Settings::default().shift_mouse_selects_locally);
    assert!(Settings::default().check_for_updates);
    assert!(!Settings::default().show_blocks);
    assert!(Settings::parse("show_blocks = true").unwrap().show_blocks);
    assert_eq!(Settings::default().submitted_draft, SubmittedDraft::Close);
    assert_eq!(
        Settings::parse("submitted_draft = keep")
            .unwrap()
            .submitted_draft,
        SubmittedDraft::Keep
    );
    assert!(Settings::parse("submitted_draft = remove").is_err());

    let settings = Settings::parse(
        "theme = dark\nfont_size = 16\nsoft_wrap = false\npost_submit_focus = terminal\nshift_mouse_selects_locally = false\nanimate_logo = false\n",
    )
    .unwrap();
    assert_eq!(settings.appearance, Appearance::Dark);
    assert_eq!(settings.font_size, Some(16.));
    assert!(!settings.soft_wrap);
    assert_eq!(settings.post_submit_focus, PostSubmitFocus::Terminal);
    assert!(!settings.shift_mouse_selects_locally);
    assert!(!settings.animate_logo);
}

#[test]
fn update_checks_default_on_and_can_be_disabled() {
    assert!(Settings::default().check_for_updates);
    assert!(
        !Settings::parse("check_for_updates = false")
            .unwrap()
            .check_for_updates
    );
    assert!(Settings::parse("check_for_updates = maybe").is_err());
}

#[test]
fn input_history_persistence_requires_explicit_opt_in() {
    assert!(!Settings::default().history_persistence);
    assert!(
        Settings::parse("history_persistence = true")
            .unwrap()
            .history_persistence
    );
    assert!(Settings::parse("history_persistence = yes").is_err());
}

#[test]
fn rejects_unknown_and_unsafe_sizes() {
    for text in [
        "theme = purple",
        "font_size = NaN",
        "font_size = 100",
        "post_submit_focus = smart",
        "shift_mouse_selects_locally = sometimes",
        "typo = true",
    ] {
        assert!(Settings::parse(text).is_err());
    }
}

#[test]
fn saved_settings_round_trip_and_replace_the_previous_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("nested/settings.conf");
    let settings = Settings {
        appearance: Appearance::Dark,
        font_family: Some("Kea Mono".into()),
        font_size: Some(17.5),
        line_numbers: true,
        soft_wrap: false,
        output_wrap: false,
        syntax_highlighting: false,
        show_blocks: true,
        post_submit_focus: PostSubmitFocus::Terminal,
        submitted_draft: SubmittedDraft::Keep,
        persist_history: true,
        history_persistence: true,
        shift_mouse_selects_locally: false,
        animate_logo: false,
        confirm_ctrl_c: true,
        composer_suggestions: false,
        composer_action_prefix: "!!".into(),
        check_for_updates: false,
    };

    assert_eq!(settings.save_to(&path).unwrap(), None);
    assert_eq!(
        Settings::parse(&fs::read_to_string(&path).unwrap()).unwrap(),
        settings
    );

    let replacement = Settings::default();
    replacement.save_to(&path).unwrap();
    assert_eq!(
        Settings::parse(&fs::read_to_string(&path).unwrap()).unwrap(),
        replacement
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn workflow_settings_round_trip_and_reject_ambiguous_prefixes() {
    assert!(!Settings::default().confirm_ctrl_c);
    assert!(Settings::default().composer_suggestions);
    for prefix in ["::", "!!", "none"] {
        let settings = Settings::parse(&format!(
            "confirm_ctrl_c = true\ncomposer_action_prefix = {prefix}\n"
        ))
        .unwrap();
        assert!(settings.confirm_ctrl_c);
        assert_eq!(Settings::parse(&settings.to_config()).unwrap(), settings);
    }
    assert!(Settings::parse("composer_action_prefix = hello").is_err());
    assert!(Settings::parse("confirm_ctrl_c = smart").is_err());
}

#[test]
fn an_unreadable_settings_file_is_kept_before_the_first_save() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.conf");
    let original = "# mine\ntheme = dark\nfuture_option = yes\n";
    fs::write(&path, original).unwrap();
    let backup = Settings::default().save_to(&path).unwrap().unwrap();
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    assert!(Settings::parse(&fs::read_to_string(&path).unwrap()).is_ok());
    // A second rejected file never overwrites the first backup.
    fs::write(&path, "broken =").unwrap();
    let second = Settings::default().save_to(&path).unwrap().unwrap();
    assert_ne!(second, backup);
    assert_eq!(fs::read_to_string(&backup).unwrap(), original);
    // A valid file needs no backup.
    assert_eq!(Settings::default().save_to(&path).unwrap(), None);
}
