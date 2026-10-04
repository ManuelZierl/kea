use super::{KeybindingEditor, Keymap};
use gpui::{
    div, point, px, size, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window,
};
use gpui_component::{
    input::{Input, InputEvent, InputState},
    Root,
};

struct Empty;
impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct ZeroWidthInput {
    state: gpui::Entity<InputState>,
}

impl Render for ZeroWidthInput {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(0.)).child(Input::new(&self.state))
    }
}

#[gpui::test]
fn invalid_shortcuts_preserve_saved_file_and_edits_then_valid_save_round_trips(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let empty = cx.new(|_| Empty);
        Root::new(empty, window, cx)
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("keybindings.conf");
    let initial = Keymap::parse("").unwrap();
    initial.save_to(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    window
        .update(cx, |_, window, cx| {
            let editor = cx
                .new(|cx| KeybindingEditor::from_keymap(&initial, Some(path.clone()), window, cx));
            editor.update(cx, |this, cx| {
                let field = this
                    .rows
                    .iter()
                    .find(|r| r.name == "focus_editor")
                    .unwrap()
                    .input
                    .clone();
                // Ctrl-R is still bound to reverse_search: reject the entire edit.
                field.update(cx, |input, cx| input.set_value("ctrl-r", window, cx));
                this.save(cx);
                assert!(this.error);
                assert!(this.status.contains("assigned to both"));
                assert_eq!(field.read(cx).value().as_ref(), "ctrl-r");
                assert_eq!(std::fs::read(&path).unwrap(), before);
                field.update(cx, |input, cx| input.set_value("alt-l", window, cx));
                this.save(cx);
                assert!(!this.error);
                assert!(this.status.contains("Restart Kea"));
                assert_eq!(
                    Keymap::parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
                    Keymap::parse("focus_editor = alt-l").unwrap()
                );
            });
        })
        .unwrap();
}

#[gpui::test]
fn enter_in_a_shortcut_field_saves_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let empty = cx.new(|_| Empty);
        Root::new(empty, window, cx)
    });
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("keybindings.conf");
    Keymap::parse("").unwrap().save_to(&path).unwrap();
    let editor = window
        .update(cx, |_, window, cx| {
            let initial = Keymap::parse("").unwrap();
            let editor = cx
                .new(|cx| KeybindingEditor::from_keymap(&initial, Some(path.clone()), window, cx));
            let field = editor
                .read(cx)
                .rows
                .iter()
                .find(|row| row.name == "focus_editor")
                .unwrap()
                .input
                .clone();
            field.update(cx, |input, cx| input.set_value("alt-l", window, cx));
            field.update(cx, |_, cx| {
                cx.emit(InputEvent::PressEnter { secondary: false });
            });
            editor
        })
        .unwrap();
    window
        .update(cx, |_, _, cx| {
            editor.update(cx, |this, _| {
                assert!(!this.error);
                assert!(this.status.contains("Restart Kea"));
            });
            let _ = cx;
        })
        .unwrap();
    assert_eq!(
        Keymap::parse(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        Keymap::parse("focus_editor = alt-l").unwrap()
    );
}

#[gpui::test]
fn save_failure_is_visible_and_preserves_edited_input(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let empty = cx.new(|_| Empty);
        Root::new(empty, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let keymap = Keymap::parse("").unwrap();
            let editor = cx.new(|cx| KeybindingEditor::from_keymap(&keymap, None, window, cx));
            editor.update(cx, |this, cx| {
                this.rows[0]
                    .input
                    .update(cx, |input, cx| input.set_value("alt-l", window, cx));
                this.save(cx);
                assert!(this.error);
                assert!(this.status.contains("path unavailable"));
                assert_eq!(this.rows[0].input.read(cx).value().as_ref(), "alt-l");
            });
        })
        .unwrap();
}

