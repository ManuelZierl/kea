# Architecture

## One session, independent surfaces

Kea displays a live terminal and a persistent multiline editor together. Focus decides who receives input. A block inspector is optional presentation, hidden by default; it is not a separate execution mode. See [unified interaction](unified-session.md).

The editor/platform owns ordinary editing, selection, clipboard, undo and text composition. Kea owns explicit submission, process lifecycle, retained documents and historical inspection. A live terminal reserves no Kea shortcuts. The toolbar remains usable when the child owns the keys.

The same PTY is used by direct terminal input, Run in shell, and Send to app. There is no second shell and no application-name guessing. Run in shell requires an explicit prompt-ready report. Send to app pastes text and Enter without any shell wrapper. Both reject historical input and incomplete composition.

## Crate boundaries

- `kea-core`: standard-library-only ordered output/resize/lifecycle recording, bounded file format and replay interface.
- `kea-document`: depends only on kea-core; streaming structural markers, command/output records and last reported shell directory/readiness. No UI, OS, shell or PTY dependency.
- `kea-alacritty`: replaceable terminal projection.
- `kea-pty`: standalone Unix PTY / Windows ConPTY transport.
- `kea-session`: live/history emulation, optional persistence, observation tap and application-owned input echo handling.
- `kea-app`: GPUI surfaces, GPUI Component editor integration, shell adapters, focus-scoped actions and conservative local completion.

A Zed integration should use Zed's editor/actions, terminal renderer and PTY ownership, while reusing portable recording/document layers. Do not import the standalone UI or copy GPL Zed code into Kea's MIT crates.

## Canonical events and metadata

Raw terminal output plus ordered resizes/lifecycle events remain canonical. Structured command records are derived from explicit OSC markers, never prompt regexes, idle time, cursor position or `$`/`>` text. Shell wrappers emit submitted text/ID, start and completion markers; prompt hooks report base64 UTF-8 directory and readiness. Marker bytes travel through the same output stream and recording. Offline reconstruction never re-executes commands.

The last shell directory is metadata, not an assertion about the internal cwd of a running TUI or remote process. Native input invalidates prompt-ready state until the shell reports again. This prevents a shell wrapper from being appended to a partially typed native prompt or sent into an ordinary running application by accident. A prompt returning without a completion marker marks an interrupted tracked command aborted.

Markers are structural interoperability, not authentication. Programs can forge terminal output. Never treat imported metadata as trusted provenance or an authorization boundary. Retained histories contain submitted command text and may contain secrets.

## Shell integration and compatibility

Interactive POSIX-style shells and PowerShell use host-owned adapters. Noninteractive command/script invocations are not injected. Bash's prior prompt commands and PowerShell's prompt function are retained. Other hosts/shells may supply adapters without changing the core. POSIX directory encoding uses base64/tr; missing encoding produces unknown directory, not fabricated metadata.

App-owned wrappers are not user command text. The existing echo filter suppresses exact driver echoes and fails open on actual start/prompt markers. Cosmetic wrapper text may remain when an interactive line editor redraws it. Never drop real output to hide a wrapper echo.

Complex TUIs use the terminal projection; a lossy text projection of cursor operations is not a faithful replacement for a TUI. Blocks can be inspected separately without forcing the live application into a linear text model. Terminal emulation/input protocol completeness remains a real integration responsibility, not something supplied automatically by the OS.

## Text services, layout and completion

GPUI Component supplies the draft, read-only block editors and search fields. Native text enters through its text-input integration. The compatibility terminal also registers a GPUI text-input handler, reusing a bounded InputState for composing ranges and forwarding only committed text. This bridge is not certification of every platform IME, dictation or accessibility service.

PTY rows and columns come from the actual laid-out canvas rather than guessed window offsets, so editor/inspector layout cannot hide the current cursor below the rendered viewport. The live surface follows the current terminal screen. Historical screen dimensions stay with the recording.

New/populated block editors move to their tail after layout without changing focus. Selected/focused block text freezes on incoming output until explicitly refreshed. Paging older blocks does not request latest-page scrolling.

Editor Tab offers bounded retained-history and conservative local path suggestions on a worker thread. No shell code is evaluated. Results are checked against the draft and cursor before applying an undoable replacement. Native terminal Tab is passed to the child for its own completion. Unknown/complex expressions are not approximately evaluated.

## Replay and retention

Live and historical emulators are separate. Live output and required protocol replies continue while the user inspects an older screen. Historical emulators cannot send input, issue protocol replies, alter the clipboard, open URLs or change windows. Returning to live changes the view, never the process state.

Terminal history is bounded to 32 MiB of accounted event data/overhead or 100,000 events. Structured retention is bounded separately: 64 KiB per command, 4 MiB output per block, 64 MiB aggregate retained command/output/directory data and 10,000 blocks. Quota exhaustion/truncation is visible; the PTY may continue. The inspector creates at most 24 block editors at once.

Disk recording is explicit, create-only, bounded and unencrypted. Storage failures stop persistence visibly rather than block rendering. Incomplete final frames can recover a valid prefix; corrupt complete frames fail. Backward seeking currently replays from zero. Future checkpoints require the full parser state, partial escape/UTF-8 state, both buffers, modes, margins, tabs, cursor and colors—not just a cloned grid.

## Platform validation

Keep compilation, unit/component tests, graphical acceptance and real platform validation separate. The Windows binary uses the GUI subsystem; CI checks the PE header and produces an untagged executable artifact. A successful Linux graphical test or Windows build is not a claim of full Windows OpenCode/IME/mouse compatibility.
