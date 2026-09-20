---
title: Terminal selection
nav_order: 4
---

# Terminal text interaction and local selection

Status: implemented with automated coverage (2026-09-17). Real IME,
assistive-technology, and cross-platform application acceptance remain separate
validation work. The lifecycle clarifications below resolve the design review.

This is the current terminal text-interaction contract. Local selection is
temporary input ownership, not a persistent all-input-suppressed reading mode.

## 1. Outcome

The live terminal is process-first: while Kea has no local text interaction,
representable keyboard and negotiated mouse input belongs to the child.

A non-empty Kea terminal selection is direct, visible evidence that the user
now wants to interact with terminal text. While that selection exists, the
familiar read-only selection commands in this specification act locally.
Everything else returns to process interaction without losing the triggering
input.

This model must not infer reading intent from shell prompt readiness, process
state, cursor position, idle time, alternate-screen use, or rendered colors.

## 2. Ownership model

### 2.1 Child application interaction

This is the default live-terminal state. Kea has no local selection or local
selection caret. Keyboard input follows normal terminal encoding. Mouse input
follows the child's explicitly negotiated mouse-reporting mode.

### 2.2 Kea local text interaction

Kea owns this interaction when either:

- a local pointer gesture has produced a non-empty Kea terminal selection; or
- the user explicitly invokes **Select terminal text**, which creates a visible
  local caret before a range exists; or
- local keyboard navigation collapses a range into a caret, or terminal changes
  invalidate an active range and leave a visible recovery caret.

Pointer ranges and explicit interaction are distinct. A pointer range enables
local keyboard commands but does not itself override child mouse ownership.
Keyboard navigation promotes it to explicit interaction. Explicit interaction
continues through subsequent caret placement, extension, and dragging. The
header control is pressed for either kind of local interaction; invoking it
with any active local interaction clears that interaction rather than replacing
the range. A fresh invocation enters explicit interaction.

The selected range, anchor, head, selection type, and local caret belong to the
terminal projection. They are not child state and are not sent to the PTY.

The local interaction ends when:

- `Esc` clears it;
- an input outside the local command set is forwarded to the child;
- a child-owned pointer press is forwarded; or
- the user toggles **Select terminal text** off.

A non-empty range may remain selected while another Kea surface has focus. An
explicit caret with no range exits when focus leaves the terminal, because an
invisible caret is not sufficient evidence of continuing local intent.

### 2.3 Output, resize, and retention lifecycle

Alacritty owns range validity and content movement. Ordinary output scrolling
preserves retained ranges and the reading viewport. Selected-text changes,
erasure, screen-buffer switches, column reflow that invalidates the range, and
history eviction must not resurrect a range from stale coordinates. A changed
or invalidated range becomes an explicit, visible caret at the viewport's first
cell with `Selection changed · local caret retained` feedback. Copy then leaves
the clipboard unchanged; the next Ctrl+C must not unexpectedly interrupt the
child. Esc or unrelated input exits normally.

Caret-only positions are grid-coordinate anchored, not immutable text identities.
Output, viewport changes, and resize clamp them to a valid visible cell; they
do not force the reader to the tail. Focus loss clears caret-only state. Local
navigation scrolls its active head into view. Selection state is never restored
from recordings or carried to a replacement historical projection.

### 2.4 Child/TUI selection is opaque

A mouse-aware TUI may receive pointer events, maintain its own semantic
selection, and draw a highlight. Kea sees only the resulting terminal cells; it
cannot distinguish a TUI selection from syntax color, focus, or other
application rendering.

Therefore:

- child-owned highlighting never creates a Kea selection;
- Kea never scrapes colors or screen text to detect a child selection;
- with no Kea selection, selection-related keys continue to the child; and
- copying a child-owned semantic selection remains the child's responsibility.

## 3. Mouse routing

### 3.1 Child mouse reporting off

The child has not requested pointer events, so Kea owns terminal text
selection:

| Gesture | Result |
|---|---|
| Left-drag | simple Kea selection |
| Alt+Left-drag | block (column) Kea selection |
| Shift+click with an existing Kea selection | extend from its original anchor |
| Click without a drag | clear the Kea selection and local caret |

Alt changes the selection shape only when a range is created by dragging. An
Alt+click without movement follows the ordinary local click rule.

### 3.2 Child mouse reporting on

The default policy reserves Shift as Kea's explicit terminal-emulator escape:

| Gesture | Result with `shift_mouse_selects_locally = true` |
|---|---|
| Left press/drag/release | child, using its negotiated reporting mode |
| Ctrl+Left press/drag/release | child, with Ctrl encoded where supported |
| Alt+Left press/drag/release | child, with Alt encoded where supported |
| Shift+pointer motion with no button | Kea suppresses it so positioning for a reserved gesture cannot update the child |
| Shift+Left press/drag/release | Kea owns the complete gesture; dragging creates a simple selection |
| Shift+Alt+Left press/drag/release | Kea owns the complete gesture; dragging creates a block selection |

