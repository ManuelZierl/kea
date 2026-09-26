use super::KeaView;
use gpui::{prelude::*, *};
use gpui_component::{
    button::Button, dialog::Dialog, switch::Switch, ActiveTheme as _, Selectable as _,
    Sizable as _, WindowExt as _,
};
use kea_app::config::settings::{Appearance, PostSubmitFocus, Settings};

mod keybindings;

struct DialogState {
    show_keybindings: bool,
    keybindings: Entity<keybindings::KeybindingEditor>,
}

#[derive(Clone, Copy)]
pub(super) enum SettingsChange {
    Appearance(Appearance),
    FontSize(Option<f32>),
    PostSubmitFocus(PostSubmitFocus),
    LineNumbers(bool),
    SoftWrap(bool),
    OutputWrap(bool),
    SyntaxHighlighting(bool),
    ShowBlocks(bool),
    PersistHistory(bool),
    HistoryPersistence(bool),
    ShiftMouseSelectsLocally(bool),
    AnimateLogo(bool),
    ConfirmCtrlC(bool),
    ComposerSuggestions(bool),
}

pub(super) fn open(view: Entity<KeaView>, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let keybindings = cx.new(|cx| keybindings::KeybindingEditor::new(window, cx));
    let state = cx.new(|_| DialogState {
        show_keybindings: false,
        keybindings,
    });
    window.open_dialog(cx, move |dialog, window, cx| {
        build_dialog(dialog, &view, &state, window, cx)
    });
}

fn build_dialog(
    dialog: Dialog,
    view: &Entity<KeaView>,
    state: &Entity<DialogState>,
    window: &mut Window,
    cx: &mut App,
) -> Dialog {
    let settings = view.read(cx).settings.clone();
    let viewport = window.viewport_size();
    let width = px((f32::from(viewport.width) - 32.).clamp(320., 720.));
    let path = Settings::path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "Settings path unavailable".into());

    let show_keybindings = state.read(cx).show_keybindings;
    let general_state = state.clone();
    let keybindings_state = state.clone();
    let dialog = dialog
        .title(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().font_weight(FontWeight::BOLD).child("Settings"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("general-settings")
                                .label("General")
                                .small()
                                .selected(!show_keybindings)
                                .on_click(move |_, window, cx| {
                                    general_state
                                        .update(cx, |state, _| state.show_keybindings = false);
                                    window.refresh();
                                }),
                        )
                        .child(
                            Button::new("keybinding-settings")
                                .label("Keybindings")
                                .small()
                                .selected(show_keybindings)
                                .on_click(move |_, window, cx| {
                                    keybindings_state
                                        .update(cx, |state, _| state.show_keybindings = true);
                                    window.refresh();
                                }),
                        ),
                ),
        )
        .w(width)
        .max_w(px(720.))
        .content_id(if show_keybindings {
            "settings-keybindings"
        } else {
            "settings-general"
        })
        // Dialog owns scrolling. Bound the whole dialog, including title and
        // padding, rather than letting a second scroll area escape its bottom.
        // The component's entrance animation adds 30px to margin_top.
        .margin_top(px(0.))
        .h((viewport.height - px(64.)).max(px(200.)))
        .overflow_hidden()
        .overlay_closable(false);
    if show_keybindings {
        return dialog.child(state.read(cx).keybindings.clone());
    }
    dialog.child(
            div()
                .w_full()
                .min_w_0()
                .pb_4()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Changes are saved automatically and applied without replacing the current draft."),
                )
                .child(
                    setting_group(
                        "Appearance",
                        "System follows the operating system. Light and Dark remain fixed until changed here.",
                        vec![
                            choice_row(
                                "Theme",
                                "Choose how Kea's chrome, composer, and settings are drawn.",
                                vec![
                                    choice(
                                        "theme-system",
                                        "System",
                                        settings.appearance == Appearance::System,
                                        SettingsChange::Appearance(Appearance::System),
                                        view,
                                    ),
                                    choice(
                                        "theme-light",
                                        "Light",
                                        settings.appearance == Appearance::Light,
                                        SettingsChange::Appearance(Appearance::Light),
                                        view,
                                    ),
                                    choice(
                                        "theme-dark",
                                        "Dark",
                                        settings.appearance == Appearance::Dark,
                                        SettingsChange::Appearance(Appearance::Dark),
                                        view,
                                    ),
                                ],
                                cx,
                            ),
                            choice_row(
                                "Text size",
                                "Use the component default or an explicit size across the workspace.",
                                vec![
                                    choice(
                                        "font-system",
                                        "System",
                                        settings.font_size.is_none(),
                                        SettingsChange::FontSize(None),
                                        view,
                                    ),
                                    choice(
                                        "font-13",
                                        "13",
                                        settings.font_size == Some(13.),
                                        SettingsChange::FontSize(Some(13.)),
                                        view,
                                    ),
                                    choice(
                                        "font-15",
                                        "15",
                                        settings.font_size == Some(15.),
                                        SettingsChange::FontSize(Some(15.)),
                                        view,
                                    ),
                                    choice(
                                        "font-17",
                                        "17",
                                        settings.font_size == Some(17.),
                                        SettingsChange::FontSize(Some(17.)),
                                        view,
                                    ),
                                ],
                                cx,
                            ),
                            toggle_row(
                                "animate-logo",
                                "Animate composer bird",
                                "Play one short peck cycle after draft changes.",
                                settings.animate_logo,
                                SettingsChange::AnimateLogo,
                                view,
                                cx,
                            ),
                        ],
                        cx,
                    ),
                )
                .child(setting_group(
                    "Composer and output",
                    "Presentation changes preserve the current text, selection, and undo history.",
                    vec![
                        toggle_row(
                            "soft-wrap",
                            "Wrap composer lines",
                            "Wrap long draft lines to the available width.",
                            settings.soft_wrap,
                            SettingsChange::SoftWrap,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "line-numbers",
                            "Show composer line numbers",
                            "Keep multiline drafts easier to scan.",
                            settings.line_numbers,
                            SettingsChange::LineNumbers,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "syntax-highlighting",
                            "Shell syntax highlighting",
                            "Highlight POSIX shell drafts without evaluating them.",
                            settings.syntax_highlighting,
                            SettingsChange::SyntaxHighlighting,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "output-wrap",
                            "Wrap command-block output",
                            "Wrap long lines in the optional read-only block inspector.",
                            settings.output_wrap,
                            SettingsChange::OutputWrap,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "show-blocks",
                            "Show command blocks on startup",
                            "Blocks remain optional observers and never gate execution.",
                            settings.show_blocks,
                            SettingsChange::ShowBlocks,
                            view,
                            cx,
                        ),
                    ],
                    cx,
                ))
                .child(setting_group(
                    "Workflow and privacy",
                    "Submit targets the active receiver; uncertain input requires confirmation.",
                    vec![
                        choice_row(
                            "After successful Submit",
                            "Choose which surface receives the next key.",
                            vec![
                                choice(
                                    "focus-editor",
                                    "Composer",
                                    settings.post_submit_focus == PostSubmitFocus::Editor,
                                    SettingsChange::PostSubmitFocus(PostSubmitFocus::Editor),
                                    view,
                                ),
                                choice(
                                    "focus-terminal",
                                    "Terminal",
                                    settings.post_submit_focus == PostSubmitFocus::Terminal,
                                    SettingsChange::PostSubmitFocus(PostSubmitFocus::Terminal),
                                    view,
                                ),
                            ],
                            cx,
                        ),
                        toggle_row(
                            "confirm-ctrl-c",
                            "Confirm Ctrl-C before forwarding",
                            "Enter sends once; Escape cancels. Copy keeps its normal behavior. Each terminal can override this default.",
                            settings.confirm_ctrl_c,
                            SettingsChange::ConfirmCtrlC,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "composer-suggestions",
                            "Suggest composer actions",
                            "Show non-mutating recommendations for separators, code fences and placeholders. Acceptance is always explicit.",
                            settings.composer_suggestions,
                            SettingsChange::ComposerSuggestions,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "shift-local-selection",
                            "Shift-drag selects terminal text locally",
                            "Turn off to forward Shift pointer gestures to mouse-aware terminal apps.",
                            settings.shift_mouse_selects_locally,
                            SettingsChange::ShiftMouseSelectsLocally,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "persist-history",
                            "Persist submitted drafts after restart",
                            "Takes effect on the next launch. The history is plaintext and may contain secrets.",
                            settings.persist_history,
                            SettingsChange::PersistHistory,
                            view,
                            cx,
                        ),
                        toggle_row(
                            "history-persistence",
                            "Persist Ctrl-R search history after restart",
                            "Takes effect on the next launch. Plaintext; leading-space input is excluded. Named memories are saved separately.",
                            settings.history_persistence,
                            SettingsChange::HistoryPersistence,
                            view,
                            cx,
                        ),
                    ],
                    cx,
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Saved to {path}. Font-family and composer_action_prefix overrides remain available in settings.conf. Use the Keybindings tab for shortcuts.")),
                ),
        )
}

