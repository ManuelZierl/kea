# Architecture

## Product and responsibilities

Kea is a minimal editor with executable input and persistent read-only output. Standard editing and OS integration belong to reusable host components; Kea supplies the execution/document model. Replay is a consequence of retained history.

`kea-core` is standard-library-only canonical output/resize/lifecycle events, bounded storage format and replay traits. `kea-document` depends only on the core and derives command blocks from explicit shell markers. Neither depends on an editor, GPUI, OS UI, shell or PTY implementation.

`kea-alacritty` provides terminal emulation/projection; `kea-pty` provides standalone Unix PTY/Windows ConPTY transport; `kea-session` coordinates live/history engines, the output observation tap and optional journal. `kea-app` composes those with GPUI Component editor surfaces, shell adapters, document presentation, action routing and settings.

An eventual Zed host should retain Zed's own editor, text services, actions, PTY and renderer around the portable Kea layers. The standalone host's component is replaceable, not part of the document format.

## Native-feeling text surfaces

Command drafts and read-only command/output text use GPUI Component `InputState`/`Input`. The component owns buffer operations, selection, grapheme-aware movement, undo/redo, mouse navigation, search and platform composition handling. The former handwritten String/cursor editor and whole-window raw key handler are removed.

Physical shortcuts map to semantic host actions with scoped GPUI keybindings. Standard editing actions dispatch to the focused component. Execute requires the command editor itself to be focused and composition to be inactive. It does not apply to search fields, output or Direct PTY. Copy selection is separate from explicit Copy block/document/screen.

Submitting a command creates a fresh draft entity with a fresh undo history. Historical blocks never become editable. Edit as new copies a previous command into an empty fresh draft and still requires explicit execution.

The document retains a bounded page of stable editor entities. Live updates do not rewrite a focused/selected output snapshot: a visible refresh action updates it explicitly. Search/filter/collapse/page selection are UI projections and cannot execute commands or rewrite recorded output.

Appearance follows system light/dark by default; explicit host overrides are supported. Fonts and ordinary navigation inherit component/platform defaults. Native input APIs are an integration mechanism, not certification of every OS service. See [editor-integration.md](editor-integration.md) for configuration and platform validation boundaries.

## Canonical stream and document protocol

Raw output bytes plus ordered resize/lifecycle events are canonical. Application-owned wrappers emit OSC start/done markers in the current shell around the user's command. The marker carries command ID, submitted text and completion status. `kea-document` derives lifecycle, output and timing without prompt heuristics or a second authoritative database. Reopening recordings reconstructs blocks without execution.

The wrappers evaluate in the existing shell, preserving shell state between submissions. POSIX-style shells and PowerShell have adapters; other programs use Direct PTY. Switching modes does not start a second process. A TUI launched by a command is interacted with through the same terminal; the shell closes its block after control returns. See [document-protocol.md](document-protocol.md).

`send_hidden` suppresses an application's exact wrapper echo before live parsing/recording. It must fail open when shells redraw rather than echo verbatim, never discard real output for cosmetic reasons. The observation tap sees the same post-filter bytes; it can maintain document lifecycle independently if recording retention stops.

## Retention and fidelity

Document limits: 64 KiB command, 4 MiB output per block, 64 MiB aggregate, 10,000 blocks. Terminal history has independent limits of 32 MiB accounted data and 100,000 events. UI editor entities are paged, not allocated for every retained block. Quota exhaustion/truncation is visible; Direct PTY can continue.

The document output projection handles text and basic terminal controls, not all TUI semantics. The live/history terminal projection is the fidelity path for complex TUIs. Its current canvas/keyboard transport is retained in this change; complete terminal selection, mouse protocols, IME and accessibility remain separate requirements.

Live emulation continues during historical inspection. History creates/advances a separate silent emulator. Returning to live shows current state, never restores process state. Historical emulators cannot send terminal replies/input, mutate clipboard, open URLs or change windows. Explicit user actions such as Copy are distinct from replay side effects.

Seeking backwards currently reconstructs from the beginning. A future checkpoint needs the full parser/UTF-8/escape state, both screens, cursor, margins, modes, tabs, colors and dimensions; a grid copy is insufficient. Snapshots/indexes are derived caches, not authoritative history.

## Persistence and trust

Persistence is explicit and create-only. Recordings are unencrypted; submitted command markers/output can contain secrets. No raw keystroke capture is added. Shell markers are interoperability metadata, not authentication; untrusted output can forge metadata. No security decision should trust imported block provenance.

Validate complete-frame recovery, bounded data, live/history isolation and real platform behavior separately. Compilation, component tests, an X11 smoke test and an actual user desktop are different levels of evidence.
