use super::{terminal_point, terminal_visible_extent};
use crate::app::TerminalFontMetrics;
use gpui::{font, point, px, size, Bounds};
use kea_app::terminal::selection::{edge_scroll_lines, MouseOwner};
use kea_session::Session;

fn metrics() -> TerminalFontMetrics {
    TerminalFontMetrics {
        font: font("monospace"),
        font_size: px(10.),
        cell_width: px(10.),
        line_height: px(10.),
    }
}

#[test]
fn frozen_autoscroll_uses_the_clipped_visible_edge_not_the_original_grid() {
    let grid = kea_core::Size::new(20, 6).unwrap();
    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(40.), px(30.)));
    let visible = terminal_visible_extent(bounds, &metrics(), grid);
    assert_eq!(visible, size(px(40.), px(30.)));
    assert_eq!(
        edge_scroll_lines(MouseOwner::LocalSimple, 29., f32::from(visible.height), 10.),
        -1
    );
    assert_eq!(
        edge_scroll_lines(MouseOwner::LocalBlock, 50., f32::from(visible.height), 10.),
        -3
    );
    assert_eq!(
        edge_scroll_lines(MouseOwner::Forward, 29., f32::from(visible.height), 10.),
        0
    );
}

#[test]
fn pointer_extent_preserves_partial_cells_and_ignores_blank_enlarged_space() {
    let grid = kea_core::Size::new(4, 3).unwrap();
    for (bounds_size, expected) in [
        (
            size(px(25.), px(15.)),
            kea_alacritty::TerminalPoint { row: 1, column: 2 },
        ),
        (
            size(px(200.), px(100.)),
            kea_alacritty::TerminalPoint { row: 2, column: 3 },
        ),
    ] {
        let bounds = Bounds::new(point(px(0.), px(0.)), bounds_size);
        assert_eq!(
            terminal_point(point(px(300.), px(150.)), Some(bounds), &metrics(), grid),
            Some(expected)
        );
        assert_eq!(
            terminal_point(point(px(-10.), px(-10.)), Some(bounds), &metrics(), grid),
            Some(kea_alacritty::TerminalPoint { row: 0, column: 0 })
        );
    }
    let empty = Bounds::new(point(px(0.), px(0.)), size(px(0.), px(0.)));
    assert_eq!(
        terminal_point(point(px(0.), px(0.)), Some(empty), &metrics(), grid),
        None
    );
}

#[test]
fn clipped_frozen_drag_cannot_select_hidden_rows_or_columns() {
    let original = kea_core::Size::new(20, 6).unwrap();
    let mut session = Session::from_recording(kea_core::Recording::new(original).unwrap()).unwrap();
    session.freeze_display();
    session.resize(kea_core::Size::new(4, 3).unwrap()).unwrap();
    assert_eq!(
        session.terminal_size(),
        original,
        "frozen content must not reflow"
    );
    let bounds = Bounds::new(point(px(5.), px(7.)), size(px(40.), px(30.)));
    assert_eq!(
        terminal_point(
            point(px(200.), px(100.)),
            Some(bounds),
            &metrics(),
            session.terminal_size()
        ),
        Some(kea_alacritty::TerminalPoint { row: 2, column: 3 }),
        "an outside drag must stop at the last painted cell, not the hidden snapshot edge"
    );
}
