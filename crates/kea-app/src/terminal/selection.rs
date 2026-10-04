//! Pure terminal text-routing decisions. Session state and protocol encoding stay
//! in their owning layers; this module owns only app routing state.

use kea_alacritty::{SelectionMotion, TerminalPoint};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseOwner {
    LocalSimple,
    LocalBlock,
    Forward,
}

pub fn frozen_drag(control: bool, shift: bool) -> bool {
    control && shift
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

/// Lines per 50 ms tick for a local drag in the first/last visual row.
/// Distance beyond the edge accelerates scrolling, but remains bounded. Child
/// gestures never acquire local autoscroll, even if modifiers change mid-drag.
pub fn edge_scroll_lines(owner: MouseOwner, y: f32, height: f32, line_height: f32) -> i32 {
    if owner == MouseOwner::Forward
        || !y.is_finite()
        || !height.is_finite()
        || !line_height.is_finite()
        || line_height <= 0.
        || height < line_height * 2.
    {
        return 0;
    }
    if y < line_height {
        (1. + (-y / line_height).max(0.)).clamp(1., 8.) as i32
    } else if y >= height - line_height {
        -((1. + ((y - height) / line_height).max(0.)).clamp(1., 8.) as i32)
    } else {
        0
    }
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
#[path = "../../tests/unit/terminal/selection.rs"]
mod tests;
