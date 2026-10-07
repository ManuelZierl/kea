use super::*;
use kea_core::{Kind, Recording, Size};
use kea_session::Session;

fn point(column: usize) -> TerminalPoint {
    TerminalPoint { row: 0, column }
}

#[test]
fn edge_autoscroll_is_local_directional_and_bounded() {
    for owner in [MouseOwner::LocalSimple, MouseOwner::LocalBlock] {
        assert_eq!(edge_scroll_lines(owner, 0., 100., 10.), 1);
        assert_eq!(edge_scroll_lines(owner, 9., 100., 10.), 1);
        assert_eq!(edge_scroll_lines(owner, 10., 100., 10.), 0);
        assert_eq!(edge_scroll_lines(owner, 89., 100., 10.), 0);
        assert_eq!(edge_scroll_lines(owner, 90., 100., 10.), -1);
        assert_eq!(edge_scroll_lines(owner, -20., 100., 10.), 3);
        assert_eq!(edge_scroll_lines(owner, 120., 100., 10.), -3);
        assert_eq!(edge_scroll_lines(owner, -1000., 100., 10.), 8);
        assert_eq!(edge_scroll_lines(owner, 1000., 100., 10.), -8);
    }
    for y in [-1000., 0., 50., 100., 1000.] {
        assert_eq!(edge_scroll_lines(MouseOwner::Forward, y, 100., 10.), 0);
    }
    for (y, height, line_height) in [
        (0., 10., 10.),
        (0., 100., 0.),
        (f32::NAN, 100., 10.),
        (0., f32::INFINITY, 10.),
        (0., 100., f32::NAN),
    ] {
        assert_eq!(
            edge_scroll_lines(MouseOwner::LocalSimple, y, height, line_height),
            0
        );
    }
}

#[test]
fn local_shift_drag_freezes_without_overriding_forwarded_shift() {
    for owner in [MouseOwner::LocalSimple, MouseOwner::LocalBlock] {
        assert!(local_shift_drag_freezes(true, owner));
        assert!(!local_shift_drag_freezes(false, owner));
    }
    assert!(!local_shift_drag_freezes(true, MouseOwner::Forward));
}

#[test]
fn ownership_and_drag_phase_are_latched_at_press() {
    assert_eq!(
        mouse_owner(false, false, false, false, true),
        MouseOwner::LocalBlock
    );
    let mut gesture = Gesture::new(point(1), MouseOwner::LocalBlock, false, false, false);
    assert!(!gesture.block_on_first_move());
    assert!(gesture.moved_to(point(2)));
    assert!(gesture.block_on_first_move());
    assert_eq!(gesture.owner, MouseOwner::LocalBlock);
    assert!(
        gesture.moved_to(point(1)),
        "returning to the anchor must update the range"
    );
}

#[test]
fn reporting_override_and_explicit_entry_have_independent_ownership() {
    for alt in [false, true] {
        let local = if alt {
            MouseOwner::LocalBlock
        } else {
            MouseOwner::LocalSimple
        };
        assert_eq!(
            mouse_owner(true, true, false, false, alt),
            MouseOwner::Forward
        );
        assert_eq!(mouse_owner(true, true, false, true, alt), local);
        assert_eq!(
            mouse_owner(true, false, false, true, alt),
            MouseOwner::Forward
        );
        assert_eq!(mouse_owner(true, false, true, false, alt), local);
        assert_eq!(mouse_owner(false, false, false, false, alt), local);
    }
}

#[test]
fn local_pointer_policy_suppresses_reserved_hover() {
    assert!(hover_is_local(true, true, false, true));
    assert!(!hover_is_local(true, true, false, false));
    assert!(!hover_is_local(true, false, false, true));
    assert!(hover_is_local(true, false, true, false));
}

#[test]
fn explicit_and_shift_clicks_preserve_their_anchor() {
    let explicit = Gesture::new(point(1), MouseOwner::LocalSimple, true, false, false);
    assert!(!explicit.shift_extend);
    let extended = Gesture::new(point(1), MouseOwner::LocalSimple, true, true, false);
    assert!(extended.shift_extend);
    let implicit = Gesture::new(point(1), MouseOwner::LocalSimple, false, true, true);
    assert!(implicit.preserve_click);
}

#[test]
fn ordinary_shift_extension_survives_freeze_but_ctrl_shift_starts_over() {
    assert!(preserves_shift_selection(true, false, true, false));
    assert!(preserves_shift_selection(true, false, false, true));
    assert!(!preserves_shift_selection(true, true, true, true));
    assert!(!preserves_shift_selection(false, false, true, true));
}