#[gpui::test]
fn shortcut_capture_waits_for_release_and_escape_preserves_text(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let window = cx.add_window(|window, cx| {
        let empty = cx.new(|_| Empty);
        Root::new(empty, window, cx)
    });
    window
        .update(cx, |_, window, cx| {
            let keymap = Keymap::parse("").unwrap();
            let editor = cx.new(|cx| KeybindingEditor::from_keymap(&keymap, None, window, cx));
            editor.update(cx, |this, cx| {
                this.begin_recording(0, window, cx);
                this.record_key(&gpui::Keystroke::parse("alt-l").unwrap(), window, cx);
                assert_eq!(this.rows[0].input.read(cx).value().as_ref(), "alt-l");
                assert!(this.capture_focus.is_focused(window));
                assert!(this.capture_release.is_some());
                this.record_key(&gpui::Keystroke::parse("enter").unwrap(), window, cx);
                assert_eq!(this.rows[0].input.read(cx).value().as_ref(), "alt-l");
                assert!(!this.error); // No save to the intentionally unavailable path.
                this.finish_capture("l", window, cx);
                assert!(this.capture_release.is_none());
                this.begin_recording(0, window, cx);
                this.record_key(&gpui::Keystroke::parse("escape").unwrap(), window, cx);
                assert_eq!(this.rows[0].input.read(cx).value().as_ref(), "alt-l");
                assert!(this.status.contains("cancelled"));
                this.finish_capture("escape", window, cx);
                assert!(this.capture_release.is_none());
            });
        })
        .unwrap();
}

#[gpui::test]
fn record_button_click_arms_capture_and_captures_a_real_keystroke(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let editor_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let window = cx.add_window({
        let editor_slot = editor_slot.clone();
        move |window, cx| {
            let keymap = Keymap::parse("").unwrap();
            let editor = cx.new(|cx| KeybindingEditor::from_keymap(&keymap, None, window, cx));
            *editor_slot.borrow_mut() = Some(editor.clone());
            Root::new(editor, window, cx)
        }
    });
    let editor = editor_slot.borrow_mut().take().unwrap();
    let visual = VisualTestContext::from_window(*window, cx).into_mut();
    visual.draw(point(px(0.), px(0.)), size(px(1000.), px(1600.)), |_, _| {
        editor.clone()
    });
    let bounds = visual
        .debug_bounds("record-shortcut-0")
        .expect("Record button rendered");
    visual.simulate_click(bounds.center(), gpui::Modifiers::none());
    assert_eq!(visual.update(|_, cx| editor.read(cx).recording), Some(0));
    visual.simulate_keystrokes("alt-l");
    visual.update(|window, cx| {
        editor.update(cx, |this, cx| this.finish_capture("l", window, cx));
    });
    visual.draw(point(px(0.), px(0.)), size(px(1000.), px(1600.)), |_, _| {
        editor.clone()
    });
    assert_eq!(
        visual.update(|_, cx| editor.read(cx).rows[0].input.read(cx).value().to_string()),
        "alt-l"
    );
}

#[gpui::test]
fn programmatic_value_does_not_scroll_against_a_zero_width_layout(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let state_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let input_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let window = cx.add_window({
        let state_slot = state_slot.clone();
        let input_slot = input_slot.clone();
        move |window, cx| {
            let state = cx.new(|cx| InputState::new(window, cx).default_value("f10"));
            let input = cx.new(|_| ZeroWidthInput {
                state: state.clone(),
            });
            *state_slot.borrow_mut() = Some(state);
            *input_slot.borrow_mut() = Some(input.clone());
            Root::new(input, window, cx)
        }
    });
    let state = state_slot.borrow_mut().take().unwrap();
    let input = input_slot.borrow_mut().take().unwrap();
    let visual = VisualTestContext::from_window(*window, cx).into_mut();
    visual.draw(point(px(0.), px(0.)), size(px(1000.), px(200.)), |_, _| {
        input.clone()
    });
    visual.update(|window, cx| {
        state.update(cx, |state, cx| {
            state.set_value("alt-f10", window, cx);
            state.focus(window, cx);
        });
    });
    visual.draw(point(px(0.), px(0.)), size(px(1000.), px(200.)), |_, _| {
        input.clone()
    });
    assert_eq!(
        visual.update(|_, cx| state.read(cx).scroll_offset_for_test().x),
        px(0.),
        "zero-width layout must not scroll a programmatic value out of view"
    );
}
