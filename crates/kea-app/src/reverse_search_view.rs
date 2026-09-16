//! Native Ctrl-R surface. The search input is separate from the compose editor;
//! this component can replace editor text, but has no execution/PTY capability.
use crate::{
    keybindings::{Action, Invoke},
    reverse_search::{summary, Entry, Hit, InputKind, MAX_RESULTS},
    reverse_search_store::data_directory,
    reverse_search_worker::{Reply, Request, Worker},
};
use gpui::{prelude::*, *};
use gpui_component::{
    input::{Input, InputEvent, InputState},
    ActiveTheme, Disableable as _,
};
use serde::Deserialize;
use std::{sync::mpsc::TryRecvError, time::Duration};

const PAGE_SIZE: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
enum Command { Accept, Cancel, Next, Previous, PageDown, PageUp, Actions, SaveDraft }

#[derive(gpui::Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = kea_memory, no_json)]
struct OverlayAction { command: Command }

pub fn install(cx: &mut App) { cx.bind_keys(bindings()); }

fn bindings() -> Vec<KeyBinding> {
    let mut result = Vec::new();
    for (key, command) in [
        ("enter", Command::Accept), ("escape", Command::Cancel),
        ("down", Command::Next), ("up", Command::Previous),
        ("pagedown", Command::PageDown), ("pageup", Command::PageUp),
        ("ctrl-enter", Command::Actions), ("ctrl-s", Command::SaveDraft),
    ] {
        for context in ["KeaReverseSearch", "KeaReverseSearch > Input"] {
            result.push(KeyBinding::new(key, OverlayAction { command }, Some(context)));
        }
    }
    result
}

#[derive(Clone)]
enum Mode { Results, Actions, Naming(Entry), ConfirmDelete(Vec<String>) }

#[derive(Clone, Copy)]
enum ItemAction { Insert, Copy, SaveGlobal, SaveHere, Rename, MakeGlobal, MakeHere, Forget }

impl ItemAction {
    fn label(self) -> &'static str {
        match self {
            Self::Insert => "Insert into draft (never run)",
            Self::Copy => "Copy exact text",
            Self::SaveGlobal => "Save as global memory…",
            Self::SaveHere => "Save as memory for this directory…",
            Self::Rename => "Rename memory…",
            Self::MakeGlobal => "Make global",
            Self::MakeHere => "Bind to this directory",
            Self::Forget => "Forget…",
        }
    }
}

pub struct ReverseSearchView {
    worker: Option<Worker>,
    target: Option<Entity<InputState>>,
    enabled: bool,
    open: bool,
    original: String,
    directory: Option<String>,
    kind: InputKind,
    query: Entity<InputState>,
    name: Entity<InputState>,
    preview: Entity<InputState>,
    focus: FocusHandle,
    mode: Mode,
    hits: Vec<Hit>,
    selected: usize,
    selected_action: usize,
    generation: u64,
    visible_generation: Option<u64>,
    query_dirty: bool,
    operation_pending: bool,
    query_error: Option<String>,
    warning: Option<String>,
    persistent_history: bool,
    wheel_remainder: f32,
    _query_change: Subscription,
    _pump: Task<()>,
}

impl ReverseSearchView {
    pub fn new(persistent_history: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_worker(Worker::start(data_directory(), persistent_history), persistent_history, window, cx)
    }

