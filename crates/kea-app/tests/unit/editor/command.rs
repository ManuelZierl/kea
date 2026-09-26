use super::*;
use gpui::TestAppContext;
use gpui_component::Root;

struct EmptyView;

impl gpui::Render for EmptyView {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div()
    }
}

#[gpui::test]
fn component_composition_is_not_a_submittable_command(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let editor = new_draft(
                Some(ShellFlavor::Posix),
                &Settings::default(),
                "",
                window,
                cx,
            );
            editor.update(cx, |state, cx| {
                state.focus(window, cx);
                state.replace_and_mark_text_in_range(None, "日本", None, window, cx);
                assert!(state.marked_text_range(window, cx).is_some());
            });
            assert!(submission_text(&editor, window, cx).is_none());
            editor.update(cx, |state, cx| {
                state.replace_text_in_range(None, "日本語", window, cx)
            });
            assert_eq!(
                submission_text(&editor, window, cx).as_deref(),
                Some("日本語")
            );
        })
        .unwrap();
}

#[gpui::test]
fn component_replaces_utf16_ranges_without_a_keycode_approximation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let editor = new_draft(None, &Settings::default(), "a😀ö", window, cx);
            editor.update(cx, |state, cx| {
                state.replace_text_in_range(Some(1..3), "ä", window, cx);
                assert_eq!(state.value().as_ref(), "aäö");
                let mut adjusted = None;
                assert_eq!(
                    state
                        .text_for_range(1..2, &mut adjusted, window, cx)
                        .as_deref(),
                    Some("ä")
                );
            });
            let fresh = new_draft(None, &Settings::default(), "", window, cx);
            assert!(fresh.read(cx).value().is_empty());
            assert_ne!(fresh.entity_id(), editor.entity_id());
            let plain = Settings {
                syntax_highlighting: false,
                line_numbers: true,
                ..Settings::default()
            };
            let editor = new_draft(
                Some(ShellFlavor::Posix),
                &plain,
                "not highlighted",
                window,
                cx,
            );
            assert_eq!(editor.read(cx).value().as_ref(), "not highlighted");
        })
        .unwrap();
}

#[gpui::test]
fn successful_fresh_draft_commits_exact_submission_for_recall(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let editor = new_draft(
                None,
                &Settings::default(),
                "prompt\nwith details",
                window,
                cx,
            );
            editor.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(
                submission_text(&editor, window, cx).as_deref(),
                Some("prompt\nwith details")
            );
            let fresh = new_draft(None, &Settings::default(), "", window, cx);
            fresh.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(submitted_history_len(cx), 1);
            assert!(navigate_submitted_drafts(
                HistoryDirection::Previous,
                window,
                cx
            ));
            assert_eq!(fresh.read(cx).value().as_ref(), "prompt\nwith details");
        })
        .unwrap();
}

#[gpui::test]
fn recall_navigates_all_submissions_not_just_the_last(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            // Mimic two successful Runs: reading the focused draft stages the
            // submission candidate, and the empty fresh draft commits it.
            for submitted in ["echo one", "echo two"] {
                let editor = new_draft(None, &Settings::default(), submitted, window, cx);
                editor.update(cx, |state, cx| state.focus(window, cx));
                assert_eq!(
                    submission_text(&editor, window, cx).as_deref(),
                    Some(submitted)
                );
                let fresh = new_draft(None, &Settings::default(), "", window, cx);
                fresh.update(cx, |state, cx| state.focus(window, cx));
                let _ = editor;
            }
            assert_eq!(submitted_history_len(cx), 2);
            let composer = cx
                .default_global::<DraftRecallGlobal>()
                .current_editor
                .clone()
                .unwrap();
            assert!(navigate_submitted_drafts(
                HistoryDirection::Previous,
                window,
                cx
            ));
            assert_eq!(composer.read(cx).value().as_ref(), "echo two");
            assert!(navigate_submitted_drafts(
                HistoryDirection::Previous,
                window,
                cx
            ));
            assert_eq!(composer.read(cx).value().as_ref(), "echo one");
            assert!(navigate_submitted_drafts(
                HistoryDirection::Next,
                window,
                cx
            ));
            assert_eq!(composer.read(cx).value().as_ref(), "echo two");
            assert!(navigate_submitted_drafts(
                HistoryDirection::Next,
                window,
                cx
            ));
            assert_eq!(composer.read(cx).value().as_ref(), "");
        })
        .unwrap();
}

