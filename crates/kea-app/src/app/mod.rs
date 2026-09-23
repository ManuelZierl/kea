mod actions;
mod composer;
mod document_view;
mod pointer;
mod rendering;
mod settings_window;
mod shell_metadata;
mod startup;
mod tabs;
mod terminal_input;
use tabs::{KeaRoot, WorkspaceEvent};
mod workspace;

use gpui::{prelude::*, *};
use gpui_component::{input::InputState, Root};
use kea_alacritty::TERMINAL_SCROLLBACK_LINES;
use kea_app::{
    config::{
        keybindings::{Action, Invoke, Keymap},
        settings::Settings,
    },
    editor::{command as command_editor, completion, provider::Providers},
    reverse_search::view::ReverseSearchView,
    shell::ShellFlavor,
    terminal::{context::InputContext, input, recovery::PromptLineTracker, selection},
};
use kea_document::Document;
use kea_session::Session;
use shell_metadata::ShellMetadata;
use std::{sync::mpsc::Receiver, time::Instant};

const COMPONENT_LINE_HEIGHT_EM: f32 = 1.25;
const MAX_SCROLL_LINES_PER_EVENT: i32 = TERMINAL_SCROLLBACK_LINES as i32;
const MAX_MOUSE_WHEEL_REPORTS_PER_EVENT: i32 = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InitialFocus {
    Editor,
    Terminal,
}

struct PendingRun {
    text: String,
    editor: Entity<InputState>,
    context_generation: u64,
    released: bool,
}

type CompletionResult = (
    u64,
    String,
    usize,
    Result<Vec<completion::Candidate>, String>,
);

#[derive(Clone)]
struct TerminalFontMetrics {
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
}

struct KeaView {
    visible: bool,
    session: Session,
    document: Document,
    shell_metadata: ShellMetadata,
    input_context: InputContext,
    shell: Option<ShellFlavor>,
    keymap: Keymap,
    settings: Settings,
    show_blocks: bool,
    terminal_bounds: Option<Bounds<Pixels>>,
    terminal_scroll_remainder: f32,
    terminal_mouse_scroll_remainder: f32,
    terminal_gesture: Option<selection::Gesture>,
    // Keep the press-time canvas mapping stable if layout changes mid-gesture.
    terminal_gesture_bounds: Option<Bounds<Pixels>>,
    timeline_track_bounds: Option<Bounds<Pixels>>,
    timeline_hovered: bool,
    prompt_line: PromptLineTracker,
    pending_run: Option<PendingRun>,
    composer_enter_down: bool,
    terminal_composition: Entity<InputState>,
    completion_rx: Option<Receiver<CompletionResult>>,
    completion_generation: u64,
    completion_invalidated: bool,
    candidates: Vec<completion::Candidate>,
    completion_index: usize,
    completion_scroll: ScrollHandle,
    completion_text: String,
    completion_cursor: usize,
    completion_context: u64,
    completion_providers: Providers,
    completion_source: String,
    completion_bounds: Vec<Option<Bounds<Pixels>>>,
    editor: Entity<InputState>,
    reverse_search: Entity<ReverseSearchView>,
    document_ui: document_view::DocumentUi,
    document_scroll: ScrollHandle,
    focus: FocusHandle,
    notice: Option<String>,
    warning: Option<String>,
    composer_logo_gen: usize,
    composer_logo_active: bool,
    composer_logo_again: bool,
    composer_logo_deadline: Option<Instant>,
    logo_warmed_frames: usize,
    _composer_change: Subscription,
    _composer_keys: Subscription,
    _pump: Task<()>,
    _appearance: Subscription,
    _filter_change: Subscription,
    _focus_lost: Subscription,
    _memory_changed: Subscription,
}

fn button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .whitespace_nowrap()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x687380))
        .cursor_pointer()
        .child(label.into())
}

pub(crate) fn run() {
    if let Err(error) = startup::run() {
        eprintln!("kea: {error:#}");
        #[cfg(windows)]
        startup::show_message(format!("Kea could not start:\n\n{error:#}"));
        std::process::exit(1);
    }
}
