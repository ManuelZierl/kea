//! Window-level ownership. Terminal/composer state remains inside each KeaView.
use super::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, TitleBar,
    WindowExt as _,
};
use kea_app::{
    tabs::{TabId, Tabs, MAX_TERMINALS},
    tmux::{self, TmuxSession},
};
use std::{
    sync::mpsc::{self, Sender},
    time::Duration,
};

mod held_keys;
use held_keys::HeldWorkflowKeys;

pub(super) enum WorkspaceEvent {
    Action(Action),
    SettingsChanged(Settings, settings_window::SettingsChange),
}
impl EventEmitter<WorkspaceEvent> for KeaView {}

#[derive(Clone)]
enum TerminalBacking {
    Process,
    Tmux { session_name: String },
}

struct TerminalTab {
    view: Entity<KeaView>,
    title: String,
    backing: TerminalBacking,
    last_focus: InitialFocus,
    _events: Subscription,
    _changes: Subscription,
}

struct TmuxUiResult {
    generation: u64,
    notice: Option<String>,
    sessions: Result<Vec<TmuxSession>, String>,
}

pub(super) struct KeaRoot {
    tabs: Tabs<TerminalTab>,
    keymap: Keymap,
    settings: Settings,
    focus: FocusHandle,
    scroll: ScrollHandle,
    pub(super) notice: Option<String>,
    pub(super) update_notice: Option<String>,
    pub(super) close_allowed: bool,
    held_keys: Option<HeldWorkflowKeys>,
    pub(super) update_available: Option<update::UpdateInfo>,
    pub(super) update_staged: Option<update::StagedUpdate>,
    pub(super) update_rx: Option<Receiver<update::WorkerResult>>,
    pub(super) update_action: Option<updater::UpdateAction>,
    _update_poll: Task<()>,
    tmux_open: bool,
    tmux_loading: bool,
    tmux_notice: Option<String>,
    tmux_sessions: Vec<TmuxSession>,
    tmux_generation: u64,
    tmux_tx: Sender<TmuxUiResult>,
    _tmux_pump: Task<()>,
}

#[derive(Clone)]
struct DraggedTab {
    id: TabId,
    label: String,
}
impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .bg(cx.theme().background)
            .child(self.label.clone())
    }
}