Shift is a documented Kea reservation, not a claim that the child cannot use
Shift-modified mouse input. The setting in section 6 lets users choose complete
forwarding when an application needs that gesture.

Kea must claim the Shift-modified press before it can safely own a later drag.
Therefore a Shift+click without movement is local too: it extends an existing
Kea selection or clears a fresh zero-length selection, and the child receives
no click. Ctrl does not change a Shift-owned gesture. Alt chooses block shape
only if the gesture becomes a drag; a Shift+Alt click extends the existing
selection using its existing type or clears a fresh zero-length selection.

### 3.3 Explicit local selection

While **Select terminal text** is active, left-drag is local even if the child
has requested mouse reporting. Alt+Left-drag creates a block selection. The
action is the modifier-free fallback when Shift+drag is unavailable, disabled,
or difficult to perform.

A local click places the local caret and keeps the explicit interaction active;
Shift+click extends from its anchor. This gives precise endpoints without
requiring a continuous drag. A drag remains the direct range-selection path.
Alt changes a dragged range to block shape but has no effect on a no-motion
click.

The action focuses the terminal and places a visible local caret at the
application cursor when that cursor is visible. Otherwise it uses the first
selectable cell in the visible viewport. It does not create a persistent
read-only mode: input outside the local command set exits local interaction and
is forwarded normally.

The semantic action is named `select_terminal_text`. It has a visible terminal
header button. Its initial keyboard binding is `F4` in `Kea > Input` and
`KeaChrome`, but not `KeaTerminal`; the existing `focus_editor` action remains
the only Kea semantic accelerator reserved while the live terminal owns the
keyboard. From terminal focus, the keyboard-only route is `focus_editor`, then
`select_terminal_text`, which returns focus to the terminal.

### 3.4 Gesture ownership is fixed at press

Ownership is decided on the initial button press and remains fixed through
motion and release:

- a child-owned press cannot become a Kea drag when Shift is pressed later;
- a Kea-owned press cannot start sending motion when Shift is released later;
- a Kea-owned gesture sends no partial press, motion, or release sequence to
  the child; and
- a child-owned gesture retains every representable modifier.

Only ownership is latched: forwarded motion/release uses each event's current
modifiers and the currently negotiated reporting mode. Click-only tracking
does not receive motion. Unreserved hover reports do not clear local
interaction or change the reading viewport. Shift-modified hover is suppressed
while the local override is enabled, as is hover during explicit local
selection; opting out forwards Shift hover with its modifier. A release without
a press owned by this surface is not forwarded.

This prevents stuck button state and mixed host/child side effects.

## 4. Keyboard routing during local text interaction

Only the following read-only text commands are local:

| Input | Local result |
|---|---|
| Platform Copy (`Ctrl+C` on Linux/Windows, `Cmd+C` on macOS) | copy the Kea selection and retain it |
| `Esc` | clear the range/caret and return to child interaction; send nothing |
| Left/Right | collapse toward the logical start/end, then move the local caret on later presses |
| Up/Down | collapse to the active head, then move the local caret by one visual row |
| Shift+arrows | retain/create the anchor and extend the selection by one cell or visual row |
| Home/End | move the local caret to the first/last selectable cell of the visual row |
| Shift+Home/End | extend to the first/last selectable cell of the visual row |
| PageUp/PageDown | move the viewport and local caret by one page |
| Shift+PageUp/PageDown | move one page and extend the selection |

Home/End use the first cell and last occupied cell of the visual row (the first
cell for an empty row), without following logical line wraps. Left/Right collapse
a range without an extra cell step. Up/Down collapse to its active head and move
one row on the same press; Home/End and page commands also act on that head on
the first press. Copy matches the exact platform chord; additional modifiers
fall through to the child.

The local caret is always visibly distinct from the application cursor and is
scrolled into view when local navigation moves it.

Copy with an explicit caret but no range leaves the clipboard unchanged and
shows `No terminal text is selected.` Copy feedback must not claim clipboard
ownership was confirmed when the platform API only confirms that a write was
requested.

All other key or committed-text input follows one fail-open rule:

1. clear the Kea local range/caret;
2. leave local text interaction; and
3. route the original input through the normal terminal path unchanged.

This includes printable and IME-committed text, Enter, Tab, Backspace, paste,
cut, undo/redo, unsupported navigation chords, and application-specific Ctrl or
Alt combinations. The triggering input is never swallowed. After `Esc` clears
a selection, a second `Esc` reaches the child normally.

