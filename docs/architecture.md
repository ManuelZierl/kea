# Architecture

## Embedding boundaries

`kea-core` has no dependencies beyond std. It defines events, bounded recordings, a versioned file format and the Projection trait. `kea-alacritty` supplies a projection using published Alacritty 0.26.0. `kea-pty` wraps portable-pty for the standalone program. `kea-session` coordinates live state, history and disk persistence without a UI dependency. `kea-app` owns GPUI, the local command editor, input modes and keybinding policy.

An eventual Zed host can use the core with its own pinned emulator, PTY, editor and renderer. The standalone session controller and GPUI app are reference hosts, not mandatory integration layers. No Zed code is copied; there is no extension or upstream PR yet.

## Document-native interaction

Kea does not treat immediate PTY key forwarding as the only input model.

### Document mode

Document mode is the default in the standalone app. Keystrokes edit a local multiline UTF-8 buffer. Enter inserts a newline and an explicit semantic `execute` action submits the buffer to the PTY. Paste becomes editor text rather than an immediate process write.

This is currently an interaction layer above an ordinary PTY-backed shell, not yet a complete structured command model. Submission converts local newlines into terminal Enter events. It does not infer commands from prompt text, and the recording format does not pretend it can recover reliable command boundaries after the fact.

Future structured command blocks should be explicit application state, enriched by optional shell integration for metadata such as cwd and exit status.

### Direct PTY mode

Programs such as OpenCode, Vim, REPLs, SSH and other TUIs need immediate terminal input. Direct mode forwards non-application keys through the terminal keyboard encoder. It is an explicit compatibility path, not the default interaction model.

Users can toggle between Document and Direct modes. Direct mode also leaves modified Enter combinations such as `Ctrl+Enter` available to the child when they would otherwise represent the Document-mode execute action.

## Input actions and keybindings

UI commands such as `copy`, `paste`, `execute`, `interrupt`, `toggle_direct`, history navigation and `go_live` are semantic actions. Physical shortcuts map onto those actions in `kea-app`; they do not belong to `kea-core`, `kea-session` or the recording format.

Current defaults follow host-OS conventions where practical:

- Linux/Windows: `Ctrl+C` copy, `Ctrl+V` paste, `Ctrl+Shift+C` interrupt;
- macOS: `Cmd+C` copy, `Cmd+V` paste, `Ctrl+C` interrupt;
- `Ctrl+Enter` executes Document input by default;
- `Ctrl+Shift+Space` toggles Document/Direct mode by default.

The user keymap can override or unbind semantic actions. Invalid or ambiguous configuration falls back to complete OS defaults rather than leaving a partially applied keymap.

Application actions are resolved before Direct-mode PTY forwarding. This separation is important for embedding Kea in Zed, where the host should normally supply Zed actions/keybindings instead of importing the standalone app's configuration system.

## Live and historical projections

Live output is parsed by the live emulator and recorded in ingestion order. A rewind constructs or advances a separate historical emulator. The live emulator continues consuming output and sending supported protocol replies. Returning to LIVE reveals that current state rather than replaying commands or restoring a process snapshot.

Historical mode is visibly read-only and rejects process input. Historical emulators have no external-effects listener. Clipboard, title, bell, URL and window operations originating in recorded terminal output are ignored. User-initiated copying of a historical screen is separate from output-triggered clipboard access. Treat recordings as untrusted parser input, not inherently safe text documents.

Historical view does not resize the live PTY. Recorded sizes reconstruct old screens. A current window may crop an old screen; returning to live sends the current dimensions to the child.

## What is recorded

Opaque output bytes, dimensions and the directly spawned process's exit status. Raw editor keystrokes are not recorded. PTYs merge stdout/stderr: separate streams, individual command exit codes, working directories and command boundaries cannot reliably be reconstructed from these bytes alone. Optional shell integration can add explicit metadata later.

Timestamps are monotonic microseconds measured at ingestion, with equal timestamps ordered by event index. Recordings preserve chunk boundaries. If a message is printed and erased inside one chunk, this version cannot seek to the intermediate instructions; future byte/parser-boundary indexing can address that. This is terminal-protocol history, not exact GPU-frame video.

## Limits and persistence

The recording retains a bounded prefix: 32 MiB accounted data/overhead or 100,000 events. On quota exhaustion, capture stops with a warning while the live process continues. Allocator overhead, queues, grids, fonts, the document editor and GPU resources consume additional memory.

Disk recording is explicit, create-only, and uses an ordered bounded worker. Files are created with Unix mode 0600 or inherited Windows ACLs. Each checksummed event is flushed from the userspace buffer; `sync_all` happens at orderly close, not every event. Power loss can lose recent frames. Incomplete final frames recover a complete prefix; corrupted complete frames fail. There is no encryption, authentication or redaction.

A full disk queue stops persistence with a visible warning rather than blocking rendering. In-memory history may continue. The reader enforces allocation, dimension, event-count and aggregate limits.

## Process lifetime

Input/output have separate bounded worker channels. Input submission never blocks the render loop. Child exit is polled separately from output EOF; the exit event is emitted only after final output is drained.

On Windows an owned ConPTY can keep the output pipe open after the child exits. The transport closes the master off the UI/reader thread after observing child termination, while its reader drains final output. The directly spawned child defines the Windows session lifetime. Unix EOF may wait for descendants holding the slave. Closing the GUI requests termination/reaping of the immediate child; detached descendants are not supervised as a job tree.

Reference: [Microsoft ClosePseudoConsole documentation](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).

## Seeking

Forward playback advances a historical parser. Backward seek replays from zero. There are no fake cell-only checkpoints. Future checkpoints must include parser state, partial UTF-8/escapes, both buffers, terminal modes and other state affecting continuation, with equivalence tests against replay-from-zero. Recordings stay canonical; checkpoints remain disposable versioned caches.

Async cancellable seeking, compression, historical text indexing and retention policy are future work. The format currently lacks an emulator/config fingerprint, so exact cross-engine rendering is not promised.

## Zed direction

The reusable architectural boundary is intentionally below standalone UI policy. A Zed integration should reuse Zed's editor, actions/keybindings, PTY ownership and renderer while consuming Kea's persistent-session concepts and, where appropriate, core recording/history machinery.

Before proposing an upstream change, demonstrate the document interaction value, TUI compatibility and measured overhead, then discuss privacy/storage/dependency policy with maintainers. GPUI reuse lowers impedance but does not make integration a trivial transplant.
