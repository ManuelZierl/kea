//! Thin host adapter over GPUI Component, not another text editor implementation.
//! Selection, grapheme movement, undo, IME and pointer handling belong upstream.
use gpui::{App, AppContext, Entity, EntityInputHandler, Window};
use gpui_component::{highlighter::{LanguageConfig, LanguageRegistry}, input::InputState};
use crate::{settings::Settings, shell::ShellFlavor};

pub fn register_languages() {
    LanguageRegistry::singleton().register("bash", &LanguageConfig::new(
        "Bash", tree_sitter_bash::LANGUAGE.into(), vec![],
        tree_sitter_bash::HIGHLIGHT_QUERY, "", "",
    ));
}

/// A fresh entity gives each draft its own undo history. Submitting never edits
/// old command blocks; restoring an old command is an explicit new-draft action.
pub fn new_draft(shell: Option<ShellFlavor>, settings: &Settings, text: &str,
                 window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| {
        let state = InputState::new(window, cx).multi_line(true);
        let state = if settings.syntax_highlighting && shell == Some(ShellFlavor::Posix) {
            state.code_editor("bash")
        } else { state };
        state.line_number(settings.line_numbers).soft_wrap(settings.soft_wrap)
            .searchable(true).default_value(text.to_string())
    })
}

/// Do not submit intermediate IME text. The OS owns composition/candidate commit.
/// Exact focus also prevents a find field inside the editor from executing it.
pub fn submission_text(editor: &Entity<InputState>, window: &mut Window, cx: &mut App) -> Option<String> {
    use gpui::Focusable;
    if !editor.focus_handle(cx).is_focused(window) { return None }
    editor.update(cx, |state, cx| {
        if state.marked_text_range(window, cx).is_some() { None }
        else { Some(state.value().to_string()) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use gpui_component::Root;

    struct EmptyView;
    impl gpui::Render for EmptyView {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement { gpui::div() }
    }

    #[gpui::test]
    fn component_composition_is_not_a_submittable_command(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_component::init(cx); register_languages(); });
        let window = cx.add_window(|window, cx| {
            let view = cx.new(|_| EmptyView);
            Root::new(view, window, cx)
        });
        window.update(cx, |_, window, cx| {
            let editor = new_draft(Some(ShellFlavor::Posix), &Settings::default(), "", window, cx);
            editor.update(cx, |state, cx| {
                state.focus(window, cx);
                state.replace_and_mark_text_in_range(None, "日本", None, window, cx);
                assert!(state.marked_text_range(window, cx).is_some());
            });
            assert!(submission_text(&editor, window, cx).is_none());
            editor.update(cx, |state, cx| state.replace_text_in_range(None, "日本語", window, cx));
            assert_eq!(submission_text(&editor, window, cx).as_deref(), Some("日本語"));
        }).unwrap();
    }

    #[gpui::test]
    fn component_replaces_utf16_ranges_without_a_keycode_approximation(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let window = cx.add_window(|window, cx| {
            let view = cx.new(|_| EmptyView);
            Root::new(view, window, cx)
        });
        window.update(cx, |_, window, cx| {
            let editor = new_draft(None, &Settings::default(), "a😀ö", window, cx);
            editor.update(cx, |state, cx| {
                state.replace_text_in_range(Some(1..3), "ä", window, cx);
                assert_eq!(state.value().as_ref(), "aäö");
                let mut adjusted = None;
                assert_eq!(state.text_for_range(1..2, &mut adjusted, window, cx).as_deref(), Some("ä"));
            });
            let fresh = new_draft(None, &Settings::default(), "", window, cx);
            assert!(fresh.read(cx).value().is_empty());
            assert_ne!(fresh.entity_id(), editor.entity_id());
        }).unwrap();
    }
}
