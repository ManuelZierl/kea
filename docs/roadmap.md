# Roadmap and acceptance checks

## Product direction

Kea is a persistent terminal workspace built around a normal editor. The terminal and editor coexist over one session. Platform/editor services supply normal text behavior; Kea supplies explicit submission, retained history, metadata and optional structure. Replay follows from the history model.

## Implemented direction

- GPUI Component owns draft selection, clipboard, undo/redo, mouse editing and composition.
- A live terminal and editor are simultaneously visible over one PTY.
- Live-terminal focus gives ordinary terminal input to the child; the configurable `focus_editor` host escape is the deliberate exception that keeps the workspace keyboard-navigable.
- Run in shell and Send to app are independent actions, not modes.
- Editor-native Enter/newline is the default, but Run/Newline bindings are configurable (including Enter-to-run + Shift+Enter-newline).
- Successful submissions are retained as exact authored drafts independently of optional shell command blocks and can be recalled/edit-as-new.
- Integrated local-shell prompt hooks explicitly report cwd/readiness and preserve user prompt/profile behavior.
- Editor completion uses retained history, shell-reported PATH executables and shell-reported cwd paths; terminal Tab remains native application completion.
- Command blocks are optional/fail-open observers. Execution does not queue or depend on a block.
- Read-only command/output surfaces preserve selection during live output changes.
- The primary terminal screen has bounded local scrollback, viewport-aware pointer selection and selection-only copy.
- Legacy/UTF-8/SGR mouse wheel and primary-button reporting are forwarded from Alacritty's negotiated mode state; DECSET 1002/1003 controls drag/motion forwarding. Shift's left-button local override is configurable; Shift+wheel stays local.
- Terminal selection supports simple/block ranges, explicit caret entry, read-only keyboard navigation and selection-only Copy. Invalidated ranges become visible recovery carets; unrelated input exits locally and reaches the child once.
- Alacritty's Kitty keyboard support is enabled for negotiation. Classic key encoding remains the fallback and modified Enter uses CSI-u only when the child negotiated an extended keyboard mode.
- Replay uses a separate silent historical emulator while live capture continues.
- Sessions start temporary and can transition to an explicit saved journal without losing already-retained history.

## Acceptance

1. **Editor behavior:** select with keyboard/mouse, copy only the selection, cut, paste, undo/redo, Unicode and IME composition. Confirming an IME candidate must never execute a command.
2. **Configurable Enter:** default Enter inserts newline and Ctrl+Enter runs. With `run_shell = enter` and `newline = shift-enter`, Enter runs and Shift+Enter inserts a newline without double insertion.
3. **Explicit actions:** Run in shell and Send to app never depend on hidden mode state. Send never adds a shell wrapper.
4. **Fail-open structure:** execute a command with blocks hidden and with document retention saturated. The command still runs. Missing/malformed metadata may lose a block but must never strand execution in queued state.
5. **Multiline Run:** a draft such as `ls\nls` is transported as one physical PTY line, executes both lines, and may produce one observed block.
6. **cwd:** start Kea from a known directory, run `cd /tmp` via editor and directly in the terminal, and verify the header updates after the next local-shell prompt. While a TUI/SSH owns stdin, label cwd as last reported rather than current.
7. **PATH autocomplete:** modify PATH in the shell, return to a prompt, and verify editor completion sees an executable in the new PATH. File completion is relative to the reported cwd. No draft is evaluated for completion.
8. **Native completion:** terminal Tab reaches Bash/PowerShell/OpenCode/REPL completion unchanged, including profile-provided PowerShell completers.
9. **TUI keys:** with terminal focus, verify Space, Tab/Shift+Tab, Ctrl combinations, modified Enter and function keys are delivered as the negotiated terminal protocol permits.
10. **TUI mouse/selection:** verify SGR click/drag/release reaches a mouse-aware TUI; Shift+drag is local by default and forwarded when configured. Verify explicit selection, block drag, keyboard collapse/extension, copy, focus loss, streaming output and IME precedence per [terminal text selection](terminal-text-selection.md). Shift+wheel remains local scrollback.
11. **Output:** block output remains read-only/selectable; selected/focused snapshots do not jump on live output. Collapse/filter/page/Edit-as-new remain presentation-only.
12. **Replay:** save/reopen without re-executing commands; inspect an overwritten TUI error while live capture remains isolated from historical state.
13. **Persistence:** start temporary, save mid-session, produce more output, reopen the recording and verify both pre-save and post-save history is present.
14. **Appearance/config:** system theme follows OS changes; explicit overrides win; invalid key/settings files fail visibly and safely.

## Remaining work after the first alpha compatibility gate

**Advanced terminal protocols:** focus-in/out event forwarding, image/sixel/graphics protocols, hyperlinks/actions and the higher Kitty keyboard levels for key release/repeat/associated text. Unsupported advanced protocols must not corrupt classic input.

**OS service certification:** real IBus/Fcitx/Wayland/dead-key/AltGr layouts, macOS/Windows IMEs, system prediction/dictation and screen readers. API integration alone is not certification; representative real-OS checks remain release work.

**Autocomplete depth:** editor completion robustly covers history, executables and paths from the integrated local shell context. Application-specific/programmable argument completion remains native terminal behavior unless a safe provider interface is added later.

**Metadata scope:** cwd is robust for the integrated local shell. Remote/internal TUI cwd requires explicit integration from that environment; it must not be guessed. Future metadata may add environment snapshots or richer task context without becoming authoritative execution state.

**Long sessions:** asynchronous cancellable seeks, complete-state checkpoints, compressed/indexed storage, configurable retention and measured memory/CPU/latency overhead.

**Packaging:** native installers/desktop integration, signing strategy and real packaged Windows/macOS/Linux acceptance.

**Zed:** use Zed's editor/text services/actions/PTY/renderer around portable Kea state. Measure overhead/privacy/retention before a narrow upstream proposal.
