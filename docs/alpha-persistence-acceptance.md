# Alpha persistence acceptance cases

These cases are release blockers for the session-persistence stack.

1. Start temporary, emit recognizable output, start persistence, emit more output, close, replay the file: both periods are present.
2. Attempt to save to an existing path: Kea refuses rather than overwriting it.
3. Force a disk writer failure: the live PTY continues and the session reports that disk persistence stopped.
4. Reach the canonical history retention limit: the live PTY continues and Kea reports that retained/saved history is incomplete.
5. A session that is never explicitly saved creates no recording file.
6. Saved files on Unix are owner-only (`0600`).
7. Replay never starts the original command or sends input.
