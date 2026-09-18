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
        persist_history: true,
        history_persistence: true,
        shift_mouse_selects_locally: false,
        animate_logo: false,
    };

    settings.save_to(&path).unwrap();
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
