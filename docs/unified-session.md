# One session, independent surfaces

Kea's live terminal remains visible above a persistent multiline editor. There is no Document-versus-PTY execution mode. The optional block inspector is only a view of recorded, explicitly delimited shell commands. Hiding it does not replace the terminal, change the child process, or change input routing. `show_blocks = false` is the default.

## Focus owns input

In the command editor and read-only text components, the component/platform handles editing. Kea binds semantic actions only in those editor/chrome contexts. In a live terminal, Kea reserves no application shortcuts: Ctrl+C, Ctrl+V, Tab and function keys are encoded for the child, including the formerly intercepted F6–F10. Use the visible toolbar to focus the editor, copy a view, paste to the child, or inspect history. OS-reserved shortcuts cannot be forwarded, and terminal protocol support still determines which key distinctions a child can receive.

Space has an explicit named-key encoding because Windows can supply `space` without completed text. Composing text is instead routed through GPUI's text-input handler; Kea reuses the editor component to track those composition ranges and only forwards committed text. This does not certify all IME or extended keyboard/mouse protocols.

## Two explicit submission actions, not two terminal modes

- **Run in shell** (`execute`, Ctrl+Enter) evaluates the draft via the existing shell adapter. It is allowed only after an explicit prompt-ready report and while no structured command is running. It is never inferred from `$` or `>` characters.
- **Send to app** (`send_text`, Ctrl+Shift+Enter) pastes the draft to the same child and sends Enter. It never wraps the text in shell source. Multiline sends require the child's bracketed-paste mode, otherwise Kea refuses rather than executing each line.

Both require the focused draft and completed composition. Successful submission creates a fresh draft and focuses the terminal, without hiding the editor. The editor remains editable while the child runs and while inspecting history; sending during history is blocked. No automatic application-name sniffing chooses what a draft means. `toggle_direct` is retained as a backwards-compatible configuration name for showing/hiding the optional blocks, not switching input protocols. `--direct` only chooses initial terminal focus.

## Directory reports and shell integration

Interactive Bash, zsh, POSIX-style shells and PowerShell receive a prompt hook. Bash's existing PROMPT_COMMAND and PowerShell's existing prompt function are retained. Hooks report base64 UTF-8 directory metadata as `OSC 777;kea;prompt;<base64> BEL`. A prompt report closes an interrupted in-flight command as aborted and re-enables shell submission. Raw terminal input invalidates ready state until the shell reports again. The last reported shell directory is shown above the output and captured on new blocks. Noninteractive `-c` / `-Command` / script launches are not injected with hooks.

POSIX reports use base64 and tr; unavailable directory encoding means unknown directory, not a fabricated path. Arbitrary programs/remote shells without integration cannot provide a current directory. Prompt markers are interoperability metadata, not trusted authentication. Unsupported or modified hooks can require native terminal interaction. Last reported shell directory is not the current internal directory of a running remote/TUI program.

## Tab suggestions

Terminal-focus Tab goes unchanged to the child, preserving native shell/REPL completion. Editor-focus Tab offers retained command history and conservative local file/directory suggestions. Leading indentation still inserts an indent through the editor component. Suggestions run off the UI thread, are bounded, and are discarded when the draft has changed. Selecting a suggestion is an undoable text replacement, never execution.

Path suggestions require a reported idle local shell context. Quoted/substitution/glob expressions are left to native completion rather than parsed approximately. Spaces and quotes in filenames are shell-quoted. These are not PowerShell argument/flag completion, remote completion, or LSP suggestions. No command is evaluated for completion.

## Scrolling and Windows

PTY dimensions are derived from the actual laid-out terminal canvas. This prevents the last rows/cursor from being cropped as the editor or block inspector changes geometry. The live terminal displays the current screen; this change does not add terminal scrollback. Newly created/populated block editors go to their tail after layout without stealing focus. Existing selections continue to freeze live block projections.

The Windows binary uses the GUI subsystem instead of allocating a companion console. Pre-window startup errors and command-line help are shown in a GUI window. This does not bypass SmartScreen or application-control policies. No release is tagged as part of this change.

## Validation boundaries

CI checks document metadata parsing, real shell directory changes, key routing/encoding, conservative completion and the Windows PE subsystem. Graphical acceptance captures actual bytes received by a raw PTY child and checks simultaneous editor/terminal input, editing, read-only blocks and replay. It is not certification of all IMEs, screen readers, terminal mouse protocols or OpenCode versions. Keep these distinctions explicit in compatibility reports.
