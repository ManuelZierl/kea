//! A read-only, searchable text viewer dialog for copying long terminal text.
//! The text is shown before anything reaches the clipboard; Copy follows the
//! viewer's own selection, and "Copy all" is a separate explicit action.
use super::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::Input,
    ActiveTheme as _, Sizable as _, WindowExt as _,
};

/// Where the text came from. Reconstructed text must say so.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TextSource {
    /// Exact text from an application API (tmux `capture-pane`).
    Exact,
    /// Rebuilt from screen snapshots; may contain marked gaps.
    Reconstructed { gaps: usize },
}

pub(super) fn open(
    title: String,
    source: TextSource,
    text: String,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) {
        return;
    }
    let lines = text.lines().count();
    let viewer = cx.new(|cx| {
        InputState::new(window, cx)
            .code_editor("text")
            .line_number(false)
            .soft_wrap(false)
            .searchable(true)
            .default_value(text.clone())
    });
    viewer.update(cx, |state, cx| state.focus(window, cx));
    let note = match source {
        TextSource::Exact => format!("{lines} lines · exact text · select to copy, or Copy all"),
        TextSource::Reconstructed { gaps: 0 } => format!(
            "{lines} lines · reconstructed from screen snapshots · check before copying"
        ),
        TextSource::Reconstructed { gaps } => format!(
            "{lines} lines · reconstructed from screen snapshots · {gaps} gap(s) could not be aligned and are marked"
        ),
    };
    let viewport = window.viewport_size();
    let width = px((f32::from(viewport.width) - 32.).clamp(320., 1100.));
    let height = px((f32::from(viewport.height) * 0.6).max(160.));
    window.open_dialog(cx, move |dialog, _, cx| {
        let copy_text = text.clone();
        dialog
            .width(width)
            .title(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().font_weight(FontWeight::BOLD).child(title.clone()))
                    .child(
                        div()
                            .text_sm()
                            .text_color(if matches!(source, TextSource::Exact) {
                                cx.theme().muted_foreground
                            } else {
                                cx.theme().warning
                            })
                            .child(note.clone()),
                    ),
            )
            .child(
                div()
                    .id("text-viewer")
                    .h(height)
                    .border_1()
                    .border_color(cx.theme().border)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(cx.theme().mono_font_size)
                    // Disabled blocks mutation, not selection, search or Copy.
                    .child(Input::new(&viewer).disabled(true).h_full()),
            )
            .footer(move |_, _, _, _| {
                let copy_text = copy_text.clone();
                vec![
                    Button::new("text-viewer-copy-all")
                        .label("Copy all")
                        .small()
                        .on_click(move |_, window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                            window.close_dialog(cx);
                        })
                        .into_any_element(),
                    Button::new("text-viewer-close")
                        .label("Close")
                        .ghost()
                        .small()
                        .on_click(|_, window, cx| window.close_dialog(cx))
                        .into_any_element(),
                ]
            })
    });
}
