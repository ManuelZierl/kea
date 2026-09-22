//! Thin host adapter over GPUI Component, not another text editor implementation.
//! Selection, grapheme movement, undo, IME and pointer handling belong upstream.
use crate::{
    config::settings::{PostSubmitFocus, Settings},
    editor::history::DraftHistory,
    shell::ShellFlavor,
};
use gpui::{App, AppContext, Entity, EntityInputHandler, Global, Subscription, Window};
use gpui_component::{
    highlighter::{LanguageConfig, LanguageRegistry},
    input::InputState,
};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryDirection {
    Previous,
    Next,
}

#[derive(Default)]
struct DraftRecallGlobal {
    active_tab: u64,
    inactive: HashMap<u64, TabRecall>,
    persisted: DraftHistory,
    persisted_seed: Vec<String>,
    history: DraftHistory,
    current_editor: Option<Entity<InputState>>,
    submission_candidate: Option<(Entity<InputState>, String)>,
    interceptor: Option<Subscription>,
}

impl Global for DraftRecallGlobal {}

#[derive(Default)]
struct TabRecall {
    history: DraftHistory,
    editor: Option<Entity<InputState>>,
}

/// Switch the owned recall context without resetting navigation or scratch text.
/// A failed/armed submission candidate must never cross a tab switch.
pub fn activate_tab_history(id: u64, cx: &mut App) {
    let recall = cx.default_global::<DraftRecallGlobal>();
    if recall.active_tab == id {
        return;
    }
    let old = TabRecall {
        history: std::mem::take(&mut recall.history),
        editor: recall.current_editor.take(),
    };
    // Do not resurrect an empty context after its terminal was closed.
    if old.editor.is_some() || old.history.len() != 0 {
        recall.inactive.insert(recall.active_tab, old);
    }
    let state = recall.inactive.remove(&id).unwrap_or_else(|| {
        let mut history = DraftHistory::default();
        for text in &recall.persisted_seed {
            history.record(text.clone());
        }
        TabRecall {
            history,
            editor: None,
        }
    });
    recall.history = state.history;
    recall.current_editor = state.editor;
    recall.submission_candidate = None;
    recall.active_tab = id;
}

/// Release retained editor entities when a terminal closes.
pub fn forget_tab_history(id: u64, cx: &mut App) {
    let recall = cx.default_global::<DraftRecallGlobal>();
    recall.inactive.remove(&id);
    if recall.active_tab == id {
        recall.history = DraftHistory::default();
        recall.current_editor = None;
        recall.submission_candidate = None;
    }
}

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
        let recall = cx.default_global::<DraftRecallGlobal>();
        recall.history.record(submitted.clone());
        recall.persisted.record(submitted);
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
    // then make the fresh composer active. No child state is
    // guessed: this is only an explicit focus policy.
    if submission_completed && settings.post_submit_focus == PostSubmitFocus::Editor {
        let weak = editor.downgrade();
        window.defer(cx, move |window, cx| {
            let Some(editor) = weak.upgrade() else {
                return;
            };
            let is_current = cx
                .try_global::<DraftRecallGlobal>()
                .and_then(|recall| recall.current_editor.as_ref())
                == Some(&editor);
            if is_current {
                editor.update(cx, |state, cx| state.focus(window, cx));
            }
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
    let recall = cx.default_global::<DraftRecallGlobal>();
    recall.persisted_seed = entries.clone();
    recall.history = DraftHistory::default();
    for entry in &entries {
        recall.history.record(entry.clone());
    }
    recall.persisted.set_persisted_entries(path, entries);
}

/// Take the first write-through failure for display by the owning workspace.
/// Recall remains memory-authoritative after persistence stops.
pub fn take_history_warning(cx: &mut App) -> Option<String> {
    cx.default_global::<DraftRecallGlobal>()
        .persisted
        .take_persistence_warning()
}

/// Retain the keystroke interceptor for as long as the app lives.
pub fn retain_history_interceptor(subscription: Subscription, cx: &mut App) {
    cx.default_global::<DraftRecallGlobal>().interceptor = Some(subscription);
}

/// Navigate exact text that was successfully submitted from the composer. This acts
/// only on the active Kea command editor; terminal/TUI input is untouched. Replacing
/// the draft is a normal undoable editor operation and never sends bytes to the child.
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
