use super::*;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{self as edit, Input},
    resizable::{h_resizable, resizable_panel, v_resizable},
    ActiveTheme, Disableable as _, IconName, Selectable as _, Sizable as _, Theme, ThemeMode,
};
use kea_alacritty::{Screen, TerminalPoint};
use kea_app::{
    config::{
        keybindings::{Action, Keymap},
        settings::{Appearance, Settings},
    },
    history::playback,
    terminal::mouse as terminal_mouse,
};

const FALLBACK_CELL_WIDTH_EM: f32 = 0.6;
const TERMINAL_SELECTION_BACKGROUND: u32 = 0x264f78;
const TERMINAL_SELECTION_FOREGROUND: u32 = 0xffffff;

impl Render for KeaView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.clone();
        let enabled = self.session.input_allowed();
        self.reverse_search.update(cx, |search, cx| {
            search.set_target(editor, enabled, cx);
        });
        let duration = self.session.recording().duration();
        let position = self.session.position();
        let fraction = if duration == 0 {
            0.
        } else {
            position as f32 / duration as f32
        };
        let screen = self.session.screen();
        let terminal_display_offset = screen.display_offset;
        let terminal_history_size = screen.history_size;
        let terminal_selection_available = self.session.terminal_has_selection();
        let terminal_local_active = self.session.terminal_local_selection_active();
        let terminal_selection_invalidated = screen.selection_invalidated;
        let terminal_mouse_reporting = self.session.terminal_mouse_reporting();
        let terminal_metrics = terminal_font_metrics(window, cx);
        let text_line_height = f32::from(terminal_metrics.line_height)
            .max(f32::from(cx.theme().mono_font_size) * COMPONENT_LINE_HEIGHT_EM);
        let compact_chrome = uses_compact_chrome(window, cx);
        let narrow_chrome = f32::from(window.viewport_size().width) < 900.;
        let terminal_header_height = px((text_line_height + 8.).max(30.));
        let composer_header_height = px((text_line_height + 10.).max(32.));
        let toolbar_height = px((text_line_height + 12.).max(36.));
        let timeline_height = px((text_line_height + 10.).max(34.));
        let status_height = px((text_line_height + 6.).max(24.));
        let layout_metrics = terminal_metrics.clone();
        let paint_metrics = terminal_metrics;
        let weak = cx.entity().downgrade();
        let terminal = div()
            .id("terminal-surface")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .key_context(if self.session.input_allowed() {
                "KeaTerminal"
            } else {
                "KeaChrome"
            })
            .track_focus(&self.focus)
            .bg(rgb(0x11151a))
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::terminal_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::terminal_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::terminal_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::terminal_mouse_up))
            .on_mouse_move(cx.listener(Self::terminal_mouse_move))
            .on_scroll_wheel(cx.listener(Self::terminal_scroll))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let size = kea_core::Size::new(
                            (f32::from(bounds.size.width) / f32::from(layout_metrics.cell_width))
                                .floor()
                                .clamp(2., 512.) as u16,
                            (f32::from(bounds.size.height) / f32::from(layout_metrics.line_height))
                                .floor()
                                .clamp(1., 256.) as u16,
                        );
                        let _ = weak.update(cx, |this, cx| {
                            let resized = this.terminal_bounds != Some(bounds);
                            this.terminal_bounds = Some(bounds);
                            if resized {
                                let weak = cx.entity().downgrade();
                                window.defer(cx, move |_, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Ok(size) = size {
                                            let result = this.session.resize(size);
                                            this.result(result, cx);
                                        }
                                    });
                                });
                            }
                        });
                    },
                    {
                        let focus = self.focus.clone();
                        let entity = cx.entity();
                        move |bounds, _, window, cx| {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, entity.clone()),
                                cx,
                            );
                            paint_screen(&screen, bounds, &paint_metrics, window, cx);
                        }
                    },
                )
                .size_full(),
            );

        let terminal_mode = if self.session.is_history() {
            "History"
        } else {
            "Live"
        };
        let terminal_context = if self
            .terminal_gesture
            .is_some_and(|gesture| gesture.owner != selection::MouseOwner::Forward)
        {
            "Selecting terminal text locally"
        } else if terminal_selection_invalidated {
            "Selection changed · local caret retained"
        } else if terminal_local_active {
            "Local selection · arrows move · Shift+arrows extend · Esc clears"
        } else if self.session.is_history() {
            "History · select/copy only"
        } else if terminal_mouse_reporting && self.settings.shift_mouse_selects_locally {
            "Keyboard and mouse to app · Shift-drag selects locally"
        } else if terminal_mouse_reporting {
            "Keyboard and mouse to app · Use Select terminal text"
        } else if self.focus.is_focused(window) {
            "Keyboard to app · Drag selects terminal text"
        } else {
            "Click to interact"
        };
        let mut terminal_header = div()
            .h(terminal_header_height)
            .flex_shrink_0()
            .flex()
            .flex_nowrap()
            .items_center()
            .gap_2()
            .px_2()
            .overflow_hidden()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(!compact_chrome, |header| {
                header.child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .flex_shrink_0()
                        .child("Terminal"),
                )
            })
            .child(div().flex_shrink_0().child(terminal_mode))
            .when(!compact_chrome, |header| {
                header.child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(cx.theme().muted_foreground)
                        .child(terminal_context),
                )
            })
            .child(div().flex_1())
            .when(!compact_chrome, |header| {
                header.child(if terminal_display_offset == 0 {
                    div()
                        .flex_shrink_0()
                        .child(format!("{terminal_history_size} lines"))
                } else {
                    div()
                        .flex_shrink_0()
                        .child(format!("{terminal_display_offset} above bottom"))
                })
            })
            .child(if compact_chrome {
                Button::new("select-terminal-text")
                    .icon(IconName::ALargeSmall)
                    .tooltip(if terminal_local_active {
                        "Stop selecting terminal text"
                    } else {
                        "Select terminal text locally"
                    })
                    .ghost()
                    .small()
                    .selected(terminal_local_active)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.select_terminal_text(window, cx)),
                    )
            } else {
                Button::new("select-terminal-text")
                    .label(if terminal_local_active {
                        "Stop selecting"
                    } else {
                        "Select text"
                    })
                    .tooltip("Toggle local terminal text selection")
                    .ghost()
                    .small()
                    .selected(terminal_local_active)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.select_terminal_text(window, cx)),
                    )
            })
            .child(
                Button::new("copy-terminal-selection")
                    .icon(IconName::Copy)
                    .tooltip("Copy terminal selection")
                    .ghost()
                    .small()
                    .disabled(!terminal_selection_available)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_terminal_selection(cx))),
            )
            .child(
                Button::new("paste-terminal")
                    .label("Paste")
                    .tooltip("Paste clipboard into the terminal (Ctrl+Shift+V)")
                    .ghost()
                    .small()
                    .disabled(!self.session.input_allowed())
                    .on_click(cx.listener(|this, _, _, cx| this.paste_terminal(cx))),
            );
        if terminal_display_offset > 0 {
            terminal_header = terminal_header.child(
                Button::new("terminal-bottom")
                    .icon(IconName::ArrowDown)
                    .tooltip("Return to latest output")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.return_terminal_to_bottom(cx))),
            );
        }
        let terminal_panel = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(terminal_header)
            .child(terminal);

        let output = if self.show_blocks {
            let inspector = div()
                .id("block-inspector")
                .size_full()
                .border_l_1()
                .border_color(cx.theme().border)
                .child(self.render_document(window, cx));
            div().flex().flex_1().min_h_0().overflow_hidden().child(
                h_resizable("terminal-block-split")
                    .child(
                        resizable_panel()
                            .size_range(px(300.)..Pixels::MAX)
                            .child(terminal_panel),
                    )
                    .child(
                        resizable_panel()
                            .size(px(420.))
                            .size_range(px(280.)..px(520.))
                            .child(inspector),
                    ),
            )
        } else {
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(terminal_panel)
        };

        if !self.candidates.is_empty() && !self.completion_is_current(window, cx) {
            self.dismiss_completion();
        }
        let completion_visible = !self.candidates.is_empty();

        let mut completions = div()
            .id("completion-list")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(100.))
            .track_scroll(&self.completion_scroll)
            .overflow_y_scroll();
        if completion_visible {
            for (index, candidate) in self.candidates.iter().enumerate() {
                let label: String = candidate.label.chars().take(100).collect();
                let weak = cx.entity().downgrade();
                let generation = self.completion_generation;
                completions = completions.child(
                    div()
                        .id(("completion", index))
                        .relative()
                        .child(
                            canvas(
                                move |bounds, _, cx| {
                                    let _ = weak.update(cx, |this, _| {
                                        if this.completion_generation == generation {
                                            if let Some(slot) =
                                                this.completion_bounds.get_mut(index)
                                            {
                                                *slot = Some(bounds);
                                            }
                                        }
                                    });
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        )
                        .px_2()
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded_sm()
                        .when(index == self.completion_index, |item| {
                            item.bg(cx.theme().primary)
                                .text_color(cx.theme().primary_foreground)
                        })
                        .when(index != self.completion_index, |item| {
                            item.hover(|item| item.bg(cx.theme().accent.opacity(0.35)))
                        })
                        .cursor_pointer()
                        .child(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.apply_completion(index, window, cx)
                        })),
                );
            }
        }

        let status_warning = self
            .session
            .warning
            .clone()
            .or_else(|| self.warning.clone())
            .or_else(|| {
                self.reverse_search
                    .read(cx)
                    .warning()
                    .map(ToOwned::to_owned)
            });
        let status_is_warning = status_warning.is_some();
        let status = status_warning.or_else(|| self.notice.clone()).unwrap_or_else(|| {
            if self.session.is_history() {
                if compact_chrome {
                    return "Return live".into();
                }
                if narrow_chrome {
                    return "Read-only history · Return live to submit.".into();
                }
                let shortcut = self.keymap.label_or(Action::GoLive, "");
                if shortcut.is_empty() {
                    "Read-only history; the live process continues. The composer stays editable. Use Return live to submit.".into()
                } else {
                    format!("Read-only history; the live process continues. The composer stays editable. Return live ({shortcut}) to submit.")
                }
            } else if self.input_context.ready() {
                format!("Input ready · {}", self.input_context.id())
            } else {
                "Input unconfirmed · Submit asks before sending".into()
            }
        });
        let directory = match (
            self.document.directory(),
            self.input_context.local_shell_ready(),
        ) {
            (Some(path), true) => format!("Current shell directory: {path}"),
            (Some(path), false) => format!("Shell directory (last reported): {path}"),
            _ => "Shell directory: not reported".into(),
        };
        let persistence_status = if self.session.persistence_active() {
            let name = self
                .session
                .persistence_path()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "session.kea".into());
            format!("Saving · {name}")
        } else if self.session.persistence_path().is_some() {
            "Saving stopped".into()
        } else {
            "Temporary".into()
        };
        let shell_state = if self.session.is_history() {
            "History"
        } else if self.input_context.ready() {
            "Ready"
        } else {
            "Input unconfirmed"
        };
        let command_panel = div()
            .id("command-editor")
            .key_context("KeaCommand")
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .h(composer_header_height)
                    .flex_shrink_0()
                    .flex()
                    .flex_nowrap()
                    .items_center()
                    .gap_2()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .font_weight(FontWeight::BOLD)
                            .child("Composer"),
                    )
                    .when(!compact_chrome, |header| {
                        header.child(
                            div()
                                .flex_shrink_0()
                                .whitespace_nowrap()
                                .text_color(cx.theme().muted_foreground)
                                .child(shell_state),
                        )
                    })
                    .child(div().flex_1().min_w_0())
                    .child(
                        button(
                            "run-draft",
                            if compact_chrome {
                                "Submit".into()
                            } else {
                                action_label(&self.keymap, "Submit", Action::RunShell)
                            },
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.submit_button(window, cx))),
                    )
                    .child(
                        button(
                            "reverse-search",
                            format!("History · {}", self.keymap.label(Action::ReverseSearch)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_reverse_search(window, cx);
                        })),
                    ),
            )
            .when(self.pending_run.is_some(), |panel| panel.child(
                div().id("submit-confirmation").px_2().py_1().text_sm()
                    .text_color(cx.theme().danger)
                    .child("No text sent. Input state is unconfirmed or nonempty. Enter again (or Submit) sends; any other key cancels.")
            ))
            .child(self.reverse_search.clone())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex_shrink_0().mt(px(4.)).child(self.composer_logo()))
                    .child(
                        Input::new(&self.editor)
                            .flex_1()
                            .h_full()
                            .min_h(px(72.))
                            .appearance(false)
                            .bordered(false),
                    ),
            )
            .child(completions);
        let session_split = div().flex_1().min_h_0().child(
            v_resizable("terminal-command-split")
                .child(resizable_panel().child(output))
                .child(
                    resizable_panel()
                        .size(px(205.))
                        .size_range(px(144.)..px(520.))
                        .child(command_panel),
                ),
        );

        let copy_view_label = if self.show_blocks && !self.session.is_history() {
            "Copy command history"
        } else {
            "Copy visible terminal"
        };
        let focus_terminal_control = if compact_chrome {
            Button::new("focus-terminal")
                .icon(IconName::SquareTerminal)
                .tooltip("Focus terminal")
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, window, _| window.focus(&this.focus)))
                .into_any_element()
        } else {
            button("focus-terminal", "Terminal")
                .on_click(cx.listener(|this, _, window, _| window.focus(&this.focus)))
                .into_any_element()
        };
        let focus_composer_control = if compact_chrome {
            Button::new("focus-input")
                .icon(IconName::PanelBottom)
                .tooltip("Focus composer")
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.focus_editor(window, cx)))
                .into_any_element()
        } else {
            button("focus-input", "Composer")
                .on_click(cx.listener(|this, _, window, cx| this.focus_editor(window, cx)))
                .into_any_element()
        };
        let save_control = if compact_chrome {
            Button::new("save-session")
                .icon(IconName::File)
                .tooltip(if self.session.persistence_active() {
                    "Session is saving locally"
                } else {
                    "Save session locally"
                })
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.save_session(cx)))
                .into_any_element()
        } else {
            button(
                "save-session",
                if self.session.persistence_active() {
                    "Saved"
                } else {
                    "Save session"
                },
            )
            .on_click(cx.listener(|this, _, _, cx| this.save_session(cx)))
            .into_any_element()
        };
        let mut toolbar = div()
            .h(toolbar_height)
            .flex_shrink_0()
            .flex()
            .flex_nowrap()
            .items_center()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(focus_terminal_control)
            .child(focus_composer_control)
            .child(self.icon_control(
                "blocks",
                if self.show_blocks {
                    IconName::PanelRightClose
                } else {
                    IconName::PanelRightOpen
                },
                if self.show_blocks {
                    "Hide command history"
                } else {
                    "Show command history"
                },
                Action::ToggleBlocks,
                cx,
            ))
            .child(self.icon_control(
                "copy-document",
                IconName::Copy,
                copy_view_label,
                Action::CopyDocument,
                cx,
            ))
            .child(div().flex_1())
            .child(
                Button::new("settings")
                    .icon(IconName::Settings)
                    .tooltip("Open settings")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
            .child(save_control);
        if self.session.is_history() {
            toolbar = toolbar
                .child(self.icon_control(
                    "previous",
                    IconName::ArrowLeft,
                    "Previous history event",
                    Action::PreviousEvent,
                    cx,
                ))
                .child(self.icon_control(
                    "next",
                    IconName::ArrowRight,
                    "Next history event",
                    Action::NextEvent,
                    cx,
                ))
                .child(self.control(
                    "play",
                    if self.session.is_playing() {
                        "Pause"
                    } else {
                        "Play"
                    },
                    Action::PlayPause,
                    cx,
                ))
                .child(if compact_chrome {
                    self.icon_control(
                        "live",
                        IconName::ArrowUp,
                        "Return to live terminal",
                        Action::GoLive,
                        cx,
                    )
                    .into_any_element()
                } else {
                    self.control("live", "Return live", Action::GoLive, cx)
                        .into_any_element()
                });
        } else if self.session.recording().events().len() > 1 {
            toolbar = toolbar.child(if compact_chrome {
                Button::new("history")
                    .icon(IconName::GalleryVerticalEnd)
                    .tooltip("Open terminal history")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        let result = this.session.step(-1);
                        window.focus(&this.focus);
                        this.result(result, cx);
                    }))
                    .into_any_element()
            } else {
                button("history", "History")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let result = this.session.step(-1);
                        window.focus(&this.focus);
                        this.result(result, cx);
                    }))
                    .into_any_element()
            });
        }

        let timeline_weak = cx.entity().downgrade();
        let timeline_hovered = self.timeline_hovered;
        let timeline_bar_color = cx.theme().slider_bar;
        let timeline_progress_color = cx.theme().primary;
        let timeline_thumb_color = cx.theme().slider_thumb;
        let timeline_ring_color = cx.theme().ring;
        let timeline_position = format!(
            "{} / {}",
            playback::format_micros(position),
            playback::format_micros(duration)
        );
        let history_timeline = div()
            .h(timeline_height)
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .when(!compact_chrome, |timeline| {
                timeline.child(div().font_weight(FontWeight::BOLD).child("History"))
            })
            .child(
                div()
                    .id("timeline")
                    .flex_1()
                    .h(px(26.))
                    .cursor_pointer()
                    .on_hover(cx.listener(|this, hovered, _, cx| {
                        this.timeline_hovered = *hovered;
                        cx.notify();
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.seek_x(event.position.x, window, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.seek_x(event.position.x, window, cx);
                        }
                    }))
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let bounds = playback_track_bounds(bounds);
                                let _ = timeline_weak.update(cx, |this, _| {
                                    this.timeline_track_bounds = Some(bounds);
                                });
                            },
                            move |bounds, _, window, _| {
                                let track = playback_track_bounds(bounds);
                                window.paint_quad(
                                    fill(track, timeline_bar_color).corner_radii(px(3.)),
                                );
                                if fraction > 0.0 {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                track.origin,
                                                size(
                                                    track.size.width * fraction,
                                                    track.size.height,
                                                ),
                                            ),
                                            timeline_progress_color,
                                        )
                                        .corner_radii(px(3.)),
                                    );
                                }
                                let diameter = if timeline_hovered { 16.0 } else { 14.0 };
                                let radius = diameter / 2.0;
                                let center = track.origin
                                    + point(track.size.width * fraction, track.size.height / 2.0);
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            center - point(px(radius), px(radius)),
                                            size(px(diameter), px(diameter)),
                                        ),
                                        timeline_thumb_color,
                                    )
                                    .corner_radii(px(radius))
                                    .border_widths(px(2.))
                                    .border_color(timeline_ring_color),
                                );
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(timeline_position),
            );

        let status_bar = div()
            .h(status_height)
            .flex_shrink_0()
            .flex()
            .flex_nowrap()
            .items_center()
            .gap_3()
            .px_2()
            .overflow_hidden()
            .border_t_1()
            .border_color(cx.theme().border)
            .when(!compact_chrome, |bar| {
                bar.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(directory),
                )
            })
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .when(status_is_warning, |status| {
                        status.text_color(cx.theme().danger)
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_color(cx.theme().muted_foreground)
                    .child(persistence_status),
            );

        let mut workspace = div()
            .id("workspace")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(session_split);
        if self.session.is_history() {
            workspace = workspace.child(history_timeline);
        }
        workspace = workspace.child(status_bar);

        div()
            .id("kea")
            .size_full()
            .flex()
            .flex_col()
            .key_context("Kea")
            .capture_action(cx.listener(|this, _: &edit::Enter, window, cx| {
                if this.accept_completion(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_up(cx.listener(Self::composer_key_up))
            .capture_action(cx.listener(|this, _: &edit::MoveLeft, window, cx| {
                if this.move_completion(false, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &edit::MoveRight, window, cx| {
                if this.move_completion(true, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &edit::MoveUp, window, cx| {
                if this.move_completion_row(false, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &edit::MoveDown, window, cx| {
                if this.move_completion_row(true, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &edit::Escape, window, cx| {
                if this.completion_is_current(window, cx) {
                    this.dismiss_completion();
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(Self::invoke))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(cx.theme().mono_font_size)
            .font_family(cx.theme().mono_font_family.clone())
            .child(workspace)
    }
}

pub(super) fn action_label(keymap: &Keymap, label: &str, action: Action) -> String {
    let shortcut = keymap.label_or(action, "");
    if shortcut.is_empty() {
        label.into()
    } else {
        format!("{label} · {shortcut}")
    }
}

pub(super) fn uses_compact_chrome(window: &Window, cx: &App) -> bool {
    f32::from(window.viewport_size().width) / f32::from(cx.theme().mono_font_size).max(1.) < 50.
}

pub(super) fn apply_appearance(settings: &Settings, window: &mut Window, cx: &mut App) {
    match settings.appearance {
        Appearance::System => Theme::sync_system_appearance(Some(window), cx),
        Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    let theme = Theme::global_mut(cx);
    if let Some(family) = &settings.font_family {
        theme.mono_font_family = family.clone().into();
    }
    if let Some(size) = settings.font_size {
        theme.mono_font_size = px(size);
        theme.font_size = px(size);
    }
}

pub(super) fn terminal_font_metrics(window: &mut Window, cx: &mut App) -> TerminalFontMetrics {
    let (font_family, font_size) = {
        let theme = cx.theme();
        (theme.mono_font_family.clone(), theme.mono_font_size)
    };
    let mut terminal_font = font(font_family);
    terminal_font.features = FontFeatures::disable_ligatures();
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&terminal_font);
    let cell_width = text_system
        .advance(font_id, font_size, 'M')
        .map(|advance| advance.width)
        .unwrap_or_else(|_| px(f32::from(font_size) * FALLBACK_CELL_WIDTH_EM));
    let line_height = px((f32::from(text_system.ascent(font_id, font_size)).abs()
        + f32::from(text_system.descent(font_id, font_size)).abs())
    .max(1.));
    TerminalFontMetrics {
        font: terminal_font,
        font_size,
        cell_width,
        line_height,
    }
}

pub(super) fn playback_track_bounds(bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let horizontal_inset = 8.0;
    let track_height = 6.0;
    let width = (f32::from(bounds.size.width) - horizontal_inset * 2.0).max(1.0);
    let top = ((f32::from(bounds.size.height) - track_height) / 2.0).max(0.0);
    Bounds::new(
        bounds.origin + point(px(horizontal_inset), px(top)),
        size(px(width), px(track_height)),
    )
}

pub(super) fn terminal_point(
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    metrics: &TerminalFontMetrics,
    terminal_size: kea_core::Size,
) -> Option<TerminalPoint> {
    let bounds = bounds?;
    let cell_width = f32::from(metrics.cell_width);
    let line_height = f32::from(metrics.line_height);
    if cell_width <= 0. || line_height <= 0. {
        return None;
    }
    let column = ((f32::from(position.x) - f32::from(bounds.origin.x)) / cell_width)
        .floor()
        .max(0.) as usize;
    let row = ((f32::from(position.y) - f32::from(bounds.origin.y)) / line_height)
        .floor()
        .max(0.) as usize;
    Some(TerminalPoint {
        row: row.min(usize::from(terminal_size.rows).saturating_sub(1)),
        column: column.min(usize::from(terminal_size.columns).saturating_sub(1)),
    })
}

pub(super) fn terminal_scroll_lines(
    delta: ScrollDelta,
    line_height: Pixels,
    remainder: &mut f32,
) -> i32 {
    terminal_scroll_units(delta, line_height, remainder, MAX_SCROLL_LINES_PER_EVENT)
}

pub(super) fn terminal_scroll_units(
    delta: ScrollDelta,
    line_height: Pixels,
    remainder: &mut f32,
    maximum: i32,
) -> i32 {
    let line_height = f32::from(line_height).max(1.);
    let delta = match delta {
        ScrollDelta::Pixels(delta) => f32::from(delta.y) / line_height,
        ScrollDelta::Lines(delta) => delta.y,
    };
    terminal_mouse::accumulate_wheel_delta(delta, remainder, maximum)
}

fn paint_screen(
    screen: &Screen,
    bounds: Bounds<Pixels>,
    metrics: &TerminalFontMetrics,
    window: &mut Window,
    cx: &mut App,
) {
    let cell_width = f32::from(metrics.cell_width);
    let line_height = f32::from(metrics.line_height);
    for (index, cell) in screen.cells.iter().enumerate() {
        let row = index / usize::from(screen.size.columns);
        let column = index % usize::from(screen.size.columns);
        let origin =
            bounds.origin + point(px(column as f32 * cell_width), px(row as f32 * line_height));
        if origin.x >= bounds.right() || origin.y >= bounds.bottom() {
            continue;
        }
        let background = if cell.selected {
            TERMINAL_SELECTION_BACKGROUND
        } else {
            cell.background
        };
        if background != 0x11151a {
            window.paint_quad(fill(
                Bounds::new(origin, size(metrics.cell_width, metrics.line_height)),
                rgb(background),
            ));
        }
        if cell.spacer || cell.text == " " {
            continue;
        }
        let mut cell_font = metrics.font.clone();
        if cell.bold {
            cell_font.weight = FontWeight::BOLD;
        }
        let run = TextRun {
            len: cell.text.len(),
            font: cell_font,
            color: rgb(if cell.selected {
                TERMINAL_SELECTION_FOREGROUND
            } else {
                cell.foreground
            })
            .into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(
            cell.text.clone().into(),
            metrics.font_size,
            &[run],
            Some(px(if cell.wide {
                cell_width * 2.
            } else {
                cell_width
            })),
        );
        let _ = shaped.paint(origin, metrics.line_height, window, cx);
        if cell.underline {
            window.paint_quad(fill(
                Bounds::new(
                    origin + point(px(0.), px(line_height - 2.)),
                    size(
                        px(if cell.wide {
                            cell_width * 2.
                        } else {
                            cell_width
                        }),
                        px(1.),
                    ),
                ),
                rgb(if cell.selected {
                    TERMINAL_SELECTION_FOREGROUND
                } else {
                    cell.foreground
                }),
            ));
        }
    }
    if let Some((row, column)) = screen.local_caret {
        let origin =
            bounds.origin + point(px(column as f32 * cell_width), px(row as f32 * line_height));
        if origin.x < bounds.right() && origin.y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(origin, size(px(2.), metrics.line_height)),
                rgb(0xffc857),
            ));
        }
    }
    if let Some((row, column)) = screen.cursor {
        let origin = bounds.origin
            + point(
                px(column as f32 * cell_width),
                px(row as f32 * line_height + line_height - 2.),
            );
        if origin.x < bounds.right() && origin.y < bounds.bottom() {
            window.paint_quad(fill(
                Bounds::new(origin, size(metrics.cell_width, px(2.))),
                rgb(0x61afef),
            ));
        }
    }
}
