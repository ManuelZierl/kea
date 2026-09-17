# Session persistence

Kea sessions start **Temporary**. Terminal output can contain secrets, so persistence is never silently enabled just because Kea is running.

A live temporary session can transition to **Saving locally** at any time. Kea first writes the complete canonical history retained so far to a new `.kea` file, fsyncs that snapshot, then appends future terminal events through the existing bounded background journal. Existing files are never overwritten.

Saved session locations are product-owned rather than tied to the working directory:

- Linux: `$XDG_STATE_HOME/kea/sessions/`, otherwise `~/.local/state/kea/sessions/`
- macOS: `~/Library/Application Support/Kea/Sessions/`
- Windows: `%APPDATA%\Kea\Sessions\`

Explicit `--record NEW.kea` remains supported for callers that want a specific create-new path.

## States shown to users

- **Temporary** — history exists only for the current process.
- **Saving locally** — a `.kea` journal exists and new retained events are appended to it.
- **Saving stopped** — disk persistence failed or was explicitly stopped. In-memory history may continue; the UI must not describe the saved file as complete.
- **History stopped** — the bounded canonical recording reached its retention limit. The PTY may remain live, but neither replay nor the saved recording is complete past that point.

## Invariants

- Saving is an explicit user action.
- Starting save mid-session includes history already retained before the action.
- Files use create-new semantics and Unix mode `0600` where applicable.
- Disk work after the initial snapshot stays off the render loop.
- Persistence failure never stops or queues terminal execution.
- The existing recording bounds remain authoritative for the alpha; hitting them is surfaced rather than hidden.
- `.kea` files are unencrypted. The UI must communicate that saved terminal content may include secrets.

## Alpha acceptance

The session-persistence release criteria are:

1. Start temporary, produce recognizable output, choose **Save session**, produce
   more output, close, and replay the file. Both periods must be present.
2. Saving to an existing path refuses rather than overwrites it.
3. A disk-writer failure leaves the live PTY running and visibly reports that
   saving stopped.
4. Exhausting retained history leaves the live PTY running and visibly identifies
   the incomplete history/recording.
5. A session never explicitly saved creates no `.kea` recording.
6. Saved files on Unix are owner-only (`0600`).
7. Replay neither launches the original process nor sends input to it.

## Submitted-draft history

Draft recall is separate from `.kea` session recording. It is in-memory by
default. The opt-in `persist_history = true` setting stores submitted drafts in
`draft-history.txt` in the session directory and reloads up to 500 entries on
startup. The text is plaintext; Unix files are owner-only. Disabling the setting
does not remove a previously written file.

Draft-history updates write and sync a complete sibling before replacing the
previous file. A write failure stops draft persistence and remains visibly
reported while in-memory recall continues for the current process. Loading is
bounded to the newest 500 entries and rejects files larger than 16 MiB rather
than reading unbounded input during startup.
