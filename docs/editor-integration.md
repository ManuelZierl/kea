# Platform-integrated editing


## Product contract

Kea is a minimal editor with executable input and persistent, read-only output. It should not impose terminal conventions on ordinary text editing or duplicate services the platform/editor already supplies.

Reuse platform services and established editor/terminal components. Kea owns execution, session documents and their presentation, not a second text editing engine.

## Implementation

The standalone host uses `gpui-component` 0.5.1 `InputState`/`Input`, on GPUI 0.2.2, for the command editor, read-only command/output text and search fields. `src/editor/command.rs` is a small construction/submission adapter, not a buffer/cursor/undo implementation. Selection, grapheme-aware movement, mouse text interaction, scrolling, undo/redo, composition and clipboard operations are delegated to that component.

The component registers GPUI's platform `EntityInputHandler`. Text arrives through committed/replaced/composing text ranges, not Kea translating key codes into a String. The component provides candidate-window geometry and UTF-16 range conversion for the platform bridge. This is integration with OS input services, **not** a promise that every platform autocomplete, dictation, accessibility or input-method feature works on every desktop. Verify those combinations explicitly. Kea adds no autocorrection or smart-quote substitutions to executable text.

Bash syntax highlighting is provided by Tree-sitter through the editor's language registry. Only the Bash grammar is added; no all-languages bundle is enabled. PowerShell input currently uses the same multiline editor without a PowerShell grammar.

## Focus and actions

Copy, Cut, Paste, Undo, Redo, Select All and Find are semantic actions dispatched to the focused editor. Do not intercept all window key events. Kea installs configurable bindings at the editor context so a remapped/unbound shortcut actually overrides the component's default. Ordinary navigation and text entry remain with the editor and platform. Composer Find reveals its active match even when that match is outside the current editor viewport; revealing a match does not move the draft caret or mutate selection/undo state. The terminal canvas uses Alacritty's selection model for pointer selection, wrapping-aware extraction and visible highlighting.

`copy` copies only the focused selection. An empty selection leaves the clipboard unchanged. `copy_document` explicitly exports the retained command history or visible terminal screen. Each block has its own Copy block action. Terminal selection-copy uses the visible Alacritty viewport and never silently substitutes the entire screen.

Run in shell and Send to app apply only to the focused command editor, not a block, filter, embedded find field or terminal. Run in shell additionally requires an explicit prompt-ready report; Send to app never wraps its text in shell source. While composition is active it does nothing. Enter remains the editor's newline/candidate-confirmation key; no shell command is sent until an explicit execution action succeeds.

A successful submission replaces the draft editor with a fresh entity and fresh undo history. It never mutates a historical command. Edit as new copies a prior command into a fresh draft, requires the existing draft to be empty, and never executes automatically. Undo cannot undo a shell side effect.

The terminal and editor are separated by a draggable platform-component divider. Resizing changes presentation and the measured PTY dimensions; it does not create another session or execution mode.

The simultaneously visible terminal has its own focus/key context and retains the same process. The configurable `focus_editor` escape remains available; other semantic shortcuts are masked. Local terminal selection/caret owns only Copy, Esc and navigation/extension; unrelated input clears local state before forwarding. Platform composition takes precedence. Select text is available in the header or with F4 from composer/chrome. The optional block inspector changes presentation only; see unified-session.md. The Alacritty/canvas/input adapter provides bounded scrollback, simple/block selection and caret movement, plus negotiated wheel and primary-button/drag/motion forwarding. Wider platform acceptance remains separate work.

## Read-only output

Pinned GPUI Component 0.5.1's `Input::disabled(true)` disables text mutations, including platform input replacements, but preserves focus, selection, Copy, navigation, mouse selection and Find. Kea uses it without disabled-looking input decoration. This behavior is a tested dependency contract; recheck it when upgrading the component. Do not substitute a widget that becomes unselectable when read-only.

Editor entities are retained between renders, not recreated each frame. If output changes while a block is focused or has a selection, its displayed snapshot freezes and a New output available button appears. The model and recording continue receiving output. Refresh is explicit and labels that it clears the selection. Scrolling is not unconditionally reset on every output chunk.

Only 24 blocks at a time have editor entities. Older/Newer/Latest navigation and case-insensitive command/output filtering operate over retained document blocks. Long individual blocks scroll inside their component. These are bounded document searches, not an index of every historical TUI screen.

## Defaults and configuration

Appearance defaults to system light/dark and observes appearance changes. The in-app Settings dialog exposes System, Light and Dark choices and applies them live; System keeps observing OS changes while explicit overrides win. Fonts and sizes default to the component's platform-appropriate defaults; `system` in font settings means no Kea override, not that GPUI embeds the OS's native text widget. The compatibility terminal retains its ANSI palette and current fixed metrics.

`settings.conf`, next to `keybindings.conf`, supports:

```text
theme = system
show_blocks = false
font_family = system
font_size = system
syntax_highlighting = true
line_numbers = false
soft_wrap = true
output_wrap = true
shift_mouse_selects_locally = true
animate_logo = true
```

`animate_logo = false` disables the decorative composer typing animation.

Set `KEA_SETTINGS` or `KEA_KEYBINDINGS` for explicit file locations. Invalid files produce a warning and fall back to defaults. The Settings dialog atomically replaces `settings.conf` with a validated complete snapshot and applies presentation/input-policy changes without recreating the draft. Draft-history persistence changes are intentionally restart-scoped. External file edits and keybinding changes are loaded at startup. Do not add a separate Kea preference for every OS preference. Expose overrides only where the host owns policy or where a specific override is useful.

## Validation and scope

The host tests the component's composition/replacement API and Kea's execution guard, UTF-16 ranges, default/overridden/unbound shortcut resolution, and settings validation. The Linux graphical smoke test exercises typing, selection-only copy, cut, undo/redo, multiline Unicode paste, explicit execution, fresh draft history, read-only output and existing replay behavior.

Synthetic component input tests are not real IBus/Fcitx, Wayland, macOS or Windows IME acceptance tests. Keep those, accessibility, OS prediction/dictation coverage, richer shell metadata, complete terminal mouse/keyboard/image protocols, and OS packaging separately tracked. A successful Linux smoke test cannot certify all those capabilities.

Terminal local-selection routing and engine lifecycle have dedicated unit tests;
real candidate-window placement, composition cancellation/commit and selection
control accessibility require the manual checks in
[terminal-text-selection.md](terminal-text-selection.md).

## Zed integration

`kea-core` and `kea-document` remain free of GPUI/editor/OS dependencies. The standalone host uses GPUI Component; a Zed host should use Zed's editor, text services and action system instead. Do not transplant the standalone UI or copy GPL Zed code into Kea's MIT crates.
