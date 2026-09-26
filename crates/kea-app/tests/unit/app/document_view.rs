use super::{is_at_document_tail, KeaView};
use crate::app::InitialFocus;
use gpui::{
    div, point, px, size, AppContext, Context, Entity, EntityInputHandler, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta, ScrollHandle,
    ScrollWheelEvent, StatefulInteractiveElement, Styled, TestAppContext, TouchPhase,
    VisualContext, VisualTestContext, Window,
};
use gpui_component::Root;
use kea_app::config::{keybindings::Keymap, settings::Settings};
use kea_document::Document;
use kea_session::Session;
use std::{cell::RefCell, ops::Deref, rc::Rc};

struct ScrollFixture {
    handle: ScrollHandle,
}

impl Render for ScrollFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut scroll = div()
            .id("document-scroll")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.handle)
            .flex()
            .flex_col();
        for _ in 0..20 {
            scroll = scroll.child(div().h(px(20.)).flex_shrink_0());
        }
        scroll
    }
}

fn append_block(document: &mut Document, id: u64) {
    let input = format!("command-{id}");
    document.note_submission(id, "local", &input, id);
    let output = format!(
        "\x1b]777;kea;native-start;local\x07output-{id}\x1b]777;kea;native-done;local;0\x07"
    );
    assert!(document.ingest_output(id + 1, output.as_bytes()));
}

fn make_document(block_count: u64) -> Document {
    let mut document = Document::new();
    for id in 1..=block_count {
        append_block(&mut document, id);
    }
    document
}

fn make_view(window: &mut Window, cx: &mut Context<KeaView>, document: Document) -> KeaView {
    let session = Session::demo().unwrap();
    let settings = Settings {
        show_blocks: true,
        ..Settings::default()
    };
    KeaView::new(
        session,
        document,
        None,
        Keymap::parse("").unwrap(),
        settings,
        InitialFocus::Editor,
        None,
        window,
        cx,
    )
}

fn draw_view(visual: &mut VisualTestContext, view: &Entity<KeaView>) {
    visual.draw(point(px(0.), px(0.)), size(px(1200.), px(800.)), |_, _| {
        view.clone()
    });
}

#[gpui::test]
fn wheel_above_tail_preserves_page_until_latest_then_following_resumes(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let view_slot = Rc::new(RefCell::new(None));
    let window = cx.add_window({
        let view_slot = view_slot.clone();
        move |window, cx| {
            let view = cx.new(|cx| make_view(window, cx, make_document(30)));
            *view_slot.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        }
    });
    let view = view_slot.borrow_mut().take().unwrap();
    let visual = VisualTestContext::from_window(*window.deref(), cx).into_mut();
    draw_view(visual, &view);
    let initial = visual.update(|_, cx| view.read(cx).document_ui.last_start);
    assert_eq!(initial, 6);
    assert!(visual.update(|_, cx| is_at_document_tail(&view.read(cx).document_scroll)));

    visual.simulate_event(ScrollWheelEvent {
        position: point(px(1700.), px(400.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(20.))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    let before_append = visual.update(|_, cx| view.read(cx).document_scroll.offset());
    assert!(!visual.update(|_, cx| is_at_document_tail(&view.read(cx).document_scroll)));
    let reading = visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            this.document_ui.visible.values().any(|block| {
                block.editor.focus_handle(cx).is_focused(window)
                    || block.editor.update(cx, |state, cx| {
                        state
                            .selected_text_range(true, window, cx)
                            .is_some_and(|selection| !selection.range.is_empty())
                    })
            })
        })
    });
    assert!(!reading);

    visual.update(|_, cx| {
        view.update(cx, |this, _cx| {
            append_block(&mut this.document, 31);
            this.document_ui.dirty = true;
        });
    });
    draw_view(visual, &view);
    let after_append = visual.update(|_, cx| {
        let this = view.read(cx);
        (
            this.document_ui.page_start,
            this.document_ui.last_start,
            this.document_scroll.offset(),
        )
    });
    assert_eq!(after_append.0, Some(initial));
    assert_eq!(after_append.1, initial);
    assert_eq!(after_append.2, before_append);

    visual.update(|_, cx| {
        view.update(cx, |this, _cx| {
            this.document_ui.page_start = None;
            this.document_ui.latest_navigation = true;
        });
    });
    draw_view(visual, &view);
    assert!(visual.update(|_, cx| {
        let this = view.read(cx);
        this.document_ui.page_start.is_none() && is_at_document_tail(&this.document_scroll)
    }));

    visual.update(|_, cx| {
        view.update(cx, |this, _cx| {
            append_block(&mut this.document, 32);
            this.document_ui.dirty = true;
        });
    });
    draw_view(visual, &view);
    assert!(visual.update(|_, cx| {
        let this = view.read(cx);
        this.document_ui.page_start.is_none() && is_at_document_tail(&this.document_scroll)
    }));
}

#[gpui::test]
fn scroll_handle_geometry_distinguishes_tail_from_wheel_reader(cx: &mut TestAppContext) {
    let handle = ScrollHandle::new();
    let visual = cx.add_empty_window();
    let view = visual.new_window_entity(|_, _| ScrollFixture {
        handle: handle.clone(),
    });
    visual.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
        view.clone()
    });

    assert!(handle.max_offset().height > px(0.));
    handle.scroll_to_bottom();
    visual.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, _| {
        view.clone()
    });
    assert!(is_at_document_tail(&handle));

    visual.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(50.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(20.))),
        touch_phase: TouchPhase::Moved,
        ..Default::default()
    });
    assert!(!is_at_document_tail(&handle));
}
