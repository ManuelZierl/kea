# Roadmap and acceptance checks

## Product direction

Kea is a minimal editor with executable input and persistent read-only output. Replay follows from its history model. Platform services and mature editor components supply normal editing; Kea supplies execution and document semantics.

## Implemented host direction

- GPUI Component replaces the handwritten String/cursor command editor.
- Selection-only copy, cut, paste, undo/redo, navigation and text composition use the focused editor component.
- Execute is explicit and composition/focus guarded; every successful submission creates a fresh undo history.
- Read-only command/output editor surfaces retain selection during live output changes with explicit snapshot refresh.
- Bash Tree-sitter highlighting, optional line numbers/wrapping, system light/dark appearance and configurable host settings.
- Collapse/expand, command/output filtering, bounded widget pages, Copy block/document, and safe Edit as new.
- The persistent command/document/recording model and same-PTY Direct compatibility remain unchanged.

## Acceptance

1. Type, select with keyboard/mouse, copy just the selection, cut, undo and redo. Empty selection must not overwrite the clipboard.
2. Paste multiline Unicode text. It must remain a draft; Enter adds lines. Only the configured execute action submits it.
3. Compose text with an OS input method. Confirming a candidate must not execute a command; Ctrl+Enter cannot execute unfinished composition. Record actual OS/IME/display-backend results separately from simulated API tests.
4. Execute `cd /tmp`, then `pwd`; shell state and structured block metadata must remain consistent.
5. Undo immediately after successful execution. It must not alter/rerun the prior block or resurrect it through the fresh draft's undo history.
6. Select output and try typing, paste, cut, deletion, undo and platform text replacement. Output remains read-only but Copy/Find/navigation work.
7. Inspect a running block while new output arrives. The selection stays stable; refresh explicitly updates the snapshot. Collapsing/filtering/paging are presentation-only actions.
8. Edit an earlier command as new; the original stays immutable and a nonempty draft is protected. Execution still requires an explicit action.
9. Remap/unbind shortcuts and verify old component bindings do not remain active. Direct PTY still receives ordinary terminal keys such as Ctrl+Z, Ctrl+L and Tab.
10. Change system light/dark appearance; follow-system updates, explicit theme override does not. Test font/wrapping settings and invalid-file fallback.
11. Record and reopen a short document session without command re-execution; recover a transient TUI error in replay.

## Remaining work

**OS service acceptance:** actual IBus/Fcitx/Wayland/dead-key/AltGr layouts, macOS/Windows IMEs, system prediction/dictation, screen readers and accessibility tree. Integration APIs alone do not certify these.

**Document richness:** trustworthy shell cwd/environment metadata, virtualized large-document navigation, output styles, bookmarks/diffs, historical full-text indexing and stdout/stderr separation where the transport provides it.

**Direct terminal correctness:** reusable complete terminal input/selection component, mouse/focus events, terminal IME, full keyboard negotiation, image protocols, measured font metrics, scrollback and accessibility. Validate OpenCode/Vim/less/SSH/tmux on real systems.

**Long sessions:** asynchronous cancellable seeks, complete-state checkpoints, compressed/indexed storage, configurable retention, and measured memory/CPU/latency overhead.

**Packaging:** native platform installation/build tooling, Linux desktop integration and packaged macOS/Windows validation. Build systems require project configuration; OS packaging is not inherited from a text editor widget.

**Zed:** reuse host editor/text services/actions/PTY/renderer around portable Kea state. Measure overhead, privacy and retention before a narrow upstream proposal; no claim of acceptance by Zed maintainers.
