use super::*;
use gpui::{AppContext as _, TestAppContext};

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

#[gpui::test]
fn tab_switch_and_close_transfer_held_keys_but_not_interrupts(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let first = make_view(window, cx);
        let root = cx.new(|cx| KeaRoot::new(first.clone(), window, cx));
        first.update(cx, |view, _| {
            assert!(view.composer_workflow.key_latch.press("enter"));
            view.composer_workflow.owned_keys.push("enter".into());
            assert!(view.interrupt.key_latch.press("enter"));
            view.interrupt.swallowed.push("c".into());
            view.interrupt.gate.arm(1, vec![3]);
        });
        root.update(cx, |this, cx| {
            let first_id = this.tabs.active_id().unwrap();
            this.suspend_active(window, cx);
            assert!(!first.read(cx).interrupt.gate.is_pending());
            assert!(first.read(cx).composer_workflow.owned_keys.is_empty());
            let second = make_view(window, cx);
            this.attach(
                second.clone(),
                "Second".into(),
                InitialFocus::Editor,
                window,
                cx,
            );
            this.focus_selected(window, cx);
            second.update(cx, |view, _| {
                assert_eq!(view.composer_workflow.owned_keys, ["enter"]);
                assert!(!view.composer_workflow.key_latch.press("return"));
                assert_eq!(view.interrupt.swallowed, ["c"]);
                assert!(!view.interrupt.key_latch.press("enter"));
                assert!(!view.interrupt.gate.is_pending());
                assert!(view.pending_run.is_none());
            });
            this.remove_tab(this.tabs.active_id().unwrap(), window, cx);
            assert_eq!(this.tabs.active_id(), Some(first_id));
            first.update(cx, |view, _| {
                assert_eq!(view.composer_workflow.owned_keys, ["enter"]);
                assert_eq!(view.interrupt.swallowed, ["c"]);
                assert!(!view.interrupt.gate.is_pending());
            });
            this.remove_tab(first_id, window, cx);
            assert!(this.tabs.is_empty());
            let held = this.held_keys.as_mut().unwrap();
            held.release("return");
            held.release("c");
            let third = make_view(window, cx);
            this.attach(
                third.clone(),
                "Third".into(),
                InitialFocus::Editor,
                window,
                cx,
            );
            this.focus_selected(window, cx);
            third.update(cx, |view, _| {
                assert!(view.composer_workflow.owned_keys.is_empty());
                assert!(view.interrupt.swallowed.is_empty());
                assert!(view.composer_workflow.key_latch.press("enter"));
                assert!(view.interrupt.key_latch.press("enter"));
            });
        });
        Root::new(root, window, cx)
    });
    window.update(cx, |_, _, _| {}).unwrap();
}