#[gpui::test]
fn failed_send_or_edit_as_new_is_not_misclassified_as_submission(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let editor = new_draft(None, &Settings::default(), "not sent", window, cx);
            editor.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(
                submission_text(&editor, window, cx).as_deref(),
                Some("not sent")
            );
            assert_eq!(submitted_history_len(cx), 0);

            let edited = new_draft(None, &Settings::default(), "historical block", window, cx);
            edited.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(submitted_history_len(cx), 0);
            assert!(!navigate_submitted_drafts(
                HistoryDirection::Previous,
                window,
                cx
            ));
        })
        .unwrap();
}

#[gpui::test]
fn terminal_tabs_preserve_independent_recall_and_scratch(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let a = new_draft(None, &Settings::default(), "submitted A", window, cx);
            a.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(
                submission_text(&a, window, cx).as_deref(),
                Some("submitted A")
            );
            let a = new_draft(None, &Settings::default(), "", window, cx);
            a.update(cx, |state, cx| {
                state.focus(window, cx);
                state.replace_text_in_range(None, "scratch A", window, cx);
            });
            assert!(navigate_submitted_drafts(
                HistoryDirection::Previous,
                window,
                cx
            ));
            activate_tab_history(1, cx);
            let b = new_draft(None, &Settings::default(), "draft B", window, cx);
            b.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(submitted_history_len(cx), 0);
            assert!(!navigate_submitted_drafts(
                HistoryDirection::Previous,
                window,
                cx
            ));
            // Reading a candidate is not a successful send; switching discards it.
            assert_eq!(submission_text(&b, window, cx).as_deref(), Some("draft B"));
            activate_tab_history(2, cx);
            let c = new_draft(None, &Settings::default(), "", window, cx);
            assert_eq!(submitted_history_len(cx), 0);
            assert!(c.read(cx).value().is_empty());
            activate_tab_history(0, cx);
            a.update(cx, |state, cx| state.focus(window, cx));
            assert_eq!(submitted_history_len(cx), 1);
            assert_eq!(a.read(cx).value().as_ref(), "submitted A");
            assert!(navigate_submitted_drafts(
                HistoryDirection::Next,
                window,
                cx
            ));
            assert_eq!(a.read(cx).value().as_ref(), "scratch A");
            activate_tab_history(1, cx);
            assert_eq!(submitted_history_len(cx), 0);
            assert_eq!(b.read(cx).value().as_ref(), "draft B");
            forget_tab_history(0, cx);
            assert!(!cx
                .default_global::<DraftRecallGlobal>()
                .inactive
                .contains_key(&0));
        })
        .unwrap();
}

#[gpui::test]
fn interleaved_tabs_share_one_persistence_writer(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        register_languages();
    });
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("drafts.txt");
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| EmptyView);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            set_history_persistence(path.clone(), vec!["old".into()], cx);
            for (id, text) in [(0, "A"), (1, "B"), (0, "C")] {
                activate_tab_history(id, cx);
                let draft = new_draft(None, &Settings::default(), text, window, cx);
                draft.update(cx, |state, cx| state.focus(window, cx));
                assert_eq!(submission_text(&draft, window, cx).as_deref(), Some(text));
                new_draft(None, &Settings::default(), "", window, cx);
            }
            assert_eq!(
                crate::editor::history::load_history_file(&path).unwrap(),
                vec!["old", "A", "B", "C"]
            );
            assert_eq!(submitted_history_len(cx), 3); // old, A, C; B belongs to tab 1
            activate_tab_history(1, cx);
            assert_eq!(submitted_history_len(cx), 2); // old, B
        })
        .unwrap();
}

#[gpui::test]
fn closed_recall_contexts_do_not_accumulate(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for id in 0..1000 {
            activate_tab_history(id, cx);
            forget_tab_history(id, cx);
        }
        activate_tab_history(1000, cx);
        assert!(cx.default_global::<DraftRecallGlobal>().inactive.is_empty());
    });
}