fn setting_group(
    title: &'static str,
    description: &'static str,
    rows: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_3()
        .border_1()
        .border_color(cx.theme().border)
        .rounded_lg()
        .child(div().font_weight(FontWeight::BOLD).child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description),
        )
        .children(rows)
        .into_any_element()
}

fn choice_row(
    title: &'static str,
    description: &'static str,
    choices: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w(px(220.))
                .flex()
                .flex_col()
                .gap_1()
                .child(title)
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                ),
        )
        .child(div().flex().flex_wrap().gap_1().children(choices))
        .into_any_element()
}

fn choice(
    id: &'static str,
    label: &'static str,
    selected: bool,
    change: SettingsChange,
    view: &Entity<KeaView>,
) -> AnyElement {
    let view = view.clone();
    Button::new(id)
        .label(label)
        .small()
        .selected(selected)
        .on_click(move |_, window, cx| {
            view.update(cx, |this, cx| this.apply_setting_change(change, window, cx));
        })
        .into_any_element()
}

fn toggle_row(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    checked: bool,
    change: fn(bool) -> SettingsChange,
    view: &Entity<KeaView>,
    cx: &App,
) -> AnyElement {
    let view = view.clone();
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w(px(220.))
                .flex()
                .flex_col()
                .gap_1()
                .child(title)
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                ),
        )
        .child(
            Switch::new(id)
                .checked(checked)
                .small()
                .on_click(move |checked, window, cx| {
                    view.update(cx, |this, cx| {
                        this.apply_setting_change(change(*checked), window, cx)
                    });
                }),
        )
        .into_any_element()
}
