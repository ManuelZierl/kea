use super::*;
use crate::{config::settings::Settings, editor::command};
use gpui::{
    point, size, Focusable, KeyContext, Keystroke, MouseButton, ScrollWheelEvent, TestAppContext,
    VisualTestContext,
};
use gpui_component::Root;
use std::{cell::RefCell, ops::Deref, rc::Rc};

struct Empty;
impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct SearchLayerFixture {
    search: Entity<ReverseSearchView>,
    background_presses: usize,
    background_scrolls: usize,
    background_focus: FocusHandle,
}

impl Render for SearchLayerFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-behind-popup")
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| {
                    this.background_presses += 1;
                    window.focus(&this.background_focus);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, _, _, _| this.background_scrolls += 1))
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .top(px(500.))
                    .child(self.search.clone()),
            )
    }
}

#[gpui::test]
fn search_popup_occludes_terminal_pointer_input_and_retains_focus(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
        install(cx);
    });
    let fixture_slot = Rc::new(RefCell::new(None));
    let window = cx.add_window({
        let fixture_slot = fixture_slot.clone();
        move |window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "scratch", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.record("scratch".into(), InputKind::Application, None, cx);
                search.set_target(draft, true, cx);
                search.open(None, InputKind::Application, window, cx);
                settle_search(search, window, cx);
            });
            let fixture = cx.new(|cx| SearchLayerFixture {
                search,
                background_presses: 0,
                background_scrolls: 0,
                background_focus: cx.focus_handle(),
            });
            *fixture_slot.borrow_mut() = Some(fixture.clone());
            Root::new(fixture, window, cx)
        }
    });
    let fixture = fixture_slot.borrow_mut().take().unwrap();
    let visual = VisualTestContext::from_window(*window.deref(), cx).into_mut();
    visual.draw(point(px(0.), px(0.)), size(px(800.), px(600.)), |_, _| {
        fixture.clone()
    });
    let search = visual.update(|_, cx| fixture.read(cx).search.clone());
    // Bottom-left anchored panel ends at y=500. Its footer is blank here: a
    // real pointer click must not focus the terminal behind that visible panel.
    visual.simulate_mouse_move(point(px(320.), px(480.)), None, Default::default());
    visual.simulate_click(point(px(320.), px(480.)), Default::default());
    visual.update(|window, cx| {
        assert_eq!(
            fixture.read(cx).background_presses,
            0,
            "popup click leaked to terminal"
        );
        assert!(search.read(cx).focus.contains_focused(window, cx));
    });
    // Verify the same boundary while awaiting destructive confirmation. A
    // wheel event over the popup must not scroll/send input to the terminal.
    visual.update(|window, cx| {
        search.update(cx, |search, cx| {
            settle_search(search, window, cx);
            search.show_actions(window, cx);
            search.item_action(ItemAction::Forget, window, cx);
            assert!(search.open, "fixture popup must remain open");
            assert!(
                matches!(search.mode, Mode::ConfirmDelete { .. }),
                "fixture must reach confirmation"
            );
        })
    });
    visual.draw(point(px(0.), px(0.)), size(px(800.), px(600.)), |_, _| {
        fixture.clone()
    });
    visual.simulate_event(ScrollWheelEvent {
        position: point(px(320.), px(480.)),
        delta: ScrollDelta::Lines(point(0., 1.)),
        ..Default::default()
    });
    visual.update(|_, cx| {
        assert_eq!(
            fixture.read(cx).background_scrolls,
            0,
            "popup wheel leaked to terminal"
        );
        assert!(matches!(search.read(cx).mode, Mode::ConfirmDelete { .. }));
        assert!(search.read(cx).pending_operation.is_none());
    });
    visual.simulate_mouse_move(point(px(700.), px(50.)), None, Default::default());
    visual.simulate_click(point(px(700.), px(50.)), Default::default());
    visual.update(|window, cx| {
        assert!(
            !search.read(cx).open,
            "outside click must still dismiss the popup"
        );
        assert!(fixture.read(cx).background_presses > 0);
        assert!(fixture.read(cx).background_focus.is_focused(window));
    });
}

#[gpui::test]
fn search_cancellation_preserves_editor_entity_text_and_selection(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "a😀\noriginal", window, cx);
            draft.update(cx, |state, cx| state.focus(window, cx));
            let before = draft.update(cx, |state, cx| state.selected_text_range(true, window, cx));
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                assert_eq!(search.query.read(cx).value().as_ref(), "a😀\noriginal");
                search.query.update(cx, |state, cx| {
                    state.set_value("different query", window, cx)
                });
                search.close(true, window, cx);
            });
            assert_eq!(draft.read(cx).value().as_ref(), "a😀\noriginal");
            let after = draft.update(cx, |state, cx| state.selected_text_range(true, window, cx));
            assert_eq!(before.map(|s| s.range), after.map(|s| s.range));
            assert!(draft.focus_handle(cx).is_focused(window));
        })
        .unwrap();
}