impl KeaRoot {
    pub(super) fn new(view: Entity<KeaView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let keymap = view.read(cx).keymap.clone();
        let settings = view.read(cx).settings.clone();
        let initial_focus = if view.read(cx).focus.is_focused(window) {
            InitialFocus::Terminal
        } else {
            InitialFocus::Editor
        };
        let check_for_updates = settings.check_for_updates;
        let update_poll = cx.spawn_in(window, async move |this, cx| loop {
            Timer::after(std::time::Duration::from_millis(250)).await;
            let disconnected = cx
                .update(|window, cx| {
                    this.update(cx, |this, cx| this.poll_update(window, cx))
                        .is_err()
                })
                .unwrap_or(true);
            if disconnected {
                break;
            }
        });
        let (tmux_tx, tmux_rx) = mpsc::channel::<TmuxUiResult>();
        let tmux_pump = cx.spawn_in(window, async move |this, cx| loop {
            Timer::after(Duration::from_millis(50)).await;
            let disconnected = cx
                .update(|_, cx| {
                    this.update(cx, |this, cx| {
                        let mut changed = false;
                        while let Ok(result) = tmux_rx.try_recv() {
                            if result.generation != this.tmux_generation {
                                continue;
                            }
                            this.tmux_loading = false;
                            match result.sessions {
                                Ok(sessions) => {
                                    this.tmux_sessions = sessions;
                                    this.tmux_notice = result.notice;
                                }
                                Err(error) => {
                                    this.tmux_sessions.clear();
                                    this.tmux_notice = Some(error);
                                }
                            }
                            changed = true;
                        }
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                })
                .unwrap_or(true);
            if disconnected {
                break;
            }
        });
        let mut root = Self {
            tabs: Tabs::default(),
            keymap,
            settings,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            notice: None,
            update_notice: None,
            close_allowed: false,
            held_keys: None,
            update_available: None,
            update_staged: None,
            update_rx: None,
            update_action: None,
            _update_poll: update_poll,
            tmux_open: false,
            tmux_loading: false,
            tmux_notice: None,
            tmux_sessions: Vec::new(),
            tmux_generation: 0,
            tmux_tx,
            _tmux_pump: tmux_pump,
        };
        let title = terminal_title(view.read(cx).shell, view.read(cx).session.is_running());
        root.attach(
            view,
            title.into(),
            TerminalBacking::Process,
            initial_focus,
            window,
            cx,
        );
        if check_for_updates {
            root.begin_update_check(false, cx);
        }
        root
    }

    fn attach(
        &mut self,
        view: Entity<KeaView>,
        title: String,
        backing: TerminalBacking,
        last_focus: InitialFocus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.tabs.next_id();
        let events = cx.subscribe_in(
            &view,
            window,
            move |this, _, event, window, cx| match event {
                WorkspaceEvent::Action(action) if this.tabs.active_id() == Some(id) => {
                    this.invoke_action(*action, window, cx);
                }
                WorkspaceEvent::SettingsChanged(settings, change) => {
                    this.settings = settings.clone();
                    let others: Vec<_> = this
                        .tabs
                        .iter()
                        .filter(|(other, _)| *other != id)
                        .map(|(_, tab)| tab.view.clone())
                        .collect();
                    for other in others {
                        other.update(cx, |view, cx| {
                            view.apply_shared_settings(settings.clone(), *change, window, cx);
                        });
                    }
                    cx.notify();
                }
                _ => {}
            },
        );
        // Keep this subscription with the tab so closing releases the context.
        let changes = cx.observe(&view, |_, _, cx| cx.notify());
        let tab = TerminalTab {
            view,
            title,
            backing,
            last_focus,
            _events: events,
            _changes: changes,
        };
        assert!(
            self.tabs.insert(tab).is_ok(),
            "terminal capacity checked before spawning"
        );
        self.reveal_active();
    }

    fn reveal_active(&self) {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|(id, _)| Some(id) == self.tabs.active_id())
        {
            self.scroll.scroll_to_item(index);
        }
    }

    fn suspend_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.tabs.active_id() else {
            return;
        };
        let tab = self.tabs.get_mut(id).expect("active tab exists");
        tab.last_focus = if tab.view.read(cx).focus.is_focused(window) {
            InitialFocus::Terminal
        } else {
            InitialFocus::Editor
        };
        self.held_keys = Some(tab.view.update(cx, |view, cx| {
            let held = HeldWorkflowKeys::take(view);
            view.visible = false;
            view.cancel_workflow();
            view.pending_run = None;
            view.composer_enter_down = false;
            view.dismiss_completion();
            view.terminal_gesture = None;
            view.terminal_gesture_bounds = None;
            view.terminal_selection_scroll_at = None;
            cx.notify();
            held
        }));
    }

