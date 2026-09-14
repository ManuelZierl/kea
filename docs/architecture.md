# Architecture

## Embedding boundaries

`kea-core` has no dependencies beyond std. It defines events, bounded recordings, a versioned file format and the Projection trait. `kea-alacritty` supplies a projection using published Alacritty 0.26.0. `kea-pty` wraps portable-pty for the standalone program. `kea-session` coordinates live state, history and disk persistence without a UI dependency. `kea-app` owns GPUI and UI policy.

An eventual Zed host can use the core with its own pinned emulator, PTY and renderer. The standalone session controller is a reference host, not a mandatory integration layer. No Zed code is copied; there is no extension or upstream PR yet.

## Document-native direction

The long-term UI model is a persistent session document, not one mutable terminal surface. Normal command input should be represented as editable text and executed explicitly; completed commands and their output become persistent document content. A PTY-backed terminal surface remains available for applications that genuinely require terminal semantics.

Terminal replay is therefore not the top-level product abstraction. Ordered terminal history is one capability that makes interactive PTY blocks persistent and inspectable instead of ephemeral.

## Input actions and keybindings

UI commands such as `copy`, `paste`, `execute`, `interrupt`, `seek_back`, and `go_live` are semantic actions. Physical shortcuts are mappings onto those actions and belong to the host UI/configuration layer, not `kea-core` or the recording format.

Defaults should follow host-OS conventions where practical. In particular, copy should default to `Ctrl+C` on Linux/Windows and `Cmd+C` on macOS. Sending a terminal interrupt is a separate action with its own configurable binding. Users may override both and can choose traditional terminal behavior if preferred.

Interactive terminal blocks may forward keys to the PTY according to the terminal keyboard protocol, but application-level actions are resolved first according to the configured keymap. This separation is important for embedding Kea in an editor such as Zed, where the host may supply its own keybinding system entirely.

## Two independent views

Live output is parsed by the live emulator and recorded in ingestion order. A rewind constructs or advances a separate historical emulator. The live emulator continues consuming output and sending supported protocol replies. Returning to LIVE reveals that current state, rather than replaying commands or restoring a process snapshot.

Historical emulators have no external-effects listener. The adapter accepts only live PtyWrite events; clipboard, title, bell, URL and window operations from terminal output are ignored. User-initiated copying of a historical screen is separate from output-triggered clipboard access. Treat recordings as untrusted parser input, not inherently safe text documents.

Historical view does not resize the live PTY. Recorded sizes reconstruct old screens. A current window may crop an old screen; returning to live sends the current dimensions to the child.

## What is recorded

Opaque output bytes, dimensions and the directly spawned process's exit status. No raw input events. PTYs merge stdout/stderr: separate streams, individual command exit codes, working directories and command boundaries cannot reliably be reconstructed from these bytes alone. Shell integration can add optional metadata later.

Timestamps are monotonic microseconds measured at ingestion, with equal timestamps ordered by event index. Recordings preserve chunk boundaries. If a message is printed and erased inside one chunk, this version cannot seek to the intermediate instructions; future byte/parser-boundary indexing can address that. This is terminal-protocol history, not exact GPU-frame video.

## Limits and persistence

The recording retains a bounded prefix: 32 MiB accounted data/overhead or 100,000 events. On quota exhaustion, capture stops with a warning while the live process continues. Allocator overhead, queues, grids, fonts and GPU resources consume additional memory.

Disk recording is explicit, create-only, and uses an ordered bounded worker. Files are created with Unix mode0600 or inherited Windows ACLs. Each checksummed event is flushed from the userspace buffer; sync_all happens at orderly close, not every event. Power loss can lose recent frames. Incomplete final frames recover a complete prefix; corrupted complete frames fail. There is no encryption, authentication or redaction.

A full disk queue stops persistence with a visible warning rather than blocking rendering. In-memory history may continue. The reader enforces allocation, dimension, event-count and aggregate limits.

## Process lifetime

Input/output have separate bounded worker channels. Input submission never blocks the render loop. Child exit is polled separately from output EOF; the exit event is emitted only after final output is drained.

On Windows an owned ConPTY can keep the output pipe open after the child exits. The transport closes the master off the UI/reader thread after observing child termination, while its reader drains final output. The directly spawned child defines the Windows session lifetime. Unix EOF may wait for descendants holding the slave. Closing the GUI requests termination/reaping of the immediate child; detached descendants are not supervised as a job tree.

Reference: [Microsoft ClosePseudoConsole documentation](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).

## Seeking

Forward playback advances a historical parser. Backward seek replays from zero. There are no fake cell-only checkpoints. Future checkpoints must include parser state, partial UTF-8/escapes, both buffers, terminal modes and other state affecting continuation, with equivalence tests against replay-from-zero. Recordings stay canonical; checkpoints remain disposable versioned caches.

Async cancellable seeking, compression, historical text indexing and retention policy are future work. The format currently lacks an emulator/config fingerprint, so exact cross-engine rendering is not promised.

## Zed direction

Demonstrate value and measured overhead, then discuss privacy/storage/dependency policy with maintainers before proposing an upstream change. Reuse Zed's PTY, renderer and preferably its keybinding/action system; add ordered output capture and a persistent/read-only historical projection. GPUI reuse helps but does not make integration a trivial transplant.
