//! Thin host adapter over GPUI Component, not another text editor implementation.
//! Selection, grapheme movement, undo, IME and pointer handling belong upstream.
use crate::{
    config::settings::{PostSubmitFocus, Settings},
    editor::history::DraftHistory,
    shell::ShellFlavor,
};
use gpui::{
    App, AppContext, Entity, EntityInputHandler, FocusHandle, Global, Subscription, Window,
};
use gpui_component::{
    highlighter::{LanguageConfig, LanguageRegistry},
    input::InputState,
};
use std::path::PathBuf;

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
    last_external_focus: Option<FocusHandle>,
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
            (Some(current), Some((candidate, text)))
                if current == candidate && !text.is_empty() =>
            {
                Some(text.clone())
            }
            _ => None,
        }
    } else {
        None
    };
    let submission_completed = submitted.is_some();
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

    // main deliberately focuses the terminal after a successful send. When the user
    // selected an editor-first workflow, defer one turn so that hand-off still happens
    // (and can be remembered), then make the fresh composer active. No child state is
    // guessed: this is only an explicit focus policy.
    if submission_completed && settings.post_submit_focus == PostSubmitFocus::Editor {
        let editor = editor.downgrade();
        window.defer(cx, move |window, cx| {
            let external = window.focused(cx);
            if let Some(external) = external {
                cx.default_global::<DraftRecallGlobal>().last_external_focus = Some(external);
            }
            let _ = editor.update(cx, |state, cx| state.focus(window, cx));
        });
    }

    editor
}

/// Apply presentation-only composer settings without replacing the entity, text,
/// selection, or undo history.
pub fn apply_settings(
    editor: &Entity<InputState>,
    shell: Option<ShellFlavor>,
    settings: &Settings,
    window: &mut Window,
    cx: &mut App,
) {
    let language = if settings.syntax_highlighting && shell == Some(ShellFlavor::Posix) {
        "bash"
    } else {
        "text"
    };
    editor.update(cx, |state, cx| {
        state.set_highlighter(language, cx);
        state.set_line_number(settings.line_numbers, window, cx);
        state.set_soft_wrap(settings.soft_wrap, window, cx);
    });
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
        cx.default_global::<DraftRecallGlobal>()
            .submission_candidate = Some((editor.clone(), text.clone()));
    }
    text
}

/// Seed submitted-draft recall with entries persisted by a previous session.
/// Called once at startup when `persist_history` is enabled; memory stays
/// authoritative afterwards and every new submission is written through.
pub fn set_history_persistence(path: PathBuf, entries: Vec<String>, cx: &mut App) {
    cx.default_global::<DraftRecallGlobal>()
        .history
        .set_persisted_entries(path, entries);
}

/// Take the first write-through failure for display by the owning workspace.
/// Recall remains memory-authoritative after persistence stops.
pub fn take_history_warning(cx: &mut App) -> Option<String> {
    cx.default_global::<DraftRecallGlobal>()
        .history
        .take_persistence_warning()
}

/// Retain the keystroke interceptor for as long as the app lives.
pub fn retain_history_interceptor(subscription: Subscription, cx: &mut App) {
    cx.default_global::<DraftRecallGlobal>().interceptor = Some(subscription);
}

/// Remember the non-composer surface the user was actually typing into. In the normal
/// loop this is the live terminal. Recording happens on a real key event rather than by
/// naming/sniffing a child application.
pub fn remember_external_focus(window: &mut Window, cx: &mut App) {
    use gpui::Focusable;
    let editor = cx
        .try_global::<DraftRecallGlobal>()
        .and_then(|recall| recall.current_editor.clone());
    if editor
        .as_ref()
        .is_some_and(|editor| editor.focus_handle(cx).is_focused(window))
    {
        return;
    }
    if let Some(focus) = window.focused(cx) {
        cx.default_global::<DraftRecallGlobal>().last_external_focus = Some(focus);
    }
}

/// Focus the last external surface, but only when the current command editor owns focus.
/// This gives composer-first users a symmetric keyboard path back to the terminal without
/// reserving the shortcut while a TUI owns input.
pub fn focus_last_external(window: &mut Window, cx: &mut App) -> bool {
    use gpui::Focusable;
    let (editor, external) = cx
        .try_global::<DraftRecallGlobal>()
        .map(|recall| {
            (
                recall.current_editor.clone(),
                recall.last_external_focus.clone(),
            )
        })
        .unwrap_or_default();
    let Some(editor) = editor else {
        return false;
    };
    if !editor.focus_handle(cx).is_focused(window) {
        return false;
    }
    let Some(external) = external else {
        return false;
    };
    window.focus(&external);
    true
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
        editor.update(cx, |state, cx| {
            use gpui_component::input::RopeExt as _;
            state.replace_text_in_range(Some(0..old_utf16), &replacement, window, cx);
            let end = state.text().offset_to_position(state.text().len());
            state.set_cursor_position(end, window, cx);
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
#[path = "../../tests/unit/editor/command.rs"]
mod tests;
