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

Start a live session without `--record`, produce output, choose **Save session**, produce more output, exit Kea, then open the resulting `.kea` recording. Both the output from before and after choosing Save must be present, and the original process must never have been restarted or re-executed.
