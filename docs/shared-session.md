# One session, independent input and views

The live terminal and multiline editor coexist. There is no process-level Document/PTY switch. Click the terminal to type directly, or click Editor to prepare text. Blocks are an optional derived view, hidden by default; Show blocks or `show_blocks = true` reveals them beside the terminal. The `--blocks` flag enables that view for one launch. Hiding blocks does not discard a recording or change input ownership.

## Input ownership

All configured Kea accelerators are masked while the live terminal has focus. Ctrl+C/V/Z, Tab, modified Enter and F6–F10 reach the terminal keyboard encoder rather than triggering Kea actions. Named Space works even when GPUI on Windows supplies no `key_char`. Clipboard, focus, history and other Kea actions remain available via clickable controls. Shortcuts controlled by the OS/window manager, and sequences not yet supported by the terminal encoder, are not magically made available by this policy.

In the editor, editing/composition remains owned by GPUI Component. Ctrl+Enter explicitly submits the draft; Enter remains newline. There are two *submission targets*, not two exclusive application modes:

- Shell command: the recognized shell receives Kea's command-boundary wrapper. Optional blocks can associate its input, output and status.
- Application text: the current child receives the draft as text plus Enter, with no shell wrapper. Multiline text requires bracketed-paste support; otherwise submission fails visibly and preserves the draft.

A running managed command automatically uses Application text for additional input. Once its explicit done marker arrives, shell-command submission is available again. Unmanaged terminal input invalidates the assumption that a shell is waiting. Click Input: application text to explicitly choose shell-command submission again only after returning to the shell. Kea never guesses readiness from `$`, `>`, idle time or cursor position. Starting with `--direct` selects application-text submission and initial terminal focus; it no longer removes the editor.

## Completion

Tab in the focused terminal goes unchanged to the current application. Ctrl+Space in the editor (or Complete) hands a single-line draft to the same live terminal followed by Tab, **without Enter**, then focuses it. The shell/REPL owns its completion behavior, including existing shell configuration. This is an explicit completion handoff, not inline editor suggestions or a second hidden shell. Multiline/control-character drafts are refused, and a failed handoff preserves the draft. Once input belongs to the terminal, continue editing/executing there or compose additional application text below.

## Directory metadata

Known interactive shells emit an explicit `OSC 777;kea;cwd;<base64 UTF-8 path> BEL` report at startup and around managed command executions. The header displays the last reported directory. The parser is chunk-safe, bounded and independent of output rendering; directory reports are preserved in recordings. Directly typed `cd`, remote SSH shells and programs without integration may change directory without reporting it. The header deliberately says **last reported** rather than claiming that stale data is current. No prompts are scraped or user shell prompt functions overwritten. POSIX reporting uses the system `base64` and `tr`; unavailable commands mean no valid report, not a guessed directory.

## Viewport and scrolling

Terminal dimensions come from the measured canvas, not a guessed subtraction of toolbar heights. This keeps the bottom prompt/cursor visible even when toolbars wrap or the block pane opens. The primary screen retains up to 10,000 visual scrollback lines; wheel scrolling and pointer selection change only the emulator viewport while live PTY output and recording continue. New output does not force a reader back to the bottom.

New block pages follow the newest block after layout. Read-only block editors reveal the end of new output through the component's scrolling machinery while preserving focus. Focused/selected snapshots remain protected from live replacement; explicit refresh is still available. History remains observation, never process restoration or execution.

## Windows

The GUI executable uses the Windows subsystem in both build profiles, so double-clicking it does not allocate a companion console. Startup errors are also written to `%LOCALAPPDATA%\Kea\startup-error.log`; stdout/`--help` are not normally visible when launched without a console. ConPTY remains the child-program transport. Builds are CI artifacts, not signed releases; no release tag is created by this change.
