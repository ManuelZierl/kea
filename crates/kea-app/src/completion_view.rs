//! Completion is host UI around the existing editor, not a second input buffer.
use super::{command_editor, edit, InputMode, KeaView};
use gpui::{prelude::*, *};
use gpui_component::ActiveTheme;
use kea_app::completion::{self, Request, Suggestion};

pub(super) struct CompletionMenu {
    original: String,
    cursor: usize,
    directory: Option<String>,
    items: Vec<Suggestion>,
    selected: usize,
}
impl KeaView {
    pub(super) fn complete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() { self.accept_completion(window, cx); return; }
        if self.completion_busy || self.input_mode != InputMode::Document { return; }
        let Some(text) = command_editor::submission_text(&self.editor, window, cx) else { return; };
        let Some(shell) = self.shell else { return; };
        let (cursor, selected) = self.editor.update(cx, |state,cx| (state.cursor(), state.selected_text_range(true,window,cx).is_some_and(|s| !s.range.is_empty())));
        if selected || text[..cursor].rsplit('\n').next().unwrap_or("").trim().is_empty() {
            window.dispatch_action(Box::new(edit::IndentInline), cx);
            return;
        }
        let directory = self.document.current_directory().map(str::to_owned);
        let request = Request {
            text: text.clone(), cursor,
            directory: directory.as_ref().map(Into::into), shell,
            history: self.document.blocks().iter().rev().take(64).map(|b| b.input.clone()).collect(),
            path: std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default(),
        };
        let job = cx.background_executor().spawn(async move { completion::suggest(request) });
        self.completion_busy = true;
        self.completion_task = Some(cx.spawn_in(window, async move |this, cx| {
            let items = job.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.completion_busy = false;
                if this.input_mode != InputMode::Document
                    || this.document.current_directory() != directory.as_deref()
                    || this.editor.read(cx).cursor() != cursor
                    || command_editor::submission_text(&this.editor, window, cx).as_deref() != Some(text.as_str()) {
                    cx.notify();
                    return;
                }
                if items.is_empty() {
                    this.notice = Some("No local path, executable, or retained command completion. Shell-specific flags and remote files are not inferred.".into());
                } else {
                    let unique = items.len() == 1;
                    this.completion = Some(CompletionMenu { original: text, cursor, directory, items, selected: 0 });
                    if unique { this.accept_completion(window, cx); }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn accept_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.completion.take() else { return; };
        if self.input_mode != InputMode::Document
            || self.document.current_directory() != menu.directory.as_deref()
            || self.editor.read(cx).cursor() != menu.cursor
            || command_editor::submission_text(&self.editor, window, cx).as_deref() != Some(menu.original.as_str()) { cx.notify(); return; }
        let Some(item) = menu.items.get(menu.selected) else { return; };
        let start = menu.original[..item.range.start].encode_utf16().count();
        let end = menu.original[..item.range.end].encode_utf16().count();
        self.editor.update(cx, |state,cx| state.replace_text_in_range(Some(start..end), &item.replacement, window,cx));
        self.notice = None;
        cx.notify();
    }
    pub(super) fn move_completion(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(menu) = self.completion.as_mut() {
            menu.selected = (menu.selected as isize + delta).rem_euclid(menu.items.len() as isize) as usize;
            cx.notify();
        }
    }
    pub(super) fn clear_stale_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let valid = if let Some(menu) = &self.completion {
            self.input_mode == InputMode::Document
                && self.document.current_directory() == menu.directory.as_deref()
                && self.editor.read(cx).cursor() == menu.cursor
                && command_editor::submission_text(&self.editor, window, cx).as_deref() == Some(menu.original.as_str())
        } else { true };
        if !valid { self.completion = None; }
    }
    pub(super) fn render_completions(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.completion else { return div().into_any_element(); };
        let mut choices = div().h(px(145.)).flex_shrink_0().flex().flex_col().overflow_hidden()
            .child(div().text_color(cx.theme().muted_foreground).child("Completions · Tab/Enter accept · ↑/↓ choose · Esc dismiss"));
        let start = menu.selected / 5 * 5;
        for (index, item) in menu.items.iter().enumerate().skip(start).take(5) {
            choices = choices.child(div().id(("completion",index)).cursor_pointer().px_2()
                .when(index == menu.selected, |item| item.bg(cx.theme().accent))
                .child(item.label.clone())
                .on_click(cx.listener(move |this,_,window,cx| {
                    if let Some(menu) = this.completion.as_mut() { menu.selected = index; }
                    this.accept_completion(window,cx);
                })));
        }
        choices.into_any_element()
    }
}
