# Architecture

## Product model

Kea has one underlying terminal process but two UI projections:

1. **Document view** for ordinary shell commands: editable input becomes a first-class command block and its output is persistent/read-only.
2. **Terminal view** for software that requires a mutable terminal screen, including TUIs, REPLs and historical replay.

The terminal/PTY remains the compatibility substrate. The document is not reconstructed from prompts or screen scraping.

## Crate boundaries

`kea-core` has no dependencies beyond `std`. It defines the canonical ordered terminal event recording, bounded file format and the `Projection` replay interface.

`kea-document` depends only on `kea-core`. It defines command blocks, lifecycle state, bounded retained output and the streaming parser for Kea's document boundary protocol. It has no GPUI, PTY, shell, OS or Zed dependency.

`kea-alacritty` is a replaceable terminal projection. `kea-pty` owns the standalone PTY/ConPTY transport. `kea-session` coordinates live emulation, immutable history, persistence and an observation tap. `kea-app` owns GPUI, shell adapters, editor behavior and keybinding policy.

An eventual Zed host should be able to reuse `kea-core` / `kea-document` while using Zed's own PTY, renderer, editor and actions.

## Canonical events vs derived document

Raw terminal output bytes plus ordered resize/process-lifecycle events remain canonical. A document is a derived higher-level view.

Document mode sends an application-owned wrapper to the current shell. The wrapper emits private OSC boundary markers before and after evaluating the submitted command. Those marker bytes flow through the ordinary PTY output and are therefore part of the canonical recording. `kea-document` recognizes the markers and derives:

- original submitted command text;
- command ID;
- queued/running/finished/aborted state;
- start/finish timestamps and duration;
- command exit status;
- output bytes observed between start and completion.

This avoids a second authoritative database and lets an offline `.kea` recording reconstruct structured blocks without command re-execution.

The v1 recording frame schema itself is unchanged; document markers are ordinary output payload bytes. See `document-protocol.md`.

## Shell adapters

Boundary emission must be explicit. Never infer command completion from `$`, `>`, prompt text, idle time or terminal cursor position.

The standalone application currently has adapters for POSIX-style shells and PowerShell. The adapter quotes the editor text as data, emits a start marker, evaluates it in the existing shell scope, captures the status, then emits a completion marker.

Using the existing shell is important: `cd`, shell variables/functions and other session state can persist naturally. If the command launches a TUI, the user switches to Direct PTY mode and sends keys to the same process. When control returns to the shell, the completion marker closes the same command block.

Unknown shells/programs are not guessed; Kea uses Direct PTY mode instead. Additional shell adapters can be added without changing `kea-core` or `kea-document`.

## Application-owned input and PTY echo

The shell wrapper is implementation input, not user-visible command content. `kea-session::send_hidden` therefore suppresses the wrapper's exact terminal-driver echo before feeding output into the live emulator/recording.

The echo filter is deliberately fail-open. If a shell redraws the line instead of echoing the wrapper byte-for-byte, seeing the private start marker proves execution has begun and Kea releases buffered bytes rather than risking loss of actual command output. Correct output is more important than cosmetically hiding a wrapper echo.

The session observation tap receives the same post-filter PTY output that the live emulator receives. It is independent of recording retention so an in-memory document can keep command lifecycle state even if bounded terminal-history capture stops.

## Document retention

The document projection is bounded separately from terminal history. Current limits are:

- command text: 64 KiB;
- output retained per command block: 4 MiB;
- aggregate retained document text/output: 64 MiB;
- block count: 10,000.

A block that exceeds its output cap is explicitly marked truncated. A saturated document does not stop the underlying PTY. Limits are part of the model because a long-running terminal must not become an unbounded GUI heap.

## Document output projection

A block retains raw bytes between its boundary markers. The current GPUI document view renders a conservative text projection: it removes terminal control sequences and handles common carriage-return progress-line overwrites, newlines, tabs and backspaces.

This is intentionally not a claim that arbitrary TUI output is a linear text document. Complex interactive programs should be viewed through the terminal projection; their command block still records lifecycle/output but Direct PTY is the fidelity path.

## Keybindings and input modes

UI operations (`copy`, `paste`, `execute`, `interrupt`, history navigation, mode switching) are semantic actions. Physical shortcuts are mappings in the host UI, not in the recording or portable core.

Document mode owns ordinary editing keys. Direct mode forwards terminal-protocol key sequences. Kea-level actions resolve before Direct-mode forwarding, except the configured Document execute action is intentionally left to the child while Direct mode is active.

## Terminal history

Live output is parsed by the live emulator and recorded in ingestion order. Historical navigation creates/advances a separate emulator. The live emulator continues consuming output while an old state is inspected. Returning to LIVE reveals current state; it never restores a process snapshot or re-executes a command.

Historical emulators cannot emit external effects. Only the live emulator may issue required terminal protocol replies. Replay ignores clipboard, title, bell, URL and window side effects.

## Persistence and trust

Recording is explicit and create-only. Output and structured command text can contain secrets. Recordings are not encrypted or authenticated.

The document OSC protocol is a structural interoperability mechanism, not a security boundary. A process capable of deliberately emitting Kea's private marker syntax could forge document markers. Consumers must not treat block metadata reconstructed from an untrusted recording as authenticated provenance.

Incomplete final recording frames can recover a valid prefix; corrupt complete frames fail. Disk queue/storage failure is surfaced and persistence stops rather than blocking rendering.

## Seeking

Forward playback advances a historical terminal parser. Backward seek currently replays from zero. Future checkpoints must include complete parser state—partial escapes/UTF-8, buffers, modes, margins, tabs, cursor and colors—and remain disposable caches over canonical events.

## Zed direction

The strongest upstream shape is not “replace Zed's terminal.” It is:

- retain Zed's PTY/process ownership and terminal renderer;
- use Zed's editor/action/keybinding model for command input;
- derive persistent command/output blocks from explicit shell integration;
- optionally retain ordered terminal events for historical projection;
- keep portable state independent of GPUI/Zed.

Before an upstream proposal, measure CPU/memory/storage overhead and validate privacy/retention behavior on real long-running sessions and agent workflows.
