//! Thin host adapter over GPUI Component, not another text editor implementation.
//! Selection, grapheme movement, undo, IME and pointer handling belong upstream.
use crate::{draft_history::DraftHistory, settings::Settings, shell::ShellFlavor};
use gpui::{
    App, AppContext, Entity, EntityInputHandler, Global, Subscription, Window,
};
use gpui_component::{
    highlighter::{LanguageConfig, LanguageRegistry},
    input::InputState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryDirection {
    Previous,
    Next,
}

#[derive(Default)]
struct DraftRecallGlobal {
    history: DraftHistory,
    current_editor: Option<Entity<InputState>>,
    submission_candidate: Option<(Entity<InputState>, String)>,
    interceptor: Option<Subscription>,
}

impl Global for DraftRecallGlobal {}

pub fn register_languages() {
    let registry = LanguageRegistry::singleton();
    registry.register(
        "bash",
        &LanguageConfig::new(
            "Bash",
            tree_sitter_bash::LANGUAGE.into(),
            vec![],
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ),
    );
    // With the all-languages feature disabled, there is no built-in plain language.
    // Reuse the compiled grammar with empty queries: no syntax colours/substitutions.
    registry.register(
        "text",
        &LanguageConfig::new(
            "Plain text",
            tree_sitter_bash::LANGUAGE.into(),
            vec![],
            "",
            "",
            "",
        ),
    );
}

/// Each submitted draft gets a fresh entity and its own undo history.
///
/// `main` already has a useful success boundary: it creates an empty fresh draft only
/// after PTY send succeeds. `submission_text` remembers the exact candidate associated
/// with the current editor, and this constructor commits that candidate to history only
/// when that exact editor is replaced by an empty draft. Failed sends therefore retain
/// the draft but never enter submitted history. Non-empty `Edit as new` drafts also do
/// not look like submissions.
pub fn new_draft(
    shell: Option<ShellFlavor>,
    settings: &Settings,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    let submitted = if text.is_empty() {
        let recall = cx.default_global::<DraftRecallGlobal>();
        match (&recall.current_editor, &recall.submission_candidate) {
            (Some(current), Some((candidate, text))) if current == candidate && !text.is_empty() => {
                Some(text.clone())
            }
            _ => None,
        }
    } else {
        None
    };
    if let Some(submitted) = submitted {
        cx.default_global::<DraftRecallGlobal>()
            .history
            .record(submitted);
    }

    let language = if settings.syntax_highlighting && shell == Some(ShellFlavor::Posix) {
        "bash"
    } else {
        "text"
    };
    let editor = cx.new(|cx| {
        InputState::new(window, cx)
            .code_editor(language)
            .line_number(settings.line_numbers)
            .soft_wrap(settings.soft_wrap)
            .searchable(true)
            .default_value(text.to_string())
    });
    let recall = cx.default_global::<DraftRecallGlobal>();
    recall.current_editor = Some(editor.clone());
    recall.submission_candidate = None;
    recall.history.reset_navigation();
    editor
}

/// Exact focus excludes the embedded find field; composition is not execution.
pub fn submission_text(
    editor: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) -> Option<String> {
    use gpui::Focusable;
    if !editor.focus_handle(cx).is_focused(window) {
        return None;
    }
    let text = editor.update(cx, |state, cx| {
        if state.marked_text_range(window, cx).is_some() {
            None
        } else {
            Some(state.value().to_string())
        }
    });
    if let Some(text) = &text {
        cx.default_global::<DraftRecallGlobal>().submission_candidate =
            Some((editor.clone(), text.clone()));
    }
    text
}

/// Retain the keystroke interceptor for as long as the app lives.
pub fn retain_history_interceptor(subscription: Subscription, cx: &mut App) {
    cx.default_global::<DraftRecallGlobal>().interceptor = Some(subscription);
}

/// Navigate exact text that was successfully submitted through either Run in shell or
/// Send to app. This acts only on the active Kea command editor; terminal/TUI input is
/// untouched. Replacing the draft is a normal undoable editor operation and never sends
/// bytes to the child process.
pub fn navigate_submitted_drafts(
    direction: HistoryDirection,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    use gpui::Focusable;
    let Some(editor) = cx
        .try_global::<DraftRecallGlobal>()
        .and_then(|recall| recall.current_editor.clone())
    else {
        return false;
    };
    if !editor.focus_handle(cx).is_focused(window) {
        return false;
    }
    let composing = editor.update(cx, |state, cx| {
        state.marked_text_range(window, cx).is_some()
    });
    if composing {
        return false;
    }

    let current = editor.read(cx).value().to_string();
    let replacement = {
        let recall = cx.default_global::<DraftRecallGlobal>();
        match direction {
            HistoryDirection::Previous => recall.history.previous(&current),
            HistoryDirection::Next => recall.history.next(&current),
        }
    };
    let Some(replacement) = replacement else {
        return false;
    };

    if replacement != current {
        let old_utf16 = current.encode_utf16().count();
        let cursor = replacement.len();
        editor.update(cx, |state, cx| {
            state.replace_text_in_range(Some(0..old_utf16), &replacement, window, cx);
            state.set_selected_range(cursor..cursor, cx);
            state.focus(window, cx);
        });
    }
    true
}

#[cfg(test)]
pub(crate) fn submitted_history_len(cx: &mut App) -> usize {
    cx.default_global::<DraftRecallGlobal>().history.len()
}

/// Use the component's scrolling after layout; do not steal focus or a selection.
pub fn follow_output_tail(editor: &Entity<InputState>, window: &mut Window, _cx: &mut App) {
    let editor = editor.downgrade();
    window.on_next_frame(move |window, cx| {
        let previous = window.focused(cx);
        let _ = editor.update(cx, |state, cx| {
            use gpui_component::input::RopeExt as _;
            if state
                .selected_text_range(true, window, cx)
                .is_some_and(|s| !s.range.is_empty())
            {
                return;
            }
            let position = state.text().offset_to_position(state.text().len());
            state.set_cursor_position(position, window, cx);
        });
        if let Some(previous) = previous {
            window.focus(&previous);
        } else {
            window.blur();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use gpui_component::Root;
    struct EmptyView;
    impl gpui::Render for EmptyView {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
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
                let editor = new_draft(None, &Settings::default(), "prompt\nwith details", window, cx);
                editor.update(cx, |state, cx| state.focus(window, cx));
                assert_eq!(submission_text(&editor, window, cx).as_deref(), Some("prompt\nwith details"));
                let fresh = new_draft(None, &Settings::default(), "", window, cx);
                fresh.update(cx, |state, cx| state.focus(window, cx));
                assert_eq!(submitted_history_len(cx), 1);
                assert!(navigate_submitted_drafts(HistoryDirection::Previous, window, cx));
                assert_eq!(fresh.read(cx).value().as_ref(), "prompt\nwith details");
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
                assert_eq!(submission_text(&editor, window, cx).as_deref(), Some("not sent"));
                assert_eq!(submitted_history_len(cx), 0);

                let edited = new_draft(None, &Settings::default(), "historical block", window, cx);
                edited.update(cx, |state, cx| state.focus(window, cx));
                assert_eq!(submitted_history_len(cx), 0);
                assert!(!navigate_submitted_drafts(HistoryDirection::Previous, window, cx));
            })
            .unwrap();
    }
}
