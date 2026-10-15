use super::{workflow, EditPlan, KeaView};
use crate::app::InitialFocus;
use gpui::{App, AppContext as _, Context, Entity, Focusable as _, TestAppContext, Window};
use gpui_component::{input::RopeExt as _, Root};
use kea_app::config::{keybindings::Keymap, settings::Settings};
use kea_app::editor::command as command_editor;
use kea_document::Document;
use kea_session::Session;

fn make_view(window: &mut Window, cx: &mut App) -> Entity<KeaView> {
    let session = Session::demo().unwrap();
    let document = Document::from_recording(session.recording());
    cx.new(|cx| {
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
    })
}

fn write_at(
    view: &mut KeaView,
    text: &str,
    cursor: usize,
    window: &mut Window,
    cx: &mut Context<KeaView>,
) {
    view.editor.update(cx, |state, cx| {
        state.set_value(text.to_owned(), window, cx);
        let position = state.text().offset_to_position(cursor);
        state.set_cursor_position(position, window, cx);
        state.focus(window, cx);
    });
}

#[gpui::test]
fn structural_splits_keep_other_entities_and_undo_restores_literal_marker(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            let text = "first\n---\nsecond &&\nthird";
            write_at(this, text, 7, window, cx);
            let original = this.editor.clone();
            let rec = workflow::recommend(text, 7, "::").remove(0);
            let plan = EditPlan::for_recommendation(text, 7, &rec, None).unwrap();
            assert_eq!(this.editor.read(cx).value().as_ref(), text);
            this.apply_composer_plan(plan, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 2);
            assert_eq!(
                this.composer_workflow.sections[0].read(cx).value().as_ref(),
                "first"
            );
            assert_eq!(
                this.composer_workflow.sections[1].read(cx).value().as_ref(),
                "second &&\nthird"
            );
            this.undo_composer_layout(false, window, cx);
            assert_eq!(this.editor, original);
            assert_eq!(this.editor.read(cx).value().as_ref(), text);
            this.undo_composer_layout(true, window, cx);
            let first = this.composer_workflow.sections[0].clone();
            let second = this.composer_workflow.sections[1].clone();
            this.finish_section_submission("first", window, cx);
            // Default: the submitted tab closes and the next tab takes over.
            assert_eq!(this.editor, second);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            assert!(this.composer_workflow.redo.is_empty());
            // Structural undo reopens the submitted tab with its text; nothing is sent.
            this.undo_composer_layout(false, window, cx);
            assert_eq!(this.composer_workflow.sections, [first.clone(), second]);
            assert_eq!(this.editor, first);
            assert_eq!(this.editor.read(cx).value().as_ref(), "first");
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn failed_send_preserves_all_sections(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            write_at(this, "first\nsecond", 6, window, cx);
            let _ = command_editor::submission_text(&this.editor, window, cx);
            this.split_composer(window, cx);
            let sections = this.composer_workflow.sections.clone();
            this.run_shell(window, cx); // demo/history rejects live input
            assert_eq!(this.composer_workflow.sections, sections);
            assert_eq!(this.editor, sections[1]);
            assert_eq!(this.editor.read(cx).value().as_ref(), "second");
            assert!(this.has_pending_sections(cx));
            this.navigate_composer(false, window, cx);
            assert_eq!(this.editor, sections[0]);
            assert_eq!(this.editor.read(cx).value().as_ref(), "first\n");
            this.navigate_composer(true, window, cx);
            assert_eq!(this.editor, sections[1]);
            assert_eq!(this.editor.read(cx).value().as_ref(), "second");
            this.finish_section_submission("second", window, cx);
            // Closing the last tab continues with the previous one.
            assert_eq!(this.composer_workflow.sections, [sections[0].clone()]);
            assert_eq!(this.editor, sections[0]);
            assert!(this.has_pending_sections(cx));
            // The only remaining tab is emptied, never closed.
            this.finish_section_submission("first\n", window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            assert!(this.editor.read(cx).value().is_empty());
            assert!(!this.has_pending_sections(cx));
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn stale_preview_and_layout_undo_never_discard_later_edits(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            write_at(this, "left\nright", 5, window, cx);
            let stale = EditPlan::split("left\nright", 5).unwrap();
            write_at(this, "changed", 7, window, cx);
            this.apply_composer_plan(stale, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            assert_eq!(this.editor.read(cx).value().as_ref(), "changed");
            this.split_composer(window, cx);
            write_at(this, "new text", 8, window, cx);
            this.undo_composer_layout(false, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 2);
            assert_eq!(this.editor.read(cx).value().as_ref(), "new text");
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn opening_and_cancelling_actions_keeps_literal_input(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            write_at(this, "::split", 7, window, cx);
            this.open_composer_actions(window, cx);
            assert!(this.composer_workflow.menu.is_some());
            assert_eq!(this.editor.read(cx).value().as_ref(), "::split");
            this.dismiss_composer_actions(window, cx);
            assert!(this.composer_workflow.menu.is_none());
            assert_eq!(this.editor.read(cx).value().as_ref(), "::split");
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn split_suggestion_applies_once_without_a_menu_and_focuses_following_draft(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            let text = "écho first\n---\nsecond";
            let cursor = text.find("---").unwrap() + 3;
            write_at(this, text, cursor, window, cx);
            let target = this.editor.clone();
            let events = this.session.recording().events().len();
            let rec = this.composer_recommendations(cx).remove(0);
            let plan = EditPlan::for_recommendation(text, cursor, &rec, None).unwrap();
            this.accept_split_recommendation(&target, plan.clone(), window, cx);
            assert!(this.composer_workflow.menu.is_none());
            assert_eq!(this.composer_workflow.sections.len(), 2);
            assert_eq!(
                this.composer_workflow.sections[0].read(cx).value().as_ref(),
                "écho first"
            );
            assert_eq!(this.editor.read(cx).value().as_ref(), "second");
            assert_eq!(this.composer_workflow.active, 1);
            assert_eq!(this.editor.read(cx).cursor(), 0);
            assert!(this.editor.focus_handle(cx).is_focused(window));
            assert_eq!(this.session.recording().events().len(), events);
            // A second queued click cannot split the newly focused editor.
            this.accept_split_recommendation(&target, plan, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 2);
            this.undo_composer_layout(false, window, cx);
            assert_eq!(this.editor, target);
            assert_eq!(this.editor.read(cx).value().as_ref(), text);
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn split_suggestion_rejects_changed_text_caret_and_section_limit(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            let text = "first\n---\nsecond";
            let cursor = 9;
            write_at(this, text, cursor, window, cx);
            let target = this.editor.clone();
            let rec = this.composer_recommendations(cx).remove(0);
            let plan = EditPlan::for_recommendation(text, cursor, &rec, None).unwrap();
            write_at(this, text, 0, window, cx);
            this.accept_split_recommendation(&target, plan.clone(), window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            write_at(this, "edited", 6, window, cx);
            this.accept_split_recommendation(&target, plan.clone(), window, cx);
            assert_eq!(this.editor.read(cx).value().as_ref(), "edited");
            assert_eq!(this.composer_workflow.sections.len(), 1);
            write_at(this, text, cursor, window, cx);
            for _ in 1..workflow::MAX_SECTIONS {
                let editor =
                    command_editor::new_draft(this.shell, &this.settings, "keep", window, cx);
                this.composer_workflow.sections.push(editor);
            }
            this.accept_split_recommendation(&target, plan, window, cx);
            assert_eq!(
                this.composer_workflow.sections.len(),
                workflow::MAX_SECTIONS
            );
            assert_eq!(this.editor.read(cx).value().as_ref(), text);
            assert!(this.composer_workflow.undo.is_empty());
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn keep_setting_empties_the_submitted_tab_in_place(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            this.settings.submitted_draft = kea_app::config::settings::SubmittedDraft::Keep;
            write_at(this, "first\nsecond", 6, window, cx);
            this.split_composer(window, cx);
            let sections = this.composer_workflow.sections.clone();
            this.navigate_composer(false, window, cx);
            this.finish_section_submission("first\n", window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 2);
            assert_eq!(this.composer_workflow.active, 0);
            assert_ne!(this.editor, sections[0]);
            assert!(this.editor.read(cx).value().is_empty());
            assert_eq!(this.composer_workflow.sections[1], sections[1]);
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn draft_tabs_add_close_and_reopen_without_sending(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let view = make_view(window, cx);
        view.update(cx, |this, cx| {
            write_at(this, "echo one", 8, window, cx);
            let original = this.editor.clone();
            this.add_composer_section(window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 2);
            assert_eq!(this.composer_workflow.active, 1);
            assert!(this.editor.read(cx).value().is_empty());
            assert_eq!(this.draft_tab_label(0, cx), "echo one");
            assert_eq!(this.draft_tab_label(1, cx), "Draft 2");
            // Closing an inactive tab keeps the active draft.
            let added = this.editor.clone();
            this.close_composer_section(0, window, cx);
            assert_eq!(
                this.composer_workflow.sections,
                std::slice::from_ref(&added)
            );
            assert_eq!(this.editor, added);
            this.undo_composer_layout(false, window, cx);
            assert_eq!(this.composer_workflow.sections, [original.clone(), added]);
            assert_eq!(original.read(cx).value().as_ref(), "echo one");
            // The last tab cannot be closed.
            this.close_composer_section(0, window, cx);
            this.close_composer_section(0, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 1);
        });
        Root::new(view, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn moved_drafts_get_new_editors_and_leave_the_source(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let source = make_view(window, cx);
        let target = make_view(window, cx);
        let moved = source.update(cx, |this, cx| {
            write_at(this, "keep\nmove me", 5, window, cx);
            this.split_composer(window, cx);
            this.editor.clone()
        });
        target
            .update(cx, |this, cx| {
                this.receive_moved_draft("move me", window, cx)
            })
            .unwrap();
        source.update(cx, |this, cx| {
            this.complete_draft_move(&moved, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            assert_eq!(this.editor.read(cx).value().as_ref(), "keep\n");
            assert!(this.composer_workflow.undo.is_empty());
        });
        target.update(cx, |this, cx| {
            let sections = &this.composer_workflow.sections;
            assert_eq!(sections.len(), 2);
            assert!(
                !sections.contains(&moved),
                "editors are never shared across tabs"
            );
            assert_eq!(sections[1].read(cx).value().as_ref(), "move me");
            // A hidden tab only selects the moved draft once it is shown.
            assert_eq!(this.composer_workflow.active, 0);
            this.reveal_moved_draft(cx);
            assert_eq!(this.composer_workflow.active, 1);
            assert_eq!(this.editor.read(cx).value().as_ref(), "move me");
        });
        Root::new(source, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}

#[gpui::test]
fn the_only_draft_can_be_moved_and_leaves_an_empty_draft(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let source = make_view(window, cx);
        let target = make_view(window, cx);
        let moved = source.update(cx, |this, cx| {
            write_at(this, "only draft", 10, window, cx);
            this.editor.clone()
        });
        target
            .update(cx, |this, cx| {
                this.receive_moved_draft("only draft", window, cx)
            })
            .unwrap();
        source.update(cx, |this, cx| {
            this.complete_draft_move(&moved, window, cx);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            assert_ne!(this.editor, moved);
            assert!(this.editor.read(cx).value().is_empty());
        });
        Root::new(source, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}
