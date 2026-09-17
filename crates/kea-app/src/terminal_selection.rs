//! Pure terminal text-routing decisions. Session state and protocol encoding stay
//! in their owning layers; this module owns only app routing state.

use kea_alacritty::{SelectionMotion, TerminalPoint};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseOwner {
    LocalSimple,
    LocalBlock,
    Forward,
}

pub fn mouse_owner(
    reporting: bool,
    shift_local: bool,
    explicit: bool,
    shift: bool,
    alt: bool,
) -> MouseOwner {
    if explicit || !reporting || (shift_local && shift) {
        if alt && (explicit || shift || !reporting) {
            MouseOwner::LocalBlock
        } else {
            MouseOwner::LocalSimple
        }
    } else {
        MouseOwner::Forward
    }
}

pub fn hover_is_local(reporting: bool, shift_local: bool, explicit: bool, shift: bool) -> bool {
    mouse_owner(reporting, shift_local, explicit, shift, false) != MouseOwner::Forward
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Gesture {
    pub start: TerminalPoint,
    pub owner: MouseOwner,
    pub explicit: bool,
    pub shift_extend: bool,
    pub potential_block: bool,
    pub moved: bool,
    pub preserve_click: bool,
}

impl Gesture {
    pub fn new(
        start: TerminalPoint,
        owner: MouseOwner,
        explicit: bool,
        shift: bool,
        had_selection: bool,
    ) -> Self {
        Self {
            start,
            owner,
            explicit,
            shift_extend: shift && (explicit || had_selection),
            potential_block: owner == MouseOwner::LocalBlock,
            moved: false,
            preserve_click: shift && had_selection,
        }
    }

    pub fn moved_to(&mut self, point: TerminalPoint) -> bool {
        if point == self.start && !self.moved {
            return false;
        }
        self.moved = true;
        true
    }

    pub fn block_on_first_move(&self) -> bool {
        self.potential_block && self.moved
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalKey {
    Copy,
    Escape,
    Move {
        motion: SelectionMotion,
        extend: bool,
    },
    Forward,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub platform: bool,
    pub function: bool,
}

pub fn local_key(
    key: &str,
    shift: bool,
    control: bool,
    alt: bool,
    platform: bool,
    function: bool,
    active: bool,
) -> LocalKey {
    local_key_for_platform(
        key,
        KeyModifiers {
            shift,
            control,
            alt,
            platform,
            function,
        },
        cfg!(target_os = "macos"),
        active,
    )
}

pub fn local_key_for_platform(
    key: &str,
    modifiers: KeyModifiers,
    mac: bool,
    active: bool,
) -> LocalKey {
    if !active || modifiers.function {
        return LocalKey::Forward;
    }
    let copy = if mac {
        modifiers.platform && !modifiers.shift && !modifiers.control && !modifiers.alt
    } else {
        modifiers.control && !modifiers.shift && !modifiers.platform && !modifiers.alt
    };
    if key == "c" && copy {
        return LocalKey::Copy;
    }
    if key == "escape"
        && !modifiers.shift
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.platform
    {
        return LocalKey::Escape;
    }
    if modifiers.control || modifiers.alt || modifiers.platform {
        return LocalKey::Forward;
    }
    let motion = match key {
        "left" => SelectionMotion::Left,
        "right" => SelectionMotion::Right,
        "up" => SelectionMotion::Up,
        "down" => SelectionMotion::Down,
        "home" => SelectionMotion::Home,
        "end" => SelectionMotion::End,
        "pageup" | "page-up" => SelectionMotion::PageUp,
        "pagedown" | "page-down" => SelectionMotion::PageDown,
        _ => return LocalKey::Forward,
    };
    LocalKey::Move {
        motion,
        extend: modifiers.shift,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(column: usize) -> TerminalPoint {
        TerminalPoint { row: 0, column }
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
        let plain = crate::terminal_mouse::encode_pointer(
            kea_alacritty::MouseEncoding::Sgr,
            crate::terminal_mouse::PointerEvent::Motion,
            Some(crate::terminal_mouse::PointerButton::Left),
            point,
            false,
            false,
            false,
        )
        .unwrap();
        let shifted = crate::terminal_mouse::encode_pointer(
            kea_alacritty::MouseEncoding::Sgr,
            crate::terminal_mouse::PointerEvent::Motion,
            Some(crate::terminal_mouse::PointerButton::Left),
            point,
            true,
            false,
            false,
        )
        .unwrap();
        assert_ne!(plain, shifted);
    }
}