#[gpui::test]
fn insertion_uses_utf16_editor_replacement_and_refuses_stale_or_composing_drafts(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "a😀b", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                search.insert_text("printf 'ä'\necho done", window, cx);
                assert_eq!(draft.read(cx).value().as_ref(), "printf 'ä'\necho done");
                assert!(!search.open);
                search.open(None, InputKind::Application, window, cx);
                draft.update(cx, |state, cx| {
                    state.set_value("external change", window, cx)
                });
                search.insert_text("must not replace", window, cx);
                assert_eq!(draft.read(cx).value().as_ref(), "external change");
                search.close(false, window, cx);
                draft.update(cx, |state, cx| {
                    state.replace_and_mark_text_in_range(None, "日本", None, window, cx)
                });
                search.open(None, InputKind::Application, window, cx);
                assert!(!search.open);
            });
        })
        .unwrap();
}

#[test]
fn overlay_bindings_are_scoped_and_enter_is_not_a_host_execution_action() {
    let map = gpui::Keymap::new(bindings());
    let context = [
        KeyContext::parse("Kea").unwrap(),
        KeyContext::parse("KeaCommand").unwrap(),
        KeyContext::parse("KeaReverseSearch").unwrap(),
        KeyContext::parse("Input").unwrap(),
    ];
    let matches = map
        .bindings_for_input(&[Keystroke::parse("enter").unwrap()], &context)
        .0;
    assert!(!matches.is_empty());
    let terminal = [
        KeyContext::parse("Kea").unwrap(),
        KeyContext::parse("KeaTerminal").unwrap(),
    ];
    assert!(map
        .bindings_for_input(&[Keystroke::parse("enter").unwrap()], &terminal)
        .0
        .is_empty());
}

#[gpui::test]
fn keyboard_selection_survives_stationary_pointer_updates_and_mouse_can_resume(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "scratch", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                let mut library = crate::reverse_search::Library::default();
                for i in 0..8 {
                    library
                        .insert(Entry::submitted(
                            format!("entry {i}"),
                            InputKind::Application,
                            None,
                        ))
                        .unwrap();
                }
                search.hits = library.search("", None, 0).unwrap();
                search.visible_generation = Some(search.generation);
                let position = window.mouse_position();
                search.pointer_position = None;
                search.select_at_pointer(5, position, cx);
                search.move_selection(1, window, cx);
                assert_eq!(search.selected, 6);
                // A redraw or new page under the pointer is not new pointer input.
                search.select_at_pointer(7, position, cx);
                assert_eq!(search.selected, 6);
                search.move_selection(-1, window, cx);
                search.select_at_pointer(2, position, cx);
                assert_eq!(search.selected, 5);
                // Intentional mouse movement can take selection back, even within a row.
                let moved = Point::new(position.x + px(1.), position.y);
                search.select_at_pointer(2, moved, cx);
                assert_eq!(search.selected, 2);

                search.show_actions(window, cx);
                search.pointer_position = None;
                search.select_at_pointer(1, position, cx);
                search.move_selection(1, window, cx);
                search.select_at_pointer(1, position, cx);
                assert_eq!(search.selected_action, 2);
                search.select_at_pointer(0, moved, cx);
                assert_eq!(search.selected_action, 0);
                assert_eq!(draft.read(cx).value().as_ref(), "scratch");
            });
        })
        .unwrap();
}

#[gpui::test]
fn callbacks_from_previous_pages_and_menus_cannot_insert_or_change_confirmation(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "scratch", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                let mut library = crate::reverse_search::Library::default();
                for i in 0..8 {
                    library
                        .insert(Entry::submitted(
                            format!("entry {i}"),
                            InputKind::Application,
                            None,
                        ))
                        .unwrap();
                }
                search.hits = library.search("", None, 0).unwrap();
                search.visible_generation = Some(search.generation);
                let generation = search.generation;
                let first_id = search.hits[0].entry.id.clone();
                assert!(search.result_is_current(0, generation, &first_id));
                search.move_selection(PAGE_SIZE as isize, window, cx);
                assert!(!search.result_is_current(0, generation, &first_id));
                let id = search.hits[search.selected].entry.id.clone();
                search.show_actions(window, cx);
                assert!(search.actions_are_current(generation, &id));
                assert!(!search.result_is_current(search.selected, generation, &id));
                search.item_action(ItemAction::Forget, window, cx);
                let confirmation = search.mode.clone();
                assert!(matches!(confirmation, Mode::ConfirmDelete { .. }));
                assert!(!search.actions_are_current(generation, &id));
                search.item_action(ItemAction::Insert, window, cx);
                assert!(search.mode == confirmation);
                assert_eq!(draft.read(cx).value().as_ref(), "scratch");
                assert!(
                    search.pending_operation.is_none(),
                    "Forget must await explicit confirmation"
                );
                search.cancel(window, cx);
                assert!(matches!(search.mode, Mode::Results));
                assert!(search.pending_operation.is_none());
            });
        })
        .unwrap();
}