    fn with_worker(worker: Result<Worker, String>, persistent_history: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (worker, warning) = match worker { Ok(worker) => (Some(worker), None), Err(error) => (None, Some(error)) };
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search input history and saved memories"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Memory name"));
        let preview = cx.new(|cx| InputState::new(window, cx).code_editor("text").line_number(false).soft_wrap(true));
        let query_change = cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && this.open && matches!(this.mode, Mode::Results) {
                this.invalidate_query();
                cx.notify();
            }
        });
        let pump = cx.spawn_in(window, async move |this, cx| loop {
            Timer::after(Duration::from_millis(30)).await;
            let ended = cx.update(|window, cx| this.update(cx, |this, cx| this.poll(window, cx))).map_or(true, |result| result.is_err());
            if ended { break; }
        });
        Self {
            worker, target: None, enabled: false, open: false, original: String::new(),
            directory: None, kind: InputKind::Application, query, name, preview,
            focus: cx.focus_handle(), mode: Mode::Results, hits: vec![], selected: 0,
            selected_action: 0, generation: 0, visible_generation: None, query_dirty: false,
            operation_pending: false, query_error: None, warning, persistent_history,
            wheel_remainder: 0., _query_change: query_change, _pump: pump,
        }
    }

    /// Host calls this when rendering. A replacement draft or replay transition
    /// invalidates the target, rather than applying a result to a detached editor.
    pub fn set_target(&mut self, target: Entity<InputState>, enabled: bool, cx: &mut Context<Self>) {
        if self.open && (!enabled || self.target.as_ref() != Some(&target)) {
            self.open = false;
            self.visible_generation = None;
            cx.notify();
        }
        self.target = Some(target);
        self.enabled = enabled;
    }

    pub fn warning(&self) -> Option<&str> { self.warning.as_deref() }

    pub fn record(&mut self, text: String, kind: InputKind, directory: Option<String>, cx: &mut Context<Self>) {
        self.request(Request::Record(Entry::submitted(text, kind, directory)), cx);
    }

    pub fn open(&mut self, directory: Option<String>, kind: InputKind, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled { return; }
        if self.open { self.move_selection(1, cx); return; }
        let Some(target) = self.target.clone() else { return; };
        if target.update(cx, |state, cx| state.marked_text_range(window, cx).is_some()) { return; }
        self.original = target.read(cx).value().to_string();
        self.directory = directory;
        self.kind = kind;
        self.mode = Mode::Results;
        self.open = true;
        self.query.update(cx, |state, cx| {
            state.set_value(self.original.clone(), window, cx);
            state.focus(window, cx);
        });
        self.request(Request::Refresh, cx);
        self.invalidate_query();
        cx.notify();
    }

    fn request(&mut self, request: Request, cx: &mut Context<Self>) -> bool {
        let result = self.worker.as_ref().ok_or_else(|| "Input-memory worker is unavailable.".to_string()).and_then(|worker| worker.send(request));
        if let Err(error) = result {
            self.warning = Some(error);
            cx.notify();
            false
        } else { true }
    }

    fn invalidate_query(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.visible_generation = None;
        self.hits.clear();
        self.selected = 0;
        self.query_error = None;
        self.query_dirty = true;
    }

    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for _ in 0..32 {
            let reply = self.worker.as_ref().map(|worker| worker.replies.try_recv());
            match reply {
                Some(Ok(Reply::Results { generation, hits, error })) => {
                    if self.open && generation == self.generation && matches!(self.mode, Mode::Results) {
                        self.hits = hits;
                        self.query_error = error;
                        self.visible_generation = Some(generation);
                        self.selected = 0;
                        cx.notify();
                    }
                }
                Some(Ok(Reply::Saved | Reply::Deleted)) => {
                    self.operation_pending = false;
                    if self.open {
                        self.mode = Mode::Results;
                        self.query.update(cx, |state, cx| state.focus(window, cx));
                        self.invalidate_query();
                    }
                    cx.notify();
                }
                Some(Ok(Reply::OperationFailed(error))) => {
                    self.operation_pending = false;
                    self.query_error = Some(error);
                    cx.notify();
                }
                Some(Ok(Reply::Warning(warning))) => {
                    self.warning = Some(warning);
                    cx.notify();
                }
                Some(Err(TryRecvError::Disconnected)) => {
                    self.worker = None;
                    self.operation_pending = false;
                    self.warning = Some("Input-memory worker stopped; the terminal and draft are unaffected.".into());
                    cx.notify();
                    break;
                }
                _ => break,
            }
        }
        if self.open && self.query_dirty && matches!(self.mode, Mode::Results) {
            let request = Request::Search { generation: self.generation, query: self.query.read(cx).value().to_string(), directory: self.directory.clone() };
            self.query_dirty = !self.request(request, cx) && self.worker.is_some();
        }
    }

    fn close(&mut self, restore_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.visible_generation = None;
        if restore_focus && self.enabled {
            if let Some(target) = &self.target { target.update(cx, |state, cx| state.focus(window, cx)); }
        }
        cx.notify();
    }

    fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let field = if matches!(self.mode, Mode::Naming(_)) { &self.name } else { &self.query };
        field.update(cx, |state, cx| state.marked_text_range(window, cx).is_some())
    }

    fn ready_hit(&self) -> Option<&Hit> {
        (self.visible_generation == Some(self.generation)).then(|| self.hits.get(self.selected)).flatten()
    }

    fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled || self.composing(window, cx) { return; }
        let Some(target) = self.target.clone() else { return; };
        if target.read(cx).value().as_ref() != self.original {
            self.query_error = Some("The draft changed while searching. Close and reopen search; nothing was inserted.".into());
            cx.notify();
            return;
        }
        let length = self.original.encode_utf16().count();
        let inserted = target.update(cx, |state, cx| {
            if state.marked_text_range(window, cx).is_some() { return false; }
            state.replace_text_in_range(Some(0..length), text, window, cx);
            state.focus(window, cx);
            true
        });
        if inserted { self.close(false, window, cx); }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = if matches!(self.mode, Mode::Actions) { self.item_actions().len() }
            else if matches!(self.mode, Mode::Results) { self.hits.len() }
            else { return; };
        let index = if matches!(self.mode, Mode::Actions) { &mut self.selected_action } else { &mut self.selected };
        if count != 0 {
            *index = (*index as isize + delta).rem_euclid(count as isize) as usize;
            cx.notify();
        }
    }

    fn item_actions(&self) -> Vec<ItemAction> {
        let Some(hit) = self.ready_hit() else { return vec![]; };
        let mut actions = vec![ItemAction::Insert, ItemAction::Copy];
        if hit.entry.name.is_some() {
            actions.push(ItemAction::Rename);
            if hit.entry.scope.is_some() { actions.push(ItemAction::MakeGlobal); }
            else if self.directory.is_some() { actions.push(ItemAction::MakeHere); }
        } else {
            actions.push(ItemAction::SaveGlobal);
            if self.directory.is_some() { actions.push(ItemAction::SaveHere); }
        }
        actions.push(ItemAction::Forget);
        actions
    }

    fn show_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hit) = self.ready_hit().cloned() else { return; };
        self.preview.update(cx, |state, cx| state.set_value(hit.entry.text, window, cx));
        self.mode = Mode::Actions;
        self.selected_action = 0;
        window.focus(&self.focus);
        cx.notify();
    }

    fn name_memory(&mut self, mut entry: Entry, scope: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.operation_pending { return; }
        if entry.name.is_none() {
            entry = Entry::submitted(entry.text, entry.kind, entry.directory);
        }
        entry.scope = scope;
        self.name.update(cx, |state, cx| {
            state.set_value(entry.name.clone().unwrap_or_default(), window, cx);
            state.focus(window, cx);
        });
        self.mode = Mode::Naming(entry);
        self.query_error = None;
        cx.notify();
    }

    fn save_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.original.trim().is_empty() { self.query_error = Some("The original draft is empty.".into()); cx.notify(); return; }
        let entry = Entry::submitted(self.original.clone(), self.kind, self.directory.clone());
        self.name_memory(entry, None, window, cx);
    }

    fn item_action(&mut self, action: ItemAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.operation_pending || self.composing(window, cx) { return; }
        let Some(hit) = self.ready_hit().cloned() else { return; };
        match action {
            ItemAction::Insert => self.insert_text(&hit.entry.text, window, cx),
            ItemAction::Copy => cx.write_to_clipboard(ClipboardItem::new_string(hit.entry.text)),
            ItemAction::SaveGlobal => self.name_memory(hit.entry, None, window, cx),
            ItemAction::SaveHere => self.name_memory(hit.entry, self.directory.clone(), window, cx),
            ItemAction::Rename => {
                let scope = hit.entry.scope.clone();
                self.name_memory(hit.entry, scope, window, cx);
            }
            ItemAction::MakeGlobal | ItemAction::MakeHere => {
                let mut entry = hit.entry;
                entry.scope = if matches!(action, ItemAction::MakeHere) { self.directory.clone() } else { None };
                self.operation_pending = self.request(Request::Save(entry), cx);
            }
            ItemAction::Forget => {
                let ids = if hit.entry.name.is_some() { vec![hit.entry.id] } else { hit.occurrence_ids };
                self.mode = Mode::ConfirmDelete(ids);
                window.focus(&self.focus);
            }
        }
        cx.notify();
    }

    fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.operation_pending { return; }
        match self.mode.clone() {
            Mode::Results => {
                if let Some(hit) = self.ready_hit().cloned() { self.insert_text(&hit.entry.text, window, cx); }
            }
            Mode::Actions => {
                if let Some(action) = self.item_actions().get(self.selected_action).copied() { self.item_action(action, window, cx); }
            }
            Mode::Naming(mut entry) => {
                entry.name = Some(self.name.read(cx).value().trim().to_string());
                if let Err(error) = entry.validate() { self.query_error = Some(error); cx.notify(); return; }
                self.operation_pending = self.request(Request::Save(entry), cx);
            }
            Mode::ConfirmDelete(ids) => { self.operation_pending = self.request(Request::Delete(ids), cx); }
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.mode, Mode::Results) { self.close(true, window, cx); }
        else {
            self.mode = Mode::Results;
            self.query_error = None;
            self.query.update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
        }
    }

    fn overlay_action(&mut self, action: &OverlayAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.composing(window, cx) { cx.propagate(); return; }
        match action.command {
            Command::Accept => self.accept(window, cx),
            Command::Cancel => self.cancel(window, cx),
            Command::Next => self.move_selection(1, cx),
            Command::Previous => self.move_selection(-1, cx),
            Command::PageDown => self.move_selection(PAGE_SIZE as isize, cx),
            Command::PageUp => self.move_selection(-(PAGE_SIZE as isize), cx),
            Command::Actions => {
                if matches!(self.mode, Mode::Naming(_) | Mode::ConfirmDelete(_)) { self.accept(window, cx); }
                else if matches!(self.mode, Mode::Results) { self.show_actions(window, cx); }
            }
            Command::SaveDraft => self.save_draft(window, cx),
        }
    }

    fn host_action(&mut self, action: &Invoke, window: &mut Window, cx: &mut Context<Self>) {
        // Even user-remapped submission shortcuts cannot escape the popup and run
        // the compose draft. Ordinary editing shortcuts continue to the host.
        match action.action {
            Action::ReverseSearch if !self.composing(window, cx) => self.move_selection(1, cx),
            Action::FocusEditor => self.close(true, window, cx),
            Action::RunShell | Action::SendApplication | Action::Newline | Action::Complete | Action::Interrupt | Action::ReverseSearch => {}
            _ => cx.propagate(),
        }
    }

    fn chip(&self, id: &'static str, label: &'static str, command: Command, cx: &mut Context<Self>) -> Stateful<Div> {
        div().id(id).px_2().py_1().rounded_md().border_1().border_color(cx.theme().border).cursor_pointer().child(label)
            .on_click(cx.listener(move |this, _, window, cx| this.overlay_action(&OverlayAction { command }, window, cx)))
    }
}