Active platform IME composition takes precedence over this table. Its candidate
navigation and cancellation keys do not move or clear the terminal selection.
Preedit retains local state; a non-empty commitment clears it and forwards the
committed text once through the platform text-input bridge. Composition bounds
use the visible local caret when present, otherwise the application cursor.

If forwarding fails, the existing terminal-input error path reports the
failure. The local interaction remains ended because the input itself expressed
the user's return to child interaction.

`Ctrl+A`, word movement, search, and editing commands are not local in v1.
Their expected scope or read-only meaning is ambiguous, so adding them requires
a separate explicit decision rather than silently taking more child input.

## 5. Visible feedback

While the terminal is focused, its context line exposes ownership before and
after selection:

| State | Context text |
|---|---|
| Mouse reporting off, no selection | `Keyboard to app · Drag selects terminal text` |
| Mouse reporting on, Shift override enabled | `Keyboard and mouse to app · Shift-drag selects locally` |
| Mouse reporting on, Shift override disabled | `Keyboard and mouse to app · Use Select terminal text` |
| Local drag in progress | `Selecting terminal text locally` |
| Local range/caret active | `Local selection · arrows move · Shift+arrows extend · Esc clears` |

On a Kea-owned mouse press, feedback is immediate: use the local text-selection
pointer, update the context line, and render the selection as motion arrives.
There is no hold delay.

The **Select terminal text** control exposes its state through a text label,
accessible name/value, and visible pressed state. Color, glow, and tooltip text
may reinforce the state but cannot be the only indication or instruction.

Implementation boundary: GPUI Component 0.5.1 supplies the text-labelled button
and selected styling, but the pinned GPUI 0.2.2 stack does not expose a semantic
accessibility name/value API for this control. Assistive-technology exposure is
an outstanding platform integration requirement, not provided by the label or
pressed styling alone. The F4/composer-escape route supplies keyboard entry.

## 6. Configuration

`settings.conf` gains one positive boolean:

```text
shift_mouse_selects_locally = true
```

`true` is the default and retains the conventional terminal-emulator override.

When set to `false` and child mouse reporting is active:

- Shift+press/drag/release is forwarded with the Shift modifier encoded;
- Shift+Alt and Shift+Ctrl combinations are also forwarded;
- Kea does not begin or extend a local selection from those gestures; and
- **Select terminal text** remains available for explicit local selection.

The setting affects only ownership of Shift-modified left-button gestures while
child mouse reporting is active. It does not change:

- local selection when the child has not requested mouse reporting;
- Shift+wheel's existing local-scrollback override;
- keyboard routing;
- copy semantics;
- replay/history behavior; or
- editor or Submit behavior.

Configuration is loaded and validated through the existing settings path.
Invalid values reject the settings file with the existing visible warning and
safe defaults; Kea does not silently guess.

## 7. Decisions and rejected alternatives

### 7.1 Why Shift

Shift is the established local-selection bypass in xterm and several major
terminal emulators. It is not conflict-free, but it has a stronger transferred
meaning than Ctrl or Alt. Ctrl- and Alt-modified mouse gestures have concrete
application uses; for example, tmux defaults Ctrl+drag to creating a pane and
Alt+drag to moving one.

The compatibility decision is therefore explicit: ordinary input belongs to
the child, while Shift+drag is Kea's configurable local-selection escape.

Research references:

