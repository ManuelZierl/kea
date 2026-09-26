use super::{workflow, EditPlan, KeaView};
use crate::app::InitialFocus;
use gpui::{App, AppContext as _, Context, Entity, TestAppContext, Window};
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
        state.set_value(text, window, cx);
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
            let second = this.composer_workflow.sections[1].clone();
            this.finish_section_submission(window, cx);
            assert_eq!(this.editor, second);
            assert_eq!(this.composer_workflow.sections.len(), 1);
            assert!(this.composer_workflow.undo.is_empty());
            assert!(this.composer_workflow.redo.is_empty());
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
            assert_eq!(this.editor.read(cx).value().as_ref(), "first\n");
            assert!(this.has_pending_sections(cx));
            this.navigate_composer(true, window, cx);
            assert_eq!(this.editor, sections[1]);
            assert_eq!(this.editor.read(cx).value().as_ref(), "second");
            this.finish_section_submission(window, cx);
            assert_eq!(this.composer_workflow.sections[0], sections[0]);
            assert!(this.editor.read(cx).value().is_empty());
            assert!(this.has_pending_sections(cx));
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
