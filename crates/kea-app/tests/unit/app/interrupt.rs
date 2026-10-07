use crate::app::{InitialFocus, KeaView};
use gpui::{
    point, px, size, AppContext as _, Context, Keystroke, TestAppContext, VisualTestContext, Window,
};
use gpui_component::Root;
use kea_app::{
    config::{keybindings::Keymap, settings::Settings},
    terminal::input,
};
use kea_document::Document;
use kea_session::Session;
use std::{cell::RefCell, ops::Deref, rc::Rc};

#[test]
fn interrupt_classification_covers_chords_with_the_same_terminal_encoding() {
    assert!(super::is_ctrl_c(&Keystroke::parse("ctrl-shift-c").unwrap()));
    assert!(!super::is_ctrl_c(&Keystroke::parse("ctrl-alt-c").unwrap()));
    let ctrl_shift_c = Keystroke::parse("ctrl-shift-c").unwrap();
    let bytes = input::encode(&ctrl_shift_c, false, false).unwrap();
    assert_eq!(bytes, [3]);
}

#[test]
fn shift_modified_ctrl_c_is_not_consumed_as_selection_copy() {
    let local = kea_app::terminal::selection::local_key("c", true, true, false, false, false, true);
    assert_eq!(local, kea_app::terminal::selection::LocalKey::Forward);
    let copy = kea_app::terminal::selection::local_key("c", false, true, false, false, false, true);
    assert_eq!(copy, kea_app::terminal::selection::LocalKey::Copy);
}

fn make_view(window: &mut Window, cx: &mut Context<KeaView>) -> KeaView {
    #[cfg(unix)]
    let command = vec!["sh".into()];
    #[cfg(windows)]
    let command = vec!["cmd.exe".into(), "/Q".into()];
    let session = Session::spawn(&command, kea_core::Size::new(90, 22).unwrap(), None).unwrap();
    let document = Document::from_recording(session.recording());
    KeaView::new(
        session,
        document,
        None,
        Keymap::parse("").unwrap(),
        Settings {
            confirm_ctrl_c: true,
            ..Settings::default()
        },
        InitialFocus::Terminal,
        None,
        window,
        cx,
    )
}

#[gpui::test]
fn ctrl_c_confirmation_overlay_keeps_terminal_bounds(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let view_slot = Rc::new(RefCell::new(None));
    let window = cx.add_window({
        let view_slot = view_slot.clone();
        move |window, cx| {
            let view = cx.new(|cx| make_view(window, cx));
            *view_slot.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        }
    });
    let view = view_slot.borrow_mut().take().unwrap();
    let visual = VisualTestContext::from_window(*window.deref(), cx).into_mut();

    visual.draw(point(px(0.), px(0.)), size(px(1200.), px(800.)), |_, _| {
        view.clone()
    });
    let before = visual
        .update(|_, cx| view.read(cx).terminal_bounds)
        .expect("terminal bounds after initial draw");

    visual.update(|window, cx| {
        view.update(cx, |this, cx| {
            window.focus(&this.focus);
            this.request_interrupt(vec![3], window, cx);
            assert!(this.interrupt.gate.is_pending());
        });
    });
    visual.draw(point(px(0.), px(0.)), size(px(1200.), px(800.)), |_, _| {
        view.clone()
    });
    let after = visual
        .update(|_, cx| view.read(cx).terminal_bounds)
        .expect("terminal bounds with Ctrl-C confirmation visible");

    assert_eq!(
        after, before,
        "showing Ctrl-C confirmation must overlay the terminal instead of resizing it"
    );
}