    fn focus_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.tabs.active_id() {
            command_editor::activate_tab_history(id.0, cx);
        }
        if let Some(tab) = self.tabs.active() {
            let held = self.held_keys.take();
            tab.view.update(cx, |view, cx| {
                if let Some(held) = held {
                    held.restore(view);
                }
                view.visible = true;
                // A hidden terminal keeps its last nonzero dimensions. Re-entering
                // layout resizes only this session using its actual canvas.
                view.terminal_bounds = None;
                if tab.last_focus == InitialFocus::Terminal {
                    window.focus(&view.focus);
                } else {
                    view.focus_editor(window, cx);
                }
                cx.notify();
            });
        } else {
            window.focus(&self.focus);
        }
        self.reveal_active();
        cx.notify();
    }

    fn workflow_key_up(&mut self, event: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(held) = &mut self.held_keys {
            held.release(&event.keystroke.key);
        }
        // The tab bar and dialogs are outside KeaView's key-up ancestry. A
        // release there must still retire ownership, or the next press sticks.
        // When KeaView already handled this release, repeating it is harmless.
        if let Some(tab) = self.tabs.active() {
            tab.view.update(cx, |view, cx| {
                view.composer_key_up(event, window, cx);
            });
        }
    }

    fn activate(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) || self.tabs.get(id).is_none() {
            return;
        }
        if self.tabs.active_id() != Some(id) {
            self.suspend_active(window, cx);
            self.tabs.activate(id);
        }
        self.focus_selected(window, cx);
    }

    fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        if self.tabs.is_full() {
            self.notice = Some(format!(
                "At most {MAX_TERMINALS} terminals can be open. Close one before opening another."
            ));
            cx.notify();
            return;
        }
        // Never repeat the startup command (which might be SSH or an arbitrary
        // program), nor reuse its --record path. New tabs are fresh local shells.
        let (session, shell) = match startup::spawn_terminal(Vec::new(), None) {
            Ok(value) => value,
            Err(error) => {
                self.notice = Some(format!("Could not open terminal: {error:#}"));
                cx.notify();
                return;
            }
        };
        self.suspend_active(window, cx);
        command_editor::activate_tab_history(self.tabs.next_id().0, cx);
        let document = Document::from_recording(session.recording());
        let focus = if shell.is_some() {
            InitialFocus::Editor
        } else {
            InitialFocus::Terminal
        };
        let view = cx.new(|cx| {
            KeaView::new(
                session,
                document,
                shell,
                self.keymap.clone(),
                self.settings.clone(),
                focus,
                None,
                window,
                cx,
            )
        });
        let label = terminal_title(shell, true);
        self.attach(
            view,
            label.into(),
            TerminalBacking::Process,
            focus,
            window,
            cx,
        );
        self.notice = None;
        self.focus_selected(window, cx);
    }

    fn next_tmux_generation(&mut self) -> u64 {
        self.tmux_generation = self.tmux_generation.saturating_add(1);
        self.tmux_generation
    }

    fn refresh_tmux(&mut self, cx: &mut Context<Self>) {
        if self.tmux_loading {
            return;
        }
        let generation = self.next_tmux_generation();
        let tx = self.tmux_tx.clone();
        self.tmux_loading = true;
        std::thread::spawn(move || {
            let sessions = tmux::list_sessions()
                .map_err(|error| format!("Could not list local tmux sessions: {error:#}"));
            let _ = tx.send(TmuxUiResult {
                generation,
                notice: None,
                sessions,
            });
        });
        cx.notify();
    }

    fn toggle_tmux_manager(&mut self, cx: &mut Context<Self>) {
        self.tmux_open = !self.tmux_open;
        if self.tmux_open {
            self.refresh_tmux(cx);
        } else {
            cx.notify();
        }
    }

    fn open_tmux_session(
        &mut self,
        tmux_session: TmuxSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        if self.tabs.is_full() {
            self.tmux_notice = Some(format!(
                "At most {MAX_TERMINALS} terminals can be open. Close one before attaching tmux."
            ));
            cx.notify();
            return;
        }
        let command = match tmux::attach_command(&tmux_session) {
            Ok(command) => command,
            Err(error) => {
                self.tmux_notice = Some(format!("Could not attach tmux: {error:#}"));
                cx.notify();
                return;
            }
        };
        // Spawning only starts the guarded tmux client. Its stale-session
        // branch reports an error and exits; Kea must not infer a successful
        // persistent attach from spawn returning.
        let (session, shell) = match startup::spawn_terminal(command, None) {
            Ok(value) => value,
            Err(error) => {
                self.tmux_notice = Some(format!(
                    "Could not attach tmux session {}: {error:#}",
                    tmux_session.name()
                ));
                cx.notify();
                return;
            }
        };

        self.suspend_active(window, cx);
        command_editor::activate_tab_history(self.tabs.next_id().0, cx);
        let document = Document::from_recording(session.recording());
        let focus = InitialFocus::Terminal;
        let view = cx.new(|cx| {
            KeaView::new(
                session,
                document,
                shell,
                self.keymap.clone(),
                self.settings.clone(),
                focus,
                None,
                window,
                cx,
            )
        });
        let title = format!("tmux · {}", tmux_session.name());
        let backing = TerminalBacking::Tmux {
            session_name: tmux_session.name().into(),
        };
        self.attach(view, title, backing, focus, window, cx);
        self.tmux_open = false;
        self.tmux_notice = None;
        self.focus_selected(window, cx);
    }

    fn kill_tmux_session(&mut self, tmux_session: TmuxSession, cx: &mut Context<Self>) {
        if self.tmux_loading {
            return;
        }
        let generation = self.next_tmux_generation();
        let tx = self.tmux_tx.clone();
        let name = tmux_session.name().to_string();
        self.tmux_loading = true;
        std::thread::spawn(move || {
            let (notice, sessions) = match tmux::kill_session(&tmux_session) {
                Ok(()) => match tmux::list_sessions() {
                    Ok(sessions) => (
                        Some(format!("Killed tmux session {name}.")),
                        Ok(sessions),
                    ),
                    Err(error) => (
                        None,
                        Err(format!(
                            "Killed tmux session {name}, but could not refresh the session list: {error:#}"
                        )),
                    ),
                },
                Err(error) => (
                    None,
                    Err(format!("Could not kill tmux session {name}: {error:#}")),
                ),
            };
            let _ = tx.send(TmuxUiResult {
                generation,
                notice,
                sessions,
            });
        });
        cx.notify();
    }

    fn request_kill_tmux(
        &mut self,
        tmux_session: TmuxSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let name = tmux_session.name().to_string();
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            let tmux_session = tmux_session.clone();
            dialog
                .title(format!("Kill tmux session {name}?"))
                .child(
                    "This ends the persistent tmux session for every attached client. Closing a Kea tmux tab only detaches Kea and does not do this.",
                )
                .confirm()
                .on_ok(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.kill_tmux_session(tmux_session.clone(), cx);
                    });
                    true
                })
        });
    }

    fn needs_confirmation(tab: &TerminalTab, cx: &App) -> bool {
        let view = tab.view.read(cx);
        view.session.is_running()
            || view.has_pending_sections(cx)
            || (!view.session.recording().events().is_empty() && !view.session.persistence_active())
    }

    fn close_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if !Self::needs_confirmation(tab, cx) {
            self.remove_tab(id, window, cx);
            return;
        }
        let (title, message) = match &tab.backing {
            TerminalBacking::Process => (
                format!("Close {}?", tab.title),
                "Closing ends this terminal's process and discards its draft and temporary history. Saved recordings remain on disk. Other terminals keep running.".to_string(),
            ),
            TerminalBacking::Tmux { session_name } => (
                format!("Detach from tmux session {session_name}?"),
                format!(
                    "Closing ends only this Kea tmux client. The persistent tmux session {session_name} keeps running and can be attached again from the tmux manager."
                ),
            ),
        };
        let weak = cx.entity().downgrade();
        let restore = weak.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            let restore = restore.clone();
            dialog
                .title(title.clone())
                .child(message.clone())
                .confirm()
                .on_ok(move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.remove_tab(id, window, cx));
                    true
                })
                .on_close(move |_, window, cx| {
                    let restore = restore.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = restore.update(cx, |this, cx| this.focus_selected(window, cx));
                    });
                })
        });
    }

    fn remove_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.tabs.active_id() == Some(id);
        if selected {
            self.suspend_active(window, cx);
        }
        // Drop the view's pump/subscriptions and PTY, not merely its UI element.
        if self.tabs.remove(id).is_some() {
            command_editor::forget_tab_history(id.0, cx);
            if selected {
                self.focus_selected(window, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.close_allowed
            || !self
                .tabs
                .iter()
                .any(|(_, tab)| Self::needs_confirmation(tab, cx))
        {
            return true;
        }
        self.request_close_window(window, cx);
        false
    }

    fn request_close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        if !self
            .tabs
            .iter()
            .any(|(_, tab)| Self::needs_confirmation(tab, cx))
        {
            self.close_allowed = true;
            window.remove_window();
            return;
        }
        let has_tmux = self
            .tabs
            .iter()
            .any(|(_, tab)| matches!(&tab.backing, TerminalBacking::Tmux { .. }));
        let message = if has_tmux {
            "All Kea terminal client processes will end and drafts/temporary history will be discarded. Attached tmux sessions keep running; kill them explicitly from the tmux manager if that is what you intend."
        } else {
            "All terminal processes will end. Drafts and temporary session history will be discarded; saved recordings remain on disk."
        };
        let weak = cx.entity().downgrade();
        let restore = weak.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            let restore = restore.clone();
            dialog
                .title("Close all Kea terminals?")
                .child(message)
                .confirm()
                .on_ok(move |_, window, cx| {
                    let _ = weak.update(cx, |this, _| this.close_allowed = true);
                    window.defer(cx, |window, _| window.remove_window());
                    true
                })
                .on_close(move |_, window, cx| {
                    let restore = restore.clone();
                    window.defer(cx, move |window, cx| {
                        let _ = restore.update(cx, |this, cx| {
                            if !this.close_allowed {
                                this.focus_selected(window, cx);
                            }
                        });
                    });
                })
        });
    }

    fn invoke_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        match action {
            Action::NewTerminal => self.new_terminal(window, cx),
            Action::CloseTerminal => {
                if let Some(id) = self.tabs.active_id() {
                    self.close_tab(id, window, cx);
                }
            }
            Action::NextTerminal | Action::PreviousTerminal => {
                if let Some(id) = self.tabs.adjacent(action == Action::NextTerminal) {
                    self.activate(id, window, cx);
                }
            }
            Action::MoveTerminalLeft | Action::MoveTerminalRight => {
                self.tabs.move_active(action == Action::MoveTerminalRight);
                self.reveal_active();
                cx.notify();
            }
            Action::Quit => self.request_close_window(window, cx),
            _ => {}
        }
    }
}

