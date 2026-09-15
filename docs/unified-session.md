# One session, independent surfaces

Kea has one live terminal session and one persistent editor surface. They coexist. There is no Document/PTY mode and no hidden submission-target state.

The optional block inspector is a derived view of observed shell markers. Hiding it does not change input routing, execution, the child process, recording or replay.

## Focus owns physical input

When the editor or a read-only text component is focused, GPUI Component/platform services own ordinary editing: selection, clipboard, undo/redo, pointer interaction and composition.

When the live terminal is focused, Kea masks **all Kea semantic accelerators**, including user overrides. Representable Ctrl combinations, Tab, modified Enter and function keys reach the terminal encoder instead of invoking Kea actions. Composed text uses GPUI's platform text-input handler and is forwarded only after commit.

“Pass all keys to the TUI” has a protocol boundary: terminal programs receive encoded bytes/sequences, not raw physical keyboard events. Classic terminal protocols intentionally collapse some combinations (for example Tab and Ctrl+I); the OS/window manager may reserve others; newer distinctions require extended keyboard protocols. Kea's invariant is to reserve nothing in live-terminal focus and preserve every distinction exposed by the platform + negotiated terminal protocol.

## Explicit submission actions

The editor has independent actions, not modes:

- **Run in shell** (`run_shell`, default Ctrl+Enter) executes the draft in an integrated local shell. It requires that shell to have explicitly reported an idle prompt.
- **Send to app** (`send_application`, default Ctrl+Shift+Enter) sends the draft literally to the current stdin owner and then Enter. It never adds a shell wrapper.
- **Newline** is editor-native Enter by default. It can be explicitly rebound, for example `newline = shift-enter`.
- **Complete** (`complete`, default Tab) edits the draft only; it never executes it.

Terminal/chat-style editor policy is supported without changing the architecture:

```text
run_shell = enter
newline = shift-enter
```

A successful Run/Send creates a fresh draft and focuses the terminal. Undo never changes a completed execution or reverses process side effects. Historical/replay views reject input.

Direct terminal input invalidates Run readiness until another explicit prompt report arrives. Kea may automate recovery only when it observed plain ASCII insertion followed by the exact number of ordinary Backspaces: Ctrl+Enter sends Ctrl+C, waits for a new prompt marker, revalidates the unchanged focused draft, and then uses the normal Run path. It does not restore readiness from that local observation or apply this recovery to arbitrary controls, pastes, Unicode editing, commands or TUIs.

## Blocks are fail-open observers

`Run in shell` does **not** create a queued block before sending. Kea sends the shell driver line first. If the shell subsequently emits valid start/done markers, `kea-document` may derive a block.

Therefore block-retention exhaustion, parsing failure or a missing marker cannot prevent the command from running and cannot leave execution waiting on a block. At worst, optional structure is incomplete. Raw terminal history remains the canonical compatibility substrate.

## Current directory

Integrated interactive local shells receive a prompt hook. The hook preserves existing Bash `PROMPT_COMMAND`, zsh `precmd_functions`, PowerShell profile/prompt behavior, etc., and emits an explicit cwd report every time the local shell reaches its prompt.

Consequences:

- `cd` through **Run in shell** updates cwd.
- `cd` typed directly in the terminal updates cwd on the next prompt.
- Ctrl+C / an interrupted command updates readiness again when the prompt returns.
- prompt strings are never scraped and `$`/`>` are never treated as readiness signals.

While the integrated local shell is idle, the UI says **Current shell directory**. While a TUI, SSH session or other foreground program owns stdin, the shell is not at its prompt, so the UI says **Shell directory (last reported)**. Kea does not claim to know the internal cwd of a remote or arbitrary child application without integration from that application.

Noninteractive `-c`, `-Command` and script launches do not receive prompt hooks.

## Autocomplete

There are two complementary completion paths.

### Terminal completion

Tab with terminal focus goes unchanged to the child. Bash programmable completion, PowerShell completers, OpenCode/REPL/Vim behavior and user profile configuration remain authoritative. Kea does not replace this path.

### Editor completion

When the integrated local shell is idle, its prompt hook also reports the effective `PATH`. Editor Tab uses that together with the reported cwd to suggest:

- retained command-history prefixes;
- executable names from the shell's effective PATH;
- local files/directories relative to the shell cwd.

Completion runs off the UI thread, has bounded directory/PATH scanning, validates that the draft/cursor did not change before applying a result, and performs an ordinary undoable text replacement. It never evaluates draft shell code. Complex quoting, substitutions/globs, flag/argument-specific programmable completion, remote completion and application-specific completion intentionally fall back to the application's native terminal Tab rather than an approximate parser.

## Shell metadata

Recorded document metadata uses bounded OSC 777 markers for prompt/cwd and command start/done boundaries. Effective PATH is live host completion context and uses a separate OSC 778 marker; it is not necessary to reconstruct command blocks from a recording.

Markers are interoperability metadata, not authenticated provenance. Untrusted terminal output can forge them, so they must never become an authorization/security boundary.

## Viewport and scrolling

PTY rows/columns are derived from the actual laid-out terminal canvas instead of guessed toolbar/editor heights. Dragging the terminal/command-draft divider or opening the block inspector therefore remeasures the PTY and cannot silently hide the terminal's last row.

The primary terminal screen retains up to 10,000 visual scrollback lines. Scrolling changes the emulator viewport without pausing the PTY or replay timeline. New output follows normally at the tail; while the reader is above the tail it stays there until **Return to bottom** or new terminal input explicitly returns it. Drag selection is viewport-aware, and selection copy never falls back to copying the whole screen.

Ordinary local selection and scrollback apply when the child has not requested mouse reporting. While reporting is active, vertical wheel input is forwarded with the negotiated legacy, UTF-8 or SGR encoding; Shift+wheel remains local scrollback. Shift+drag explicitly selects locally. Button, drag, motion and the rest of the terminal mouse-protocol family remain compatibility work.

New block views reveal their tail after layout without stealing focus. Focused/selected read-only snapshots are not replaced beneath a reader; explicit refresh remains available.

## Windows

The GUI executable uses the Windows subsystem, avoiding an intentionally allocated companion console. The default PowerShell launch keeps the user's profile enabled (`-NoLogo -NoExit`), so profile-defined prompt/completion behavior remains available. ConPTY remains the process transport.

This does not bypass SmartScreen/application-control policies, and unsigned development builds are not release installers.

## Validation boundary

Compilation/unit tests, graphical acceptance and real OS testing are separate evidence. Important ongoing compatibility targets include Windows + OpenCode, actual macOS/Windows IMEs, Wayland/IBus/Fcitx, terminal mouse-protocol forwarding, extended keyboard protocol negotiation, accessibility and long-session scrollback/selection behavior.