impl Render for ReverseSearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open { return div().into_any_element(); }
        let border = cx.theme().border;
        let mut content = div().flex().flex_col().gap_1();
        match self.mode.clone() {
            Mode::Results => {
                content = content.child(Input::new(&self.query));
                if self.hits.is_empty() {
                    content = content.child(div().p_3().child(if self.worker.is_none() {
                        "Search is unavailable; the draft is unchanged."
                    } else if self.visible_generation.is_none() { "Searching…" }
                    else { "No matches. Try another query, or save the original draft." }));
                }
                let start = self.selected / PAGE_SIZE * PAGE_SIZE;
                let generation = self.generation;
                for (index, hit) in self.hits.iter().enumerate().skip(start).take(PAGE_SIZE) {
                    let title = hit.entry.name.as_ref().map_or_else(|| summary(&hit.entry.text, 100), |name| format!("★ {}", summary(name, 100)));
                    let label = if hit.entry.name.is_some() {
                        format!("{} · {} · {}", hit.entry.kind.label(), if hit.entry.scope.is_some() { "this directory" } else { "global" }, summary(&hit.entry.text, 100))
                    } else {
                        format!("{} · {} retained occurrence(s) · {}", hit.entry.kind.label(), hit.occurrence_ids.len(), if hit.local { "this directory" } else { "other / unknown directory" })
                    };
                    let id = hit.entry.id.clone();
                    let hover_id = id.clone();
                    content = content.child(div().id(("memory-hit", index)).px_2().py_1().rounded_md().cursor_pointer()
                        .when(index == self.selected, |row| row.bg(border))
                        .child(div().child(title)).child(div().text_sm().child(label))
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered && this.generation == generation && this.hits.get(index).is_some_and(|hit| hit.entry.id == hover_id) {
                                this.selected = index; cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.visible_generation == Some(generation) && this.generation == generation && this.hits.get(index).is_some_and(|hit| hit.entry.id == id) {
                                this.selected = index; this.accept(window, cx);
                            }
                        })));
                }
                content = content.child(div().flex().gap_1()
                    .child(self.chip("memory-previous", "↑ Previous", Command::Previous, cx))
                    .child(self.chip("memory-next", "↓ Next", Command::Next, cx))
                    .child(self.chip("memory-actions", "Actions · Ctrl+Enter", Command::Actions, cx)));
                content = content.child(div().text_sm().child(format!(
                    "↑↓ choose · Enter insert · Esc cancel · {} / {}{}",
                    if self.hits.is_empty() { 0 } else { self.selected + 1 }, self.hits.len(),
                    if self.hits.len() == MAX_RESULTS { " (top matches; refine to find more)" } else { "" },
                )));
            }
            Mode::Actions => {
                if let Some(hit) = self.ready_hit() {
                    content = content.child(div().text_sm().child(format!("{} · {} · {}",
                        hit.entry.kind.label(), hit.entry.directory.as_deref().unwrap_or("directory unknown"), age_label(hit.latest_ms))));
                }
                content = content.child(Input::new(&self.preview).h(px(85.)).disabled(true).appearance(false));
                for (index, action) in self.item_actions().into_iter().enumerate() {
                    content = content.child(div().id(("memory-action", index)).px_2().py_1().cursor_pointer()
                        .when(index == self.selected_action, |row| row.bg(border)).child(action.label())
                        .on_hover(cx.listener(move |this, hovered, _, cx| { if *hovered { this.selected_action = index; cx.notify(); } }))
                        .on_click(cx.listener(move |this, _, window, cx| this.item_action(action, window, cx))));
                }
                content = content.child(div().text_sm().child("↑↓ choose action · Enter select · Esc back"));
            }
            Mode::Naming(entry) => {
                content = content.child(div().child(format!("Save memory · {}", entry.scope.as_deref().unwrap_or("global"))))
                    .child(Input::new(&self.name))
                    .child(div().text_sm().child(summary(&entry.text, 160)))
                    .child(div().text_sm().child("Saved text is unencrypted local data. No command will run."))
                    .child(self.chip("memory-save", "Save · Enter", Command::Accept, cx));
            }
            Mode::ConfirmDelete(ids) => {
                content = content.child(div().child(format!("Forget {} retained item(s)?", ids.len())))
                    .child(div().text_sm().child("This removes input-memory records, not shell history, terminal output, recordings, or backups. It is not secure erasure."))
                    .child(self.chip("memory-delete", "Confirm deletion · Enter", Command::Accept, cx));
            }
        }
        if let Some(error) = &self.query_error { content = content.child(div().text_sm().child(error.clone())); }
        if let Some(warning) = &self.warning { content = content.child(div().text_sm().child(warning.clone())); }
        if self.operation_pending { content = content.child(div().text_sm().child("Saving changes…")); }
        let width = px((f32::from(window.viewport_size().width) - 48.).clamp(300., 780.));
        let panel = div().id("reverse-search-overlay").key_context("KeaReverseSearch").track_focus(&self.focus)
            .w(width).p_2().flex().flex_col().gap_2().rounded_md().shadow_lg()
            .bg(cx.theme().background).text_color(cx.theme().foreground).border_1().border_color(border)
            .on_action(cx.listener(Self::overlay_action)).on_action(cx.listener(Self::host_action))
            .on_mouse_down_out(MouseButton::Left, cx.listener(|this, _, window, cx| this.close(false, window, cx)))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                if matches!(this.mode, Mode::Results | Mode::Actions) {
                    let delta = match event.delta { ScrollDelta::Pixels(p) => f32::from(p.y) / 30., ScrollDelta::Lines(p) => p.y };
                    let steps = crate::terminal_mouse::accumulate_wheel_delta(delta, &mut this.wheel_remainder, 5);
                    this.move_selection(-(steps as isize), cx);
                    cx.stop_propagation();
                }
            }))
            .child(div().flex().items_center().gap_2().child(div().font_weight(FontWeight::BOLD).child("History & memories"))
                .child(div().flex_1())
                .child(self.chip("memory-save-draft", "Save draft · Ctrl+S", Command::SaveDraft, cx))
                .child(self.chip("memory-close", "Back / Close · Esc", Command::Cancel, cx)))
            .child(content)
            .child(div().text_sm().child(if self.persistent_history {
                "Submitted-input history persists locally. Leading-space input is excluded."
            } else { "New history is session-only. Saved memories and previously persisted history are loaded." }));
        deferred(anchored().anchor(Corner::BottomLeft).snap_to_window_with_margin(px(12.)).child(panel)).into_any_element()
    }
}