#[test]
fn shift_click_keeps_existing_range_through_mouse_down_and_up() {
    let size = Size::new(12, 2).unwrap();
    let mut recording = Recording::new(size).unwrap();
    recording
        .append(0, Kind::Output(b"initial".to_vec()))
        .unwrap();
    let mut session = Session::from_recording(recording).unwrap();
    session.begin_terminal_selection(TerminalPoint { row: 0, column: 0 });
    session.update_terminal_selection(TerminalPoint { row: 0, column: 2 });

    let gesture = Gesture::new(
        point(5),
        MouseOwner::LocalSimple,
        false,
        true,
        session.terminal_has_selection(),
    );
    assert!(gesture.shift_extend);
    assert!(gesture.preserve_click);
    session.freeze_display();
    session.extend_terminal_selection(point(5));
    assert_eq!(session.terminal_selection_text().as_deref(), Some("initia"));
    assert!(!session.is_history());
}

#[test]
fn shift_drag_keeps_explicit_caret_anchor_without_child_or_history_effects() {
    let size = Size::new(12, 2).unwrap();
    let mut recording = Recording::new(size).unwrap();
    recording
        .append(0, Kind::Output(b"initial".to_vec()))
        .unwrap();
    let mut session = Session::from_recording(recording).unwrap();
    session.place_terminal_selection_caret(point(0));
    let gesture = Gesture::new(
        point(4),
        MouseOwner::LocalSimple,
        session.terminal_explicit_selection_active(),
        true,
        session.terminal_has_selection(),
    );
    assert!(gesture.shift_extend);
    session.freeze_display();
    session.extend_terminal_selection(point(4));
    assert_eq!(session.terminal_selection_text().as_deref(), Some("initi"));
    assert!(!session.is_history());
}

#[test]
fn copy_requires_the_exact_platform_chord() {
    assert_eq!(
        local_key_for_platform(
            "c",
            KeyModifiers {
                control: true,
                ..Default::default()
            },
            false,
            true,
        ),
        LocalKey::Copy
    );
    assert_eq!(
        local_key_for_platform(
            "c",
            KeyModifiers {
                platform: true,
                ..Default::default()
            },
            true,
            true,
        ),
        LocalKey::Copy
    );
    assert_eq!(
        local_key_for_platform(
            "c",
            KeyModifiers {
                platform: true,
                shift: true,
                ..Default::default()
            },
            true,
            true,
        ),
        LocalKey::Forward
    );
    assert_eq!(
        local_key_for_platform(
            "c",
            KeyModifiers {
                platform: true,
                control: true,
                ..Default::default()
            },
            true,
            true,
        ),
        LocalKey::Forward
    );
    assert_eq!(
        local_key_for_platform(
            "c",
            KeyModifiers {
                platform: true,
                function: true,
                ..Default::default()
            },
            true,
            true,
        ),
        LocalKey::Forward
    );
    assert_eq!(
        local_key_for_platform(
            "c",
            KeyModifiers {
                platform: true,
                ..Default::default()
            },
            false,
            false,
        ),
        LocalKey::Forward
    );
}

#[test]
fn local_keyboard_policy_is_selection_only_and_fail_open() {
    assert_eq!(
        local_key("escape", false, false, false, false, false, true),
        LocalKey::Escape
    );
    assert_eq!(
        local_key("right", true, false, false, false, false, true),
        LocalKey::Move {
            motion: SelectionMotion::Right,
            extend: true
        }
    );
    assert_eq!(
        local_key("enter", false, false, false, false, false, true),
        LocalKey::Forward
    );
    assert_eq!(
        local_key("x", false, true, false, false, false, true),
        LocalKey::Forward
    );
}

#[test]
fn forwarded_motion_can_change_modifiers_after_press() {
    let mut gesture = Gesture::new(point(1), MouseOwner::Forward, false, false, false);
    assert!(!gesture.moved_to(point(1)));
    assert_eq!(gesture.owner, MouseOwner::Forward);
    let point = point(2);
    let plain = crate::terminal::mouse::encode_pointer(
        kea_alacritty::MouseEncoding::Sgr,
        crate::terminal::mouse::PointerEvent::Motion,
        Some(crate::terminal::mouse::PointerButton::Left),
        point,
        false,
        false,
        false,
    )
    .unwrap();
    let shifted = crate::terminal::mouse::encode_pointer(
        kea_alacritty::MouseEncoding::Sgr,
        crate::terminal::mouse::PointerEvent::Motion,
        Some(crate::terminal::mouse::PointerButton::Left),
        point,
        true,
        false,
        false,
    )
    .unwrap();
    assert_ne!(plain, shifted);
}