- [xterm mouse tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Mouse-Tracking)
- [kitty mouse actions](https://sw.kovidgoyal.net/kitty/conf/#mouse-actions)
- [WezTerm mouse bindings](https://wezterm.org/config/mouse.html)
- [tmux default mouse bindings](https://github.com/tmux/tmux/blob/master/key-bindings.c)

### 7.2 Why not long-press

Long-press does not safely arbitrate a terminal-protocol gesture:

- forwarding the press immediately lets the child act before Kea takes over,
  and the terminal mouse protocol has no semantic cancellation event; while
- buffering the press changes the timing and down/up ordering observed by every
  ordinary TUI click.

A longer threshold makes normal interaction slower and does not prove that the
child has no long-press behavior. Visual feedback can explain a completed
ownership decision but cannot undo a press already delivered to the child.

Long-press also adds timing and press-hold-drag demands. The explicit button
and keyboard route reduce dependence on the modifier gesture. A local click can
place the caret before keyboard extension; this follows the direction of
[W3C guidance on dragging movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html)
without claiming web-content conformance for the desktop terminal host.

### 7.3 Other rejected models

- Do not send one gesture to both Kea and the child; that permits simultaneous
  application side effects and local selection.
- Do not choose routing from shell readiness or other process heuristics.
- Do not infer a TUI selection from rendered highlights.
- Do not automatically enter a persistent all-input-suppressed reading mode.
- Do not use Ctrl+drag or Alt+drag as the universal reporting override; both
  have established child/application uses, and Alt remains Kea's block
  selection modifier after local ownership is established.

## 8. Product and architecture invariants

- Focus still determines whether the terminal or composer receives physical
  input. This feature creates no Document/PTY or submission-target mode.
- Local selection never pauses PTY execution, output, recording, or required
  terminal protocol replies.
- Ordinary new output preserves valid selections and never forces a scrolled-up
  reader to the tail. Invalidated selections follow section 2.3.
- Local selection uses Alacritty's grid, wrapping, wide-cell, and scrollback
  semantics; Kea does not implement a parallel text buffer.
- Copy uses only the Kea selection and never silently falls back to the whole
  screen or document.
- Selection and local caret state are ephemeral. They are not recorded or
  persisted.
- Replay/history remains observation-only and sends no pointer or keyboard
  input to a historical child.
- Missing selection structure fails open to ordinary child interaction; it can
  never queue or block command execution.

## 9. Implementation ownership

- `kea-alacritty` owns simple/block range creation, anchor/head/caret movement,
  viewport conversion, scrolling into view, rendering, and text extraction.
- `kea-session` exposes narrow passthrough operations against the displayed
  engine without exposing Alacritty internals.
- `kea-app` owns input routing, gesture ownership, focus lifecycle, feedback,
  the explicit action/button, and settings integration.
- `kea-core` and `kea-document` remain unchanged.

## 10. Verification

### 10.1 Pure routing tests

Cover mouse-reporting off/on, Shift override true/false, simple/block
modifiers, explicit local selection, and ownership fixed at press. Assert each
decision as `LocalSimple`, `LocalBlock`, or `Forward`, without shell readiness
or application-name inputs. Include Alt+click, Shift+Alt+click, and modifiers
changing after the initial press.

Cover local keyboard routing for Copy, Esc, navigation, extension, and
forward-and-clear. Include Linux/Windows Ctrl+C, macOS Cmd+C, macOS Ctrl+C,
printable input, Enter, Tab, paste, and committed IME text. Assert that the
platform Copy chord follows normal terminal routing when neither a Kea range
nor an explicit local caret exists.

### 10.2 Engine tests

- simple and block selection across ragged and wrapped lines;
- wide cells;
- selection head/caret movement before and after viewport scrolling;
- directional collapse without an extra cell step;
- page movement and scroll-into-view;
- output and resize preserving valid selection state; and
- existing select-scroll behavior remaining intact.

Also cover overwrite, erase, buffer switch, reflow and retention eviction;
assert recovery-caret bounds and Copy routing after invalidation. Cover
caret-only resize/focus loss, both axes of reverse block selection, and
drag → collapse → click → focus away/back → toggle transitions.

### 10.3 Application tests

- the default setting keeps Shift+drag local during reporting;
- the default setting also consumes Shift+click as one complete local gesture;
- the default setting suppresses Shift-modified hover before a local press;
- `shift_mouse_selects_locally = false` forwards Shift bits in Legacy, UTF-8,
  and SGR press/motion/release reports;
- no partial child sequence escapes from a locally owned gesture;
- changing modifiers mid-drag does not change ownership;
- a non-empty Kea selection activates local keyboard commands;
- the platform Copy chord reaches normal terminal routing with no Kea range or
  explicit caret;
- Esc clears locally and a following Esc reaches the child;
- unrelated key/text/paste input clears selection and reaches the child once;
- explicit caret-only state exits when terminal focus leaves;
- Shift+wheel behavior is unchanged; and
- invalid configuration reports the existing warning and uses defaults.

### 10.4 Manual acceptance

Exercise Bash, OpenCode, Vim/Neovim, and tmux with reporting both active and
inactive:

- ordinary TUI click/drag remains immediate;
- Shift+drag shows immediate local feedback and never activates the TUI;
- Shift+click is local by default and reaches the TUI when the override is off;
- disabling the override lets the TUI receive Shift+drag;
- the explicit button still permits local simple and block selection;
- local arrows extend/move only after visible Kea selection intent;
- typing after selection reaches the child on the first key and clears the
  local state;
- Esc clears locally, then reaches the child on the next press;
- streaming output preserves selection and reading position; and
- local selection can be entered through the button and operated by keyboard
  without a continuous drag.

With local selection active, exercise real IME candidate navigation, Esc
cancellation and Unicode commitment. Confirm candidate-window placement and
exactly-once child delivery. Check the selection control with platform assistive
technology; component labels and selected styling alone are not certification.

## 11. Compatibility requirement

The configurable Shift override, selection-aware keyboard routing, block
selection and explicit Select text entry are delivered together. Keep the
explicit entry fallback available whenever the Shift override can be disabled.