fn settle_search(
    search: &mut ReverseSearchView,
    window: &mut Window,
    cx: &mut Context<ReverseSearchView>,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        search.poll(window, cx);
        if search.pending_operation.is_none()
            && search.visible_generation == Some(search.generation)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker reply timed out"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[gpui::test]
fn confirmed_forget_refreshes_results_and_preserves_draft(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "scratch", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.record("needle".into(), InputKind::Application, None, cx);
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                search
                    .query
                    .update(cx, |state, cx| state.set_value("", window, cx));
                settle_search(search, window, cx);
                assert_eq!(search.hits.len(), 1);
                search.show_actions(window, cx);
                search.item_action(ItemAction::Forget, window, cx);
                search.accept(window, cx);
                settle_search(search, window, cx);
                assert!(search.hits.is_empty());
                assert_eq!(search.notice.as_deref(), Some("Forgotten."));
                assert_eq!(draft.read(cx).value().as_ref(), "scratch");
                search.close(true, window, cx);
                search.open(None, InputKind::Application, window, cx);
                search
                    .query
                    .update(cx, |state, cx| state.set_value("", window, cx));
                settle_search(search, window, cx);
                assert!(search.hits.is_empty());
            });
        })
        .unwrap();
}

#[gpui::test]
fn partial_forget_refreshes_group_and_keeps_failure_visible(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("memory");
    let store = crate::reverse_search::store::Store::new(directory.clone());
    for _ in 0..2 {
        store
            .write(&Entry::submitted(
                "needle".into(),
                InputKind::Application,
                None,
            ))
            .unwrap();
    }
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(
                    Worker::start(Some(directory.clone()), true),
                    true,
                    window,
                    cx,
                )
            });
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                settle_search(search, window, cx);
                assert_eq!(search.hits[0].occurrence_ids.len(), 2);
                let failed_id = search.hits[0].occurrence_ids[1].clone();
                let failed_path = directory.join(format!("{failed_id}.kmem"));
                std::fs::remove_file(&failed_path).unwrap();
                std::fs::create_dir(&failed_path).unwrap();
                search.show_actions(window, cx);
                search.item_action(ItemAction::Forget, window, cx);
                search.accept(window, cx);
                settle_search(search, window, cx);
                assert_eq!(search.hits[0].occurrence_ids, vec![failed_id]);
                assert!(search.warning().unwrap().contains("Forgot 1 of 2 records."));
                assert!(matches!(search.mode, Mode::Results));
                assert_eq!(draft.read(cx).value().as_ref(), "");
            });
        })
        .unwrap();
}

#[gpui::test]
fn delayed_forget_reply_cannot_dismiss_a_reopened_action_menu(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        command::register_languages();
    });
    let window = cx.add_window(|window, cx| {
        let view = cx.new(|_| Empty);
        Root::new(view, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let draft = command::new_draft(None, &Settings::default(), "", window, cx);
            let search = cx.new(|cx| {
                ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx)
            });
            search.update(cx, |search, cx| {
                search.record("first".into(), InputKind::Application, None, cx);
                search.record("second".into(), InputKind::Application, None, cx);
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                settle_search(search, window, cx);
                let remaining = search.hits[1].clone();
                search.show_actions(window, cx);
                search.item_action(ItemAction::Forget, window, cx);
                search.accept(window, cx);
                // Reopen before the UI polls the worker's completion.
                search.close(false, window, cx);
                search.open(None, InputKind::Application, window, cx);
                search.hits = vec![remaining];
                search.visible_generation = Some(search.generation);
                search.query_dirty = false;
                search.show_actions(window, cx);
                let generation = search.generation;
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                while search.pending_operation.is_some() {
                    search.poll(window, cx);
                    assert!(
                        std::time::Instant::now() < deadline,
                        "delete reply timed out"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(matches!(search.mode, Mode::Actions));
                assert_eq!(search.generation, generation);
                assert!(search.notice.is_none());
                assert!(search.focus.is_focused(window));
            });
        })
        .unwrap();
}