impl Render for KeaRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !window.is_window_active() {
            self.held_keys = None;
        }
        let active = self.tabs.active_id();
        let mut strip = div()
            .id("terminal-tabs")
            .flex()
            .items_center()
            .h(px(34.))
            .min_w_0()
            .flex_1()
            .overflow_x_scroll()
            .track_scroll(&self.scroll);
        for (id, tab) in self.tabs.iter() {
            let view = tab.view.read(cx);
            // Context ids are explicit receiver reports, not guessed host names.
            let context = view.input_context.id();
            let suffix = if !view.session.is_running() {
                " · ended"
            } else if view.session.is_frozen() {
                " · frozen"
            } else if !view.session.input_allowed() {
                " · history"
            } else {
                ""
            };
            let label = if !context.is_empty() && context != "local" {
                format!("{} · {context}{suffix}", tab.title)
            } else if matches!(&tab.backing, TerminalBacking::Process) {
                format!("{} {}{suffix}", tab.title, id.0 + 1)
            } else {
                format!("{}{suffix}", tab.title)
            };
            let drag = DraggedTab {
                id,
                label: label.clone(),
            };
            strip = strip.child(
                div()
                    .id(("terminal-tab", id.0))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .h_full()
                    .px_1()
                    .border_b_2()
                    .border_color(if active == Some(id) {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                        this.tabs.reorder(dragged.id, id);
                        this.reveal_active();
                        cx.notify();
                    }))
                    .child(
                        Button::new(("activate-terminal", id.0))
                            .label(label)
                            .ghost()
                            .small()
                            .selected(active == Some(id))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.activate(id, window, cx)
                            })),
                    )
                    .child(
                        Button::new(("close-terminal", id.0))
                            .icon(IconName::Close)
                            .tooltip("Close terminal")
                            .ghost()
                            .small()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab(id, window, cx)
                            })),
                    ),
            );
        }
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let update_control = update::supported().then(|| {
            let (label, tooltip, disabled, action) = match self.update_action {
                Some(updater::UpdateAction::Checking { .. }) => (
                    "Checking…".to_string(),
                    "Checking GitHub Releases for a newer Kea version".to_string(),
                    true,
                    0_u8,
                ),
                Some(updater::UpdateAction::Downloading) => (
                    "Updating…".to_string(),
                    "Downloading and verifying the Kea update".to_string(),
                    true,
                    0_u8,
                ),
                Some(updater::UpdateAction::Verifying) => (
                    "Verifying…".to_string(),
                    "Verifying the staged Kea update before restart".to_string(),
                    true,
                    0_u8,
                ),
                None if self.update_staged.is_some() => {
                    let staged = self.update_staged.as_ref().expect("checked above");
                    (
                        format!("Restart for v{}", staged.version),
                        format!("Restart Kea and install v{}", staged.version),
                        false,
                        2_u8,
                    )
                }
                None => match &self.update_available {
                    Some(info) => (
                        format!("Update v{}", info.version),
                        format!("Download and verify Kea v{}", info.version),
                        false,
                        1_u8,
                    ),
                    None => (
                        "Updates".to_string(),
                        "Check GitHub Releases for a newer Kea version".to_string(),
                        false,
                        0_u8,
                    ),
                },
            };
            Button::new("updates")
                .label(label)
                .tooltip(tooltip)
                .ghost()
                .small()
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, window, cx| match action {
                    1 => this.begin_update_download(cx),
                    2 => this.request_update_restart(window, cx),
                    _ => this.begin_update_check(true, cx),
                }))
        });
        let body = if let Some(tab) = self.tabs.active() {
            div().flex_1().min_h_0().child(tab.view.clone())
        } else {
            div().flex_1().flex().items_center().justify_center().child(
                "No terminals open. Use + or the New terminal shortcut to open a local shell.",
            )
        };

        let tmux_manager = self.tmux_open.then(|| {
            let mut panel = div()
                .id("tmux-manager")
                // This floating panel covers the live terminal. Keep covered
                // pointer and wheel events in the manager instead of letting
                // them reach the terminal behind it; child buttons still
                // receive their own click events before this boundary.
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                }))
                .on_scroll_wheel(cx.listener(|_, _, _, cx| {
                    cx.stop_propagation();
                }))
                .absolute()
                .top(px(72.))
                .right(px(8.))
                .w(px(430.))
                .max_h(px(420.))
                .overflow_y_scroll()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .font_weight(FontWeight::BOLD)
                                .child("Local tmux sessions"),
                        )
                        .child(
                            Button::new("refresh-tmux")
                                .label("Refresh")
                                .ghost()
                                .small()
                                .disabled(self.tmux_loading)
                                .on_click(cx.listener(|this, _, _, cx| this.refresh_tmux(cx))),
                        )
                        .child(
                            Button::new("close-tmux-manager")
                                .icon(IconName::Close)
                                .tooltip("Close tmux manager")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.tmux_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("tmux is the source of truth. Closing a Kea tab detaches only Kea; Kill ends the persistent session for all clients."),
                );

            if let Some(notice) = &self.tmux_notice {
                panel = panel.child(
                    div()
                        .text_color(cx.theme().danger)
                        .child(notice.clone()),
                );
            }
            if self.tmux_loading {
                panel = panel.child(
                    div()
                        .py_2()
                        .text_color(cx.theme().muted_foreground)
                        .child("Refreshing local tmux sessions…"),
                );
            } else if self.tmux_sessions.is_empty() && self.tmux_notice.is_none() {
                panel = panel.child(
                    div()
                        .py_2()
                        .text_color(cx.theme().muted_foreground)
                        .child("No local tmux sessions are running."),
                );
            } else {
                for (index, tmux_session) in self.tmux_sessions.clone().into_iter().enumerate() {
                    let open_session = tmux_session.clone();
                    let kill_session = tmux_session.clone();
                    let window_label = if tmux_session.windows() == 1 {
                        "window"
                    } else {
                        "windows"
                    };
                    let attached_label = if tmux_session.attached_clients() == 1 {
                        "client"
                    } else {
                        "clients"
                    };
                    panel = panel.child(
                        div()
                            .id(("tmux-session", index))
                            .flex()
                            .items_center()
                            .gap_2()
                            .py_2()
                            .px_2()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::BOLD)
                                            .child(tmux_session.name().to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{} {window_label} · {} attached {attached_label}",
                                                tmux_session.windows(),
                                                tmux_session.attached_clients()
                                            )),
                                    ),
                            )
                            .child(
                                Button::new(("open-tmux-session", index))
                                    .label("Open")
                                    .ghost()
                                    .small()
                                    .disabled(self.tabs.is_full())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_tmux_session(open_session.clone(), window, cx);
                                    })),
                            )
                            .child(
                                Button::new(("kill-tmux-session", index))
                                    .label("Kill")
                                    .ghost()
                                    .small()
                                    .disabled(self.tmux_loading)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.request_kill_tmux(kill_session.clone(), window, cx);
                                    })),
                            ),
                    );
                }
            }
            panel
        });
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .key_context("KeaChrome")
            // A focusable ancestor would blur the composer on a suggestion's
            // mouse-down, dismissing its popup before the click can be accepted.
            // Only the empty workspace needs its own keyboard focus target.
            .when(self.tabs.is_empty(), |root| root.track_focus(&self.focus))
            .on_action(cx.listener(|this, action: &Invoke, window, cx| {
                this.invoke_action(action.action, window, cx)
            }))
            .on_key_up(cx.listener(Self::workflow_key_up))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().font_weight(FontWeight::BOLD).child("Kea"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                            )
                            .children(update_control),
                    )
                    .on_close_window(
                        cx.listener(|this, _, window, cx| this.request_close_window(window, cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(strip)
                    .child(
                        Button::new("tmux-manager-button")
                            .label("tmux")
                            .tooltip("Manage local tmux sessions")
                            .ghost()
                            .small()
                            .selected(self.tmux_open)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_tmux_manager(cx))),
                    )
                    .child(
                        Button::new("new-terminal")
                            .icon(IconName::Plus)
                            .ghost()
                            .small()
                            .tooltip(format!(
                                "New local terminal · {}",
                                self.keymap.label(Action::NewTerminal)
                            ))
                            .disabled(self.tabs.is_full())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.new_terminal(window, cx)),
                            ),
                    ),
            )
            .children(self.update_notice.clone().map(|notice| {
                div()
                    .px_2()
                    .py_1()
                    .text_color(cx.theme().muted_foreground)
                    .child(notice)
            }))
            .children(self.notice.clone().map(|notice| {
                div()
                    .px_2()
                    .py_1()
                    .text_color(cx.theme().danger)
                    .child(notice)
            }))
            .child(body)
            .children(tmux_manager)
            .children(dialog_layer)
    }
}

fn terminal_title(shell: Option<ShellFlavor>, running: bool) -> &'static str {
    match (running, shell) {
        (false, _) => "Recording",
        (true, Some(ShellFlavor::PowerShell)) => "PowerShell",
        (true, Some(ShellFlavor::Posix)) => "Shell",
        (true, None) => "Terminal",
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app/tab_keys.rs"]
mod tests;
