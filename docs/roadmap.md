# Roadmap and acceptance checks

## Product direction

Kea is a persistent terminal workspace built around a normal editor. The terminal and editor coexist over one session. Platform/editor services supply normal text behavior; Kea supplies explicit submission, retained history, metadata and optional structure. Replay follows from the history model.

## Implemented direction

- GPUI Component owns draft selection, clipboard, undo/redo, mouse editing and composition.
- A live terminal and editor are simultaneously visible over one PTY.
- Live-terminal focus reserves no Kea shortcuts; representable keys/text go to the child.
- Run in shell and Send to app are independent actions, not modes.
- Editor-native Enter/newline is the default, but Run/Newline bindings are configurable (including Enter-to-run + Shift+Enter-newline).
- Integrated local-shell prompt hooks explicitly report cwd/readiness and preserve user prompt/profile behavior.
- Editor completion uses retained history, shell-reported PATH executables and shell-reported cwd paths; terminal Tab remains native application completion.
- Command blocks are optional/fail-open observers. Execution does not queue or depend on a block.
- Read-only command/output surfaces preserve selection during live output changes.
- Replay uses a separate silent historical emulator while live capture continues.

## Acceptance

1. **Editor behavior:** select with keyboard/mouse, copy only the selection, cut, paste, undo/redo, Unicode and IME composition. Confirming an IME candidate must never execute a command.
2. **Configurable Enter:** default Enter inserts newline and Ctrl+Enter runs. With `run_shell = enter` and `newline = shift-enter`, Enter runs and Shift+Enter inserts a newline without double insertion.
3. **Explicit actions:** Run in shell and Send to app never depend on hidden mode state. Send never adds a shell wrapper.
4. **Fail-open structure:** execute a command with blocks hidden and with document retention saturated. The command still runs. Missing/malformed metadata may lose a block but must never strand execution in queued state.
5. **Multiline Run:** a draft such as `ls\nls` is transported as one physical PTY line, executes both lines, and may produce one observed block.
6. **cwd:** start Kea from a known directory, run `cd /tmp` via editor and directly in the terminal, and verify the header updates after the next local-shell prompt. While a TUI/SSH owns stdin, label cwd as last reported rather than current.
7. **PATH autocomplete:** modify PATH in the shell, return to a prompt, and verify editor completion sees an executable in the new PATH. File completion is relative to the reported cwd. No draft is evaluated for completion.
8. **Native completion:** terminal Tab reaches Bash/PowerShell/OpenCode/REPL completion unchanged, including profile-provided PowerShell completers.
9. **TUI keys:** with terminal focus, verify Space, Tab/Shift+Tab, Ctrl combinations, modified Enter and function keys are delivered as terminal protocol permits; Kea actions remain reachable through visible controls.
10. **Output:** block output remains read-only/selectable; selected/focused snapshots do not jump on live output. Collapse/filter/page/Edit-as-new remain presentation-only.
11. **Replay:** record/reopen without re-executing commands; inspect an overwritten TUI error while live capture remains isolated from historical state.
12. **Appearance/config:** system theme follows OS changes; explicit overrides win; invalid key/settings files fail visibly and safely.

## Remaining work

**Terminal protocol completeness:** mouse/focus reporting, terminal selection/scrollback, modern keyboard protocol negotiation, image protocols, measured font metrics and accessibility. The invariant is to reuse established terminal machinery rather than invent Kea-specific behavior.

**OS service acceptance:** actual IBus/Fcitx/Wayland/dead-key/AltGr layouts, macOS/Windows IMEs, system prediction/dictation and screen readers. API integration alone is not certification.

**Autocomplete depth:** editor completion robustly covers history, executables and paths from the integrated local shell context. Application-specific/programmable argument completion remains native terminal behavior unless a safe provider interface is added later.

**Metadata scope:** cwd is robust for the integrated local shell. Remote/internal TUI cwd requires explicit integration from that environment; it must not be guessed. Future metadata may add environment snapshots or richer task context without becoming authoritative execution state.

**Long sessions:** asynchronous cancellable seeks, complete-state checkpoints, compressed/indexed storage, configurable retention and measured memory/CPU/latency overhead.

**Packaging:** native installers/desktop integration, signing strategy and real packaged Windows/macOS/Linux acceptance.

**Zed:** use Zed's editor/text services/actions/PTY/renderer around portable Kea state. Measure overhead/privacy/retention before a narrow upstream proposal.
