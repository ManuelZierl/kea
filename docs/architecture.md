# Architecture

## One session, independent surfaces

Kea displays a live terminal and a persistent editor together. Focus decides who receives physical input. A block inspector is optional presentation, hidden by default; it is not an execution mode. See [interaction contract](unified-session.md).

The editor/platform owns ordinary text editing, selection, clipboard, undo and composition. Kea owns explicit submission, process/session history, optional metadata and historical inspection. A live terminal reserves no Kea semantic shortcuts.

The same PTY is used by terminal input, **Run in shell**, and **Send to app**. There is no second shell, no submission-target mode and no application-name guessing.

## Execution before metadata

Execution is authoritative; structure observes it.

`Run in shell` requires an explicit prompt-ready report, creates a shell-driver line, and sends it. It does **not** first create a queued document block. Start/done markers in later output may derive a block, but retention exhaustion, malformed/missing markers or document-parser failure cannot prevent the command from executing.

`Send to app` never creates shell metadata or a wrapper. It sends literal editor text plus Enter to the current stdin owner, using bracketed paste for multiline content when the application enables it.

This keeps the optional block model useful without making terminal correctness depend on it.

## Crate boundaries

- `kea-core`: std-only ordered output/resize/lifecycle recording, bounded file format and replay interface.
- `kea-document`: depends only on `kea-core`; optional streaming command/output/cwd metadata derived from explicit markers. No UI, shell, PTY or OS dependency.
- `kea-alacritty`: terminal emulation/projection.
- `kea-pty`: Unix PTY / Windows ConPTY transport.
- `kea-session`: live/history emulation, observation tap, application input, playback and optional journal.
- `kea-app`: GPUI surfaces, platform-integrated editor, shell prompt hooks, completion and host actions.

A Zed integration should use Zed's editor/actions/terminal renderer/PTY ownership while reusing the portable recording/document layers. The standalone GPUI UI is replaceable.

## Canonical stream and metadata

Raw terminal output plus ordered resize/lifecycle events are canonical. Live sessions also retain a bounded, event-aligned presentation stream for rewind, so application-owned shell driver echoes hidden from the live terminal remain hidden during playback without changing raw bytes, timestamps or journal data. Imported v1 recordings have no separate presentation data and replay their canonical stream. Structured records are derived from explicit OSC metadata, never prompt regexes, idle time, cursor position or `$`/`>` text.

Integrated shell prompt hooks report cwd/readiness through bounded OSC 777 metadata. The live host also receives the effective shell `PATH` on OSC 778 for editor command completion. PATH is completion context, not required for replaying command blocks.

Native terminal input immediately invalidates prompt-ready state. Only the next explicit shell prompt report makes **Run in shell** safe again. This prevents a shell wrapper from being appended to a partially typed prompt or injected into OpenCode/SSH/a REPL.

Markers are interoperability data, not authentication. Programs can forge terminal output; metadata must never become an authorization boundary.

## Shell integration

Interactive POSIX-style shells and PowerShell receive small host-owned prompt hooks. Noninteractive command/script invocations are not injected.

The integration preserves user shell behavior where possible:

- Bash retains existing `PROMPT_COMMAND`;
- zsh extends `precmd_functions`;
- PowerShell keeps the user's profile and delegates to the pre-existing prompt function;
- unsupported/remote foreground applications are not guessed.

Prompt hooks update cwd and PATH after commands typed either through Kea or directly in the terminal. While a foreground TUI/remote program is active, the local shell metadata is explicitly treated as **last reported**.

Shell driver input for **Run in shell** transports multiline drafts as one physical PTY line. User newline bytes are encoded as data and reconstructed inside the shell, avoiding interactive line-editor splitting before the start marker executes. Before the start marker, the driver prints a control-safe presentation of the original draft at the terminal's measured prompt column; private transport text remains hidden.

The hidden-input echo filter fails open: if exact cosmetic suppression becomes unsafe, real output wins over hiding wrapper text.

## Text services and key routing

GPUI Component supplies the editable draft, read-only block text and search fields. Platform text-input integration owns composition and replacement ranges. The terminal surface also implements GPUI's text-input handler so committed IME/composed text can reach the child without a handwritten character approximation.

Editor submission keys are semantic/configurable actions. The editor-native default leaves Enter to the editor and binds Run in shell to Ctrl+Enter. A terminal/chat-style policy can instead bind Enter to Run and Shift+Enter to Newline.

With live terminal focus, every Kea accelerator—defaults and user overrides—is masked at the deeper terminal key context. The terminal encoder then receives representable key distinctions. OS-reserved combinations and distinctions absent from the terminal protocol cannot be recreated by Kea; modern keyboard-protocol negotiation is a terminal-compatibility concern.

## Completion

Terminal Tab is passed to the child unchanged and remains the authoritative path for shell programmable completion, REPL/application completion, aliases/functions and remote/application-specific behavior.

Editor Tab uses bounded local completion on a worker thread. When an integrated shell is idle it combines:

- retained command prefixes;
- executables from the shell-reported effective `PATH`;
- filesystem entries relative to the shell-reported cwd.

Candidates are discarded if text/cursor changed and are applied as ordinary undoable editor replacements. Kea never evaluates draft shell code for completion. Complex syntax intentionally falls back to native terminal completion instead of speculative parsing.

## Layout and optional blocks

PTY rows/columns come from the actual laid-out terminal canvas, not guessed window offsets. The terminal/command-draft divider is draggable, and each resulting layout remeasures the terminal rather than cropping unseen rows. The optional inspector follows the same rule.

The terminal emulator retains a bounded 10,000-line visual scrollback projection. Scrolling changes only the selected emulator viewport: live PTY output, recording and protocol replies continue, and new output does not force a reader back to the tail. When the child requests mouse reporting, ordinary vertical wheel input uses its negotiated legacy, UTF-8 or SGR encoding; Shift+wheel remains an explicit local-scrollback override. Pointer selection and selection text extraction reuse Alacritty's grid, wrapping and wide-cell semantics rather than implementing a second terminal text model. Whole-view copy remains a separate explicit action.

Block widgets are bounded/paged. Output text is read-only/selectable. Focused or selected live snapshots do not change under the reader; explicit refresh updates them. Block UI actions cannot execute commands or alter recorded history.

## Replay and retention

Live and historical emulators are separate. Live output/protocol replies continue while an older screen is inspected. Historical emulators cannot send input, issue replies, mutate the clipboard, open URLs or change windows. Returning to LIVE changes only the view.

Terminal history is currently bounded to 32 MiB of accounted data/overhead or 100,000 events. Structured retention is bounded separately. Reaching a block/document limit may truncate optional structure but never stops the PTY or command execution.

Disk recording is explicit, create-only, bounded and unencrypted. Backward seeks currently replay from zero; future checkpoints must capture complete parser/emulator state, not only the visible grid.

## Platform validation

Compilation, unit/component tests, graphical acceptance and real-machine validation are distinct evidence. Priority real-system targets include Windows + OpenCode, macOS/Windows IMEs, Wayland/IBus/Fcitx, terminal mouse-protocol forwarding, keyboard protocol negotiation, accessibility, long-session scrollback/selection behavior and packaging.
