# Daily-driver product acceptance

Kea should win because composing and continuing terminal work is more comfortable than using a conventional terminal plus a scratch editor, without making the terminal less trustworthy.

## Core loop

A user can compose and revise multiline input in a normal editor, submit it to the integrated shell or current terminal application, keep composing while output streams, switch into the terminal for direct interaction, switch back without touching the mouse, recover previously authored submissions as editable drafts, inspect earlier terminal state without affecting the live process, and continue without Kea changing shell semantics.

## Invariants

- Focus policy is explicit, never guessed from process names or terminal output.
- `post_submit_focus = editor` is the default: a successful submission creates and focuses a fresh composer so the next thought can be written immediately while terminal output remains visible. Users who prefer immediate child ownership can set `post_submit_focus = terminal`.
- `focus_editor` is the configurable host escape from a live terminal (Ctrl+L / Cmd+L by default).
- `focus_terminal` is the inverse keyboard path from the composer to the last external surface (Ctrl+Shift+L / Cmd+Shift+L by default). It is intercepted only while the composer owns focus, so the same physical key remains available to a TUI when the terminal owns input.
- Submitted text is user-authored work. Kea retains the exact text of successful **Run in shell** and **Send to app** submissions independently of optional command blocks.
- `previous_draft` / `next_draft` navigate that authored history only while the command editor owns focus. The defaults are Ctrl+Up / Ctrl+Down and remain configurable in `keybindings.conf`.
- Entering history preserves the current unsubmitted draft as scratch text. Navigating forward past the newest submission restores that scratch text exactly.
- Recall edits the composer only. It never sends bytes, executes a command, or claims that the child application accepted the earlier submission.
- Failed sends and `Edit as new` do not become submitted-history entries.
- Shell instrumentation is observational. An editor-run command must leave the same observable shell status (`$?`) as the equivalent native command.
- History and replay never imply process-state rollback.
- Persistence must be explicit about whether a session is temporary, being saved, or has stopped retaining history.
- Product chrome should prioritize terminal + composer. Playback is supporting functionality, not the primary workflow.

## Acceptance scenario

Launch OpenCode (or another interactive TUI), write a substantial multiline prompt in the composer and send it. The fresh composer is immediately ready for another thought while output streams. Use `focus_terminal` when the child needs direct input, answer it, use `focus_editor` to return, recall the previous prompt, modify it, and submit again. No mouse interaction, copied scratch file, lost authored text, process-name sniffing, or ambiguous execution mode should be required.
