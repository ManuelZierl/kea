//! Read-only output reuses the component's selection/search/platform input.
//! Only a bounded page has editor entities; the model retains all bounded blocks.
use super::{button, command_editor, KeaView};
use gpui::{prelude::*, *};
use gpui_component::{
    input::{Input, InputState},
    ActiveTheme,
};
use kea_document::status_label;
use std::collections::{HashMap, HashSet};
const PAGE_SIZE: usize = 24;

struct BlockText {
    editor: Entity<InputState>,
    output_len: usize,
    line_count: usize,
    pending: bool,
    follow: bool,
    reveal_pending: bool,
}
pub(super) struct DocumentUi {
    pub filter: Entity<InputState>,
    pub page_start: Option<usize>,
    pub dirty: bool,
    query: String,
    matches: Vec<usize>,
    last_start: usize,
    visible: HashMap<u64, BlockText>,
    collapsed: HashSet<u64>,
}
impl DocumentUi {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            filter: cx.new(|cx| InputState::new(window, cx).placeholder("Find in command blocks…")),
            page_start: None,
            dirty: true,
            query: String::new(),
            matches: Vec::new(),
            last_start: 0,
            visible: HashMap::new(),
            collapsed: HashSet::new(),
        }
    }
}
impl KeaView {
    pub(super) fn render_document(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = self.document_ui.filter.read(cx).value().to_lowercase();
        let at_bottom =
            self.document_scroll.max_offset().height + self.document_scroll.offset().y <= px(4.);
        if self.document_ui.dirty && self.document_ui.page_start.is_none() && at_bottom {
            self.document_scroll.scroll_to_bottom();
        }
        // Pin the existing page before new output/blocks change the latest-page offset.
        if self.document_ui.page_start.is_none()
            && self
                .document_ui
                .visible
                .values()
                .any(|view| view.editor.focus_handle(cx).is_focused(window))
        {
            self.document_ui.page_start = Some(self.document_ui.last_start);
        }
        if self.document_ui.dirty || self.document_ui.query != query {
            self.document_ui.matches = self
                .document
                .blocks()
                .iter()
                .enumerate()
                .filter(|(_, block)| {
                    query.is_empty()
                        || block.input.to_lowercase().contains(&query)
                        || block.plain_output().to_lowercase().contains(&query)
                })
                .map(|(index, _)| index)
                .collect();
            self.document_ui.query = query;
            self.document_ui.dirty = false;
        }
        let total = self.document_ui.matches.len();
        let start = self
            .document_ui
            .page_start
            .unwrap_or_else(|| total.saturating_sub(PAGE_SIZE))
            .min(total.saturating_sub(1));
        self.document_ui.last_start = start;
        let end = (start + PAGE_SIZE).min(total);
        let indices = self.document_ui.matches[start..end].to_vec();
        let ids: HashSet<_> = indices
            .iter()
            .map(|&index| self.document.blocks()[index].id)
            .collect();
        self.document_ui.visible.retain(|id, _| ids.contains(id));
        let mut blocks = div()
            .id("session-document")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.document_scroll)
            .flex()
            .flex_col()
            .gap_3()
            .p_2();
        if total == 0 {
            blocks = blocks.child(div().p_4().text_color(cx.theme().muted_foreground).child(
                if self.document.blocks().is_empty() {
                    "Write a command below. Enter adds a line; the execute action runs it."
                } else {
                    "No retained command/output matches the search."
                },
            ));
        }
        for index in indices {
            let block = &self.document.blocks()[index];
            let id = block.id;
            let collapsed = self.document_ui.collapsed.contains(&id);
            let mut panel = div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .border_1()
                .border_color(cx.theme().border);
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .p_1()
                    .child(
                        div()
                            .flex_1()
                            .child(format!("#{id}   {}", status_label(block))),
                    )
                    .child(
                        button("collapse", if collapsed { "Expand" } else { "Collapse" }).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if !this.document_ui.collapsed.remove(&id) {
                                    this.document_ui.collapsed.insert(id);
                                }
                                cx.notify();
                            }),
                        ),
                    )
                    .child(button("reuse", "Edit as new").on_click(
                        cx.listener(move |this, _, window, cx| this.reuse_block(id, window, cx)),
                    ))
                    .child(button("copy-block", "Copy block").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if let Some(block) = this.document.blocks().iter().find(|b| b.id == id)
                            {
                                cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                    "$ {}\n{}",
                                    block.input,
                                    block.plain_output()
                                )));
                            }
                        },
                    ))),
            );
            if !collapsed {
                if !self.document_ui.visible.contains_key(&id) {
                    let text = format!("$ {}\n{}", block.input, block.plain_output());
                    let line_count = text.lines().count().max(1);
                    let editor = cx.new(|cx| {
                        InputState::new(window, cx)
                            .multi_line(true)
                            .searchable(true)
                            .soft_wrap(self.settings.output_wrap)
                            .default_value(text)
                    });
                    self.document_ui.visible.insert(
                        id,
                        BlockText {
                            editor,
                            output_len: block.output().len(),
                            line_count,
                            pending: false,
                            follow: true,
                            reveal_pending: true,
                        },
                    );
                }
                let view = self.document_ui.visible.get_mut(&id).unwrap();
                if view.output_len != block.output().len() {
                    let reading = !view.follow
                        || view.editor.focus_handle(cx).is_focused(window)
                        || view.editor.update(cx, |state, cx| {
                            state
                                .selected_text_range(true, window, cx)
                                .is_some_and(|selection| !selection.range.is_empty())
                        });
                    if reading {
                        view.pending = true;
                    } else {
                        let text = format!("$ {}\n{}", block.input, block.plain_output());
                        view.line_count = text.lines().count().max(1);
                        view.editor
                            .update(cx, |state, cx| state.set_value(text, window, cx));
                        view.output_len = block.output().len();
                        view.pending = false;
                        view.reveal_pending = true;
                    }
                }
                // Reveal the tail after the component has measured its text. Do
                // not reset an existing reader's selection or retain input focus.
                if view.reveal_pending {
                    view.reveal_pending = false;
                    let weak = cx.entity().downgrade();
                    window.on_next_frame(move |window, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            let Some(view) = this.document_ui.visible.get_mut(&id) else {
                                return;
                            };
                            if !view.follow || view.editor.focus_handle(cx).is_focused(window) {
                                return;
                            }
                            let selected = view.editor.update(cx, |state, cx| {
                                state
                                    .selected_text_range(true, window, cx)
                                    .is_some_and(|s| !s.range.is_empty())
                            });
                            if selected {
                                return;
                            }
                            if let Some(previous_focus) = window.focused(cx) {
                                view.editor.update(cx, |state, cx| {
                                    use gpui_component::input::RopeExt;
                                    let end = state.text().offset_to_position(state.text().len());
                                    state.set_cursor_position(end, window, cx);
                                });
                                window.focus(&previous_focus);
                            }
                            cx.notify();
                        });
                    });
                }
                let weak = cx.entity().downgrade();
                // Observe scrolling in capture phase without consuming it: the
                // editor still owns native wheel/trackpad behavior.
                let scroll_observer = canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let weak = weak.clone();
                        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && bounds.contains(&event.position) {
                                let _ = weak.update(cx, |this, cx| {
                                    if let Some(view) = this.document_ui.visible.get_mut(&id) {
                                        view.follow = false;
                                    }
                                    cx.notify();
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full();
                // In pinned gpui-component 0.5.1 disabled blocks mutations, not selection/search.
                panel = panel.child(
                    div()
                        .relative()
                        .child(
                            Input::new(&view.editor)
                                .disabled(true)
                                .appearance(false)
                                .bordered(false)
                                .h(px((view.line_count as f32 * 22.0 + 22.0).clamp(66.0, 330.0))),
                        )
                        .child(scroll_observer),
                );
                if view.pending || !view.follow {
                    panel = panel.child(
                        button(
                            "refresh-output",
                            "Follow latest output (refreshes snapshot and clears selection)",
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.document_ui.visible.remove(&id);
                                this.focus_active(window, cx);
                                cx.notify();
                            },
                        )),
                    );
                }
            }
            if block.truncated {
                panel = panel.child(
                    div()
                        .p_1()
                        .text_color(cx.theme().danger)
                        .child("Retained output is truncated; this is not a complete transcript."),
                );
            }
            blocks = blocks.child(div().id(("block", id)).flex_shrink_0().child(panel));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .child(div().flex_1().child(Input::new(&self.document_ui.filter)))
                    .child(
                        button("older", "Older").on_click(cx.listener(move |this, _, _, cx| {
                            this.document_ui.page_start = Some(start.saturating_sub(PAGE_SIZE));
                            cx.notify();
                        })),
                    )
                    .child(
                        button("newer", "Newer").on_click(cx.listener(move |this, _, _, cx| {
                            this.document_ui.page_start =
                                Some((start + PAGE_SIZE).min(total.saturating_sub(PAGE_SIZE)));
                            cx.notify();
                        })),
                    )
                    .child(button("latest-blocks", "Latest").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.focus_active(window, cx);
                            this.document_ui.page_start = None;
                            this.document_scroll.scroll_to_bottom();
                            cx.notify();
                        },
                    )))
                    .child(format!(
                        "{}–{end} / {total}",
                        if total == 0 { 0 } else { start + 1 }
                    )),
            )
            .child(blocks)
            .into_any_element()
    }
    fn reuse_block(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.input_allowed() {
            return;
        }
        if !self.editor.read(cx).value().is_empty() {
            self.notice = Some(
                "Your draft is not empty. Clear it before editing a previous command as new."
                    .into(),
            );
            cx.notify();
            return;
        }
        if let Some(block) = self.document.blocks().iter().find(|block| block.id == id) {
            self.editor =
                command_editor::new_draft(self.shell, &self.settings, &block.input, window, cx);
            self.focus_active(window, cx);
            self.notice =
                Some("Previous command copied to a new draft; nothing has been executed.".into());
            cx.notify();
        }
    }
}
