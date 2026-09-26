//! Window-level ownership. Terminal/composer state remains inside each KeaView.
use super::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, TitleBar,
    WindowExt as _,
};
use kea_app::tabs::{TabId, Tabs, MAX_TERMINALS};

mod held_keys;
use held_keys::HeldWorkflowKeys;

pub(super) enum WorkspaceEvent {
    Action(Action),
    SettingsChanged(Settings, settings_window::SettingsChange),
}
impl EventEmitter<WorkspaceEvent> for KeaView {}

struct TerminalTab {
    view: Entity<KeaView>,
    title: String,
    last_focus: InitialFocus,
    _events: Subscription,
    _changes: Subscription,
}

pub(super) struct KeaRoot {
    tabs: Tabs<TerminalTab>,
    keymap: Keymap,
    settings: Settings,
    focus: FocusHandle,
    scroll: ScrollHandle,
    notice: Option<String>,
    close_allowed: bool,
    held_keys: Option<HeldWorkflowKeys>,
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
        let mut root = Self {
            tabs: Tabs::default(),
            keymap,
            settings,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            notice: None,
            close_allowed: false,
            held_keys: None,
        };
        let title = if view.read(cx).session.is_running() {
            "Terminal"
        } else {
            "Recording"
        };
        root.attach(view, title.into(), initial_focus, window, cx);
        root
    }

    fn attach(
        &mut self,
        view: Entity<KeaView>,
        title: String,
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

    fn workflow_key_up(
        &mut self,
        event: &KeyUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let label = match shell {
            Some(ShellFlavor::PowerShell) => "PowerShell",
            Some(ShellFlavor::Posix) => "Shell",
            None => "Terminal",
        };
        self.attach(view, label.into(), focus, window, cx);
        self.notice = None;
        self.focus_selected(window, cx);
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
        let name = tab.title.clone();
        let weak = cx.entity().downgrade();
        let restore = weak.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            let restore = restore.clone();
            dialog.title(format!("Close {name}?"))
                .child("Closing ends this terminal's process and discards its draft and temporary history. Saved recordings remain on disk. Other terminals keep running.")
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
        let weak = cx.entity().downgrade();
        let restore = weak.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let weak = weak.clone();
            let restore = restore.clone();
            dialog.title("Close all Kea terminals?")
                .child("All terminal processes will end. Drafts and temporary session history will be discarded; saved recordings remain on disk.")
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
                            if !this.close_allowed { this.focus_selected(window, cx); }
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
            } else if !view.session.input_allowed() {
                " · history"
            } else {
                ""
            };
            let label = if !context.is_empty() && context != "local" {
                format!("{} · {context}{suffix}", tab.title)
            } else {
                format!("{} {}{suffix}", tab.title, id.0 + 1)
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
        let body = if let Some(tab) = self.tabs.active() {
            div().flex_1().min_h_0().child(tab.view.clone())
        } else {
            div().flex_1().flex().items_center().justify_center().child(
                "No terminals open. Use + or the New terminal shortcut to open a local shell.",
            )
        };
        div()
            .size_full()
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
                    .child(div().font_weight(FontWeight::BOLD).child("Kea"))
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
            .children(self.notice.clone().map(|notice| {
                div()
                    .px_2()
                    .py_1()
                    .text_color(cx.theme().danger)
                    .child(notice)
            }))
            .child(body)
            .children(dialog_layer)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app/tab_keys.rs"]
mod tests;
