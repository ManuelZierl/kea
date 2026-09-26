---
title: Architecture
nav_order: 8
---

# Architecture

## Per-tab sessions, independent surfaces

Kea displays a live terminal and a persistent editor together. Focus decides who receives physical input. A block inspector is optional presentation, hidden by default; it is not an execution mode. See [interaction contract](unified-session.md).

The editor/platform owns ordinary text editing, selection, clipboard, undo and composition. Kea owns explicit submission, process/session history, optional metadata and historical inspection. The configurable `focus_editor` escape is the only semantic accelerator reserved in live-terminal focus; visible local text interaction temporarily owns its limited read-only commands.

The same PTY is used by direct terminal input and composer **Submit**. There is
no second shell, submission-target mode, or process-name guessing. An explicit
live `InputContext` models receiver identity, readiness and generation; it is
independent of `kea-document` and its retained blocks.

## Execution before metadata

Submit sends authored text followed by Enter, never an eval wrapper. Ready input
permits immediate send; unknown/nonempty/busy input requires confirmation of the
unchanged draft and context. Confirmation is a UI safeguard, not authenticated
provenance or transactional delivery. Normal input invalidates readiness and
pending work. Interrupt is always explicit.

A successful shell-ready submission can append a typed metadata event. Shell
hooks subsequently report scoped native start/done boundaries. The optional block
observer correlates those reports without gating PTY writes. Missing hooks,
retention exhaustion and malformed metadata degrade to untracked execution.

See [active input and provider contract](active-input.md) and the
[interaction contract](unified-session.md) for the exact policy and limits.

## Crate boundaries

- `kea-core`: std-only ordered output/resize/lifecycle recording, bounded file format and replay interface.
- `kea-document`: depends only on `kea-core`; optional streaming command/output/cwd metadata derived from explicit markers. No UI, shell, PTY or OS dependency.
- `kea-alacritty`: terminal emulation/projection.
- `kea-pty`: Unix PTY / Windows ConPTY transport.
- `kea-session`: live/history emulation, observation tap, application input, playback and optional journal.
- `kea-app`: GPUI surfaces, platform-integrated editor, shell prompt hooks, completion and host actions.

A Zed integration should use Zed's editor/actions/terminal renderer/PTY ownership while reusing the portable recording/document layers. The standalone GPUI UI is replaceable.

### Source and test layout

Each crate's `lib.rs` is its module/export boundary. Implementation lives in
responsibility-named modules rather than being accumulated in the crate root:

- `kea-core`: event types, recording, file format and replay.
- `kea-document`: document model, marker parsing, input encoding and output text.
- `kea-alacritty`: engine lifecycle, screen projection, negotiated modes and selection.
- `kea-session`: session orchestration, observation, presentation filtering and journal.
- `kea-pty`: the small transport boundary remains in `lib.rs`.
- `kea-app`: `config/`, `editor/`, `history/`, `reverse_search/`, `shell/`,
  `terminal/` and `ui/` hold reusable library code. `app/` holds the binary's
  startup, workspace state, actions, composer, pointer routing and views;
  `main.rs` only enters the application.

Unit tests live under each crate's `tests/unit/`, mirroring the source path:
`src/terminal/input.rs` → `tests/unit/terminal/input.rs`, for example. A `mod.rs`
module uses the corresponding directory's `mod.rs` test file. Source modules
mount these files with `#[cfg(test)]` and `#[path]`, retaining access to private
implementation details without making them public just for tests. Cargo does
not discover nested test files as separate integration-test executables.

Public-boundary integration tests remain under `tests/` (currently the real PTY
session test). `kea-app/tests/standalone/` provides the dependency-free
reverse-search harness used by `scripts/test-reverse-search-core.sh`. Run
`cargo test --locked -p kea-app` to include the binary host's tests as well as
the library/component tests; `--lib` intentionally runs only the latter.

## Canonical stream and metadata

Raw terminal output is never replaced by rendered text or fabricated metadata
bytes. Ordered `Submitted` events hold authored shell submissions separately
from output, resize and lifecycle events. The writer emits v2; the reader retains
v1 compatibility. Terminal replay ignores `Submitted`; `kea-document` observes
it. Recordings contain no raw keystroke stream.

The bounded OSC 779 parser owns live input context. Each complete report is a
new generation, even if its fields repeat. Malformed/oversized reports fail closed
to unknown readiness. Normal terminal input invalidates readiness. These live
observations are never used as an authentication boundary.

OSC 777 retains cwd and legacy block compatibility and adds scoped native
start/done. OSC 778 supplies live PATH. The flat block model cannot represent
nested command trees; it observes an outer block without gating inner input.

## Shell integration

Bash uses PS0 plus PROMPT_COMMAND hooks; zsh uses preexec/precmd hooks. PowerShell
preserves the original prompt and PSConsoleHostReadLine, returning the authored
command to the normal execution path. Hooks capture status before housekeeping.
Bash/zsh report integer shell status; PowerShell reports a success/failure flag.
Shells without execution hooks still accept input but may not produce blocks.

