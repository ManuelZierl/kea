use super::super::InitialFocus;
use super::{next_completion_index, KeaView};
use gpui::{AppContext as _, TestAppContext};
use gpui_component::{input::RopeExt as _, Root};
use kea_app::config::{keybindings::Keymap, settings::Settings};
use kea_app::editor::completion::Candidate;
use kea_document::Document;
use kea_session::Session;

#[gpui::test]
fn completion_selection_wraps_in_both_directions(_: &mut gpui::TestAppContext) {
    assert_eq!(next_completion_index(0, 3, true), 1);
    assert_eq!(next_completion_index(2, 3, true), 0);
    assert_eq!(next_completion_index(0, 3, false), 2);
    assert_eq!(next_completion_index(2, 3, false), 1);
    assert_eq!(next_completion_index(0, 0, true), 0);
}

#[gpui::test]
fn completion_replaces_utf16_range_and_retains_invalidated_worker(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let session = Session::demo().unwrap();
        let document = Document::from_recording(session.recording());
        let view = cx.new(|cx| {
            KeaView::new(
                session,
                document,
                None,
                Keymap::parse("").unwrap(),
                Settings::default(),
                InitialFocus::Editor,
                None,
                window,
                cx,
            )
        });
        view.update(cx, |this, cx| {
            this.editor.update(cx, |state, cx| {
                state.set_value("echo 😀 ca suffix", window, cx);
                let cursor = state.text().offset_to_position(12);
                state.set_cursor_position(cursor, window, cx);
                state.focus(window, cx);
            });
            this.completion_text = "echo 😀 ca suffix".into();
            this.completion_cursor = 12;
            this.completion_index = 0;
            this.candidates = vec![Candidate {
                label: "Path: café".into(),
                replacement: "café".into(),
                range: 10..12,
            }];

            assert!(this.accept_completion(window, cx));
            assert_eq!(this.editor.read(cx).value().as_ref(), "echo 😀 café suffix");
            assert!(this.candidates.is_empty());

            let (_sender, receiver) = std::sync::mpsc::sync_channel(1);
            this.completion_generation = 7;
            this.completion_invalidated = false;
            this.completion_rx = Some(receiver);
            this.dismiss_completion();
            assert!(this.completion_rx.is_some());
            assert!(this.completion_invalidated);
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[test]
fn completion_rows_follow_geometry_not_an_assumed_grid_width() {
    let points = vec![
        Some((10., 5.)),
        Some((90., 5.)),
        Some((15., 25.)),
        Some((95., 25.)),
        Some((30., 45.)),
    ];
    assert_eq!(super::vertical_completion_index(1, &points, true), 3);
    assert_eq!(super::vertical_completion_index(3, &points, false), 1);
    assert_eq!(super::vertical_completion_index(0, &points, true), 2);
}