fn age_label(timestamp: u64) -> String {
    let seconds = crate::reverse_search::now_ms().saturating_sub(timestamp) / 1_000;
    match seconds {
        0..=59 => "less than a minute ago".into(),
        60..=3_599 => format!("{} minutes ago", seconds / 60),
        3_600..=86_399 => format!("{} hours ago", seconds / 3_600),
        _ => format!("{} days ago", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{command_editor, settings::Settings};
    use gpui_component::Root;
    struct Empty;
    impl Render for Empty {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { div() }
    }
    #[gpui::test]
    fn search_cancellation_preserves_editor_entity_text_and_selection(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_component::init(cx); command_editor::register_languages(); });
        let window = cx.add_window(|window, cx| { let view = cx.new(|_| Empty); Root::new(view, window, cx) });
        window.update(cx, |_, window, cx| {
            let draft = command_editor::new_draft(None, &Settings::default(), "a😀\noriginal", window, cx);
            draft.update(cx, |state, cx| state.focus(window, cx));
            let before = draft.update(cx, |state, cx| state.selected_text_range(true, window, cx));
            let search = cx.new(|cx| ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx));
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                assert_eq!(search.query.read(cx).value().as_ref(), "a😀\noriginal");
                search.query.update(cx, |state, cx| state.set_value("different query", window, cx));
                search.close(true, window, cx);
            });
            assert_eq!(draft.read(cx).value().as_ref(), "a😀\noriginal");
            let after = draft.update(cx, |state, cx| state.selected_text_range(true, window, cx));
            assert_eq!(before.map(|s| s.range), after.map(|s| s.range));
            assert!(draft.focus_handle(cx).is_focused(window));
        }).unwrap();
    }
    #[gpui::test]
    fn insertion_uses_utf16_editor_replacement_and_refuses_stale_or_composing_drafts(cx: &mut TestAppContext) {
        cx.update(|cx| { gpui_component::init(cx); command_editor::register_languages(); });
        let window = cx.add_window(|window, cx| { let view = cx.new(|_| Empty); Root::new(view, window, cx) });
        window.update(cx, |_, window, cx| {
            let draft = command_editor::new_draft(None, &Settings::default(), "a😀b", window, cx);
            let search = cx.new(|cx| ReverseSearchView::with_worker(Worker::start(None, false), false, window, cx));
            search.update(cx, |search, cx| {
                search.set_target(draft.clone(), true, cx);
                search.open(None, InputKind::Application, window, cx);
                search.insert_text("printf 'ä'\necho done", window, cx);
                assert_eq!(draft.read(cx).value().as_ref(), "printf 'ä'\necho done");
                assert!(!search.open);
                search.open(None, InputKind::Application, window, cx);
                draft.update(cx, |state, cx| state.set_value("external change", window, cx));
                search.insert_text("must not replace", window, cx);
                assert_eq!(draft.read(cx).value().as_ref(), "external change");
                search.close(false, window, cx);
                draft.update(cx, |state, cx| state.replace_and_mark_text_in_range(None, "日本", None, window, cx));
                search.open(None, InputKind::Application, window, cx);
                assert!(!search.open);
            });
        }).unwrap();
    }
    #[test]
    fn overlay_bindings_are_scoped_and_enter_is_not_a_host_execution_action() {
        let map = gpui::Keymap::new(bindings());
        let context = [KeyContext::parse("Kea").unwrap(), KeyContext::parse("KeaCommand").unwrap(), KeyContext::parse("KeaReverseSearch").unwrap(), KeyContext::parse("Input").unwrap()];
        let matches = map.bindings_for_input(&[Keystroke::parse("enter").unwrap()], &context).0;
        assert!(!matches.is_empty());
        let terminal = [KeyContext::parse("Kea").unwrap(), KeyContext::parse("KeaTerminal").unwrap()];
        assert!(map.bindings_for_input(&[Keystroke::parse("enter").unwrap()], &terminal).0.is_empty());
    }
}
