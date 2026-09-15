# Daily-driver product acceptance

Kea should win because composing and continuing terminal work is more comfortable than using a conventional terminal plus a scratch editor, without making the terminal less trustworthy.

## Core loop

A user can compose and revise multiline input in a normal editor, submit it to the integrated shell or current terminal application, interact directly with that application, return to the composer without touching the mouse, recover previously authored submissions as editable drafts, inspect earlier terminal state without affecting the live process, and continue without Kea changing shell semantics.

## Invariants

- `focus_editor` is the explicit configurable host escape from a live terminal. All other Kea semantic shortcuts remain owned by the child while terminal focus is active.
- Submitted text is user-authored work. Kea should retain it independently of optional shell command blocks and allow recall/edit-as-new.
- Shell instrumentation is observational. An editor-run command must leave the same observable shell status (`$?`) as the equivalent native command.
- History and replay never imply process-state rollback.
- Persistence must be explicit about whether a session is temporary, being saved, or has stopped retaining history.
- Product chrome should prioritize terminal + composer. Playback is supporting functionality, not the primary workflow.

## Acceptance scenario

Launch OpenCode (or another interactive TUI), write a substantial multiline prompt in the composer, send it, answer an interactive question in the terminal, use the configured host escape to return to the composer, recall the previous prompt, modify it, and submit again. No mouse interaction, copied scratch file, lost authored text, or ambiguous execution mode should be required.