The original POSIX shell receives one fail-open bootstrap line. PowerShell startup
uses -NoExit -Command after the profile, avoiding PSReadLine echo/history pollution.
No authored command is submitted through eval or Invoke-Expression. The same
integration can be printed and installed explicitly inside nested/remote shells;
Kea does not install remotely, infer prompt text, or treat SSH specially.

## Text services and key routing

GPUI Component supplies the editable draft, read-only block text and search fields. Platform text-input integration owns composition and replacement ranges. The terminal surface also implements GPUI's text-input handler so committed IME/composed text can reach the child without a handwritten character approximation.

Editor submission keys are semantic/configurable actions. The editor-native default leaves Enter to the editor and binds Submit to Ctrl+Enter. A terminal/chat-style policy can bind Enter to Submit and Shift+Enter to Newline.

With live terminal focus, Kea accelerators other than `focus_editor`—defaults and user overrides—are masked at the deeper terminal key context. `focus_editor` is the single switch key (terminal → composer, and the same chord returns from composer focus). Explicit terminal clipboard paste (Ctrl+Shift+V, Cmd+V on macOS) is handled on the terminal key path rather than as a masked action; plain Ctrl+V stays ordinary child input. A local selection or caret owns Copy, Esc and navigation/extension; unrelated input clears it before normal terminal forwarding. Active IME composition takes precedence. The terminal encoder receives representable key distinctions. OS-reserved combinations and distinctions absent from the terminal protocol cannot be recreated by Kea; modern keyboard-protocol negotiation is a terminal-compatibility concern.

## Completion

Native terminal Tab is unchanged. Composer completion is either an explicitly
configured receiver provider or labelled local cwd/PATH/history suggestions at a
confirmed local prompt. Provider transport is bounded loopback JSON RPC, does not
launch processes or send PTY probes, and never accepts endpoint addresses from
terminal output. Remote file paths are never resolved on the local host.

Requests include context, draft and UTF-8 cursor; replies carry request/context
identity plus validated replacement ranges. The host additionally checks focus,
IME state, draft/cursor equality and context generation. One worker is retained
until drained; there are no retries. The menu uses measured geometry for vertical
navigation and ordered candidates for horizontal/Tab cycling.

This transport is not a bundled native-shell/TUI completion adapter. Native
equivalence requires a cooperating provider with access to the actual receiver's
state. Starting another shell, scraping a screen or silently pasting a draft into
a TUI is not an equivalent implementation.

## Layout and optional blocks

PTY rows/columns come from the actual laid-out terminal canvas, not guessed window offsets. The terminal/command-draft divider is draggable, and each resulting layout remeasures the terminal rather than cropping unseen rows. The optional inspector follows the same rule.

The terminal emulator retains a bounded 10,000-line visual scrollback projection. Scrolling changes only the selected emulator viewport: live PTY output, recording and protocol replies continue, and new output does not force a reader back to the tail. When the child requests mouse reporting, ordinary vertical wheel input uses its negotiated legacy, UTF-8 or SGR encoding; Shift+wheel remains an explicit local-scrollback override. Pointer selection and selection text extraction reuse Alacritty's grid, wrapping and wide-cell semantics rather than implementing a second terminal text model. Whole-view copy remains a separate explicit action.

Block widgets are bounded/paged. Output text is read-only/selectable. Focused or selected live snapshots do not change under the reader; explicit refresh updates them. Block UI actions cannot execute commands or alter recorded history.

Terminal [text selection](terminal-text-selection.md) keeps simple/block ranges,
caret movement and lifecycle reconciliation in `kea-alacritty`, with narrow
`kea-session` passthroughs. The app owns press-latched pointer routing, settings,
focus, actions and feedback. Shift's local mouse override is configurable;
explicit Select text remains available. Valid ranges follow Alacritty scrolling;
invalidation creates a visible recovery caret instead of restoring stale ranges.
No selection state is recorded and historical interaction cannot send PTY input.

## Replay and retention

Live and historical emulators are separate. Live output/protocol replies continue while an older screen is inspected. Historical emulators cannot send input, issue replies, mutate the clipboard, open URLs or change windows. Returning to LIVE changes only the view.

Terminal history is currently bounded to 32 MiB of accounted data/overhead or 100,000 events. Structured retention is bounded separately. Reaching a block/document limit may truncate optional structure but never stops the PTY or command execution.

Disk recording is explicit, create-only, bounded and unencrypted. Backward seeks currently replay from zero; future checkpoints must capture complete parser/emulator state, not only the visible grid.

## Platform validation

Compilation, unit/component tests, graphical acceptance and real-machine validation are distinct evidence. Priority real-system targets include Windows + OpenCode, macOS/Windows IMEs, Wayland/IBus/Fcitx, terminal mouse-protocol forwarding, keyboard protocol negotiation, accessibility, long-session scrollback/selection behavior and packaging.

Window-level ownership and isolation are specified in [Terminal tabs](terminal-tabs.md).

## Composer sections and guarded interrupts

See [Composer sections and actions](composer-workflow.md) for independently
submitted drafts, opt-in Ctrl-C confirmation and explicitly accepted pattern
recommendations. A section is pending input for the existing receiver, not a
separate shell or an execution queue.
