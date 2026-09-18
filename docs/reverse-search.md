# Native reverse search

Ctrl-R in the compose editor opens a lightweight history-and-memory popup anchored
above the input. With terminal focus, Ctrl-R still belongs to the shell or TUI.
The History button opens the same popup with the mouse, including while the
terminal owns keyboard input. There is no separate memory application or second
shell.

## Interaction

The original draft seeds a separate search field. Editing that query never edits
the draft. With an empty draft the popup shows ranked recent input and available
saved memories.

| Input | Action |
| --- | --- |
| Type | Case-insensitive, Unicode-aware subsequence search across words |
| Up / Down | Select a match or an action |
| Ctrl-R again | Next match; follows the configured reverse-search binding |
| Page Up / Page Down | Move by one page |
| Enter or click a result | Replace the compose draft; **never execute** |
| Ctrl-Enter or Actions | Details and available actions |
| Ctrl-S or Save draft | Name and save the original draft |
| Escape | Back, then close without changing the original draft |
| Click outside | Close without stealing focus from the clicked surface |
| Mouse wheel | Move through results or actions |

Right Arrow retains its normal search-field cursor behavior. The action shortcut
is Ctrl-Enter, not a hidden interception of ordinary text navigation. Every
operation has a visible mouse path and a keyboard path. The popup intercepts
submission actions, including user overrides such as `run_shell = enter`.

Enter insertion uses the existing editor's UTF-16 replacement API and undo
history. It preserves exact Unicode and multiline source; control-safe summaries
are presentation only. If the target draft changes externally or is replaced by
a new editor entity, old results cannot overwrite it. Search generations reject
stale worker results and clicks. Composition is not interpreted as execution or
acceptance.

Configure the opener in `keybindings.conf`:

```text
reverse_search = ctrl-r
# Alternatives: reverse_search = alt-r; reverse_search = none
```

Ctrl-R is the default on macOS as well as Linux and Windows. The existing terminal
mask also covers a remapped opener. The compose editor's embedded Find field and
read-only output fields do not open reverse search.

## History and saved definitions

History observes **successfully submitted compose input**, independently of
optional command-block retention. Run in shell is labeled with its integrated
shell dialect and last explicitly reported starting directory. Send to app is
literal text with unknown application/remote directory. Submission is not a
claim that execution succeeded; no exit code or duration is fabricated.

Repeated historical input is grouped by exact text and input kind, while retaining
individual occurrence IDs. Ranking combines fuzzy quality, recency, frequency,
exact current-directory relevance, and a saved-memory boost. Search is bounded
and runs on a worker, never on the PTY or UI execution path.

The details/action view supports inserting, copying exact text, saving a named
memory, renaming, changing scope, and forgetting. Saved definitions are separate
from occurrences: renaming a memory does not rewrite what was submitted.
Global memories are available everywhere. Exact-directory memories require a
currently reported matching local-shell directory; unknown or stale context does
not count as a match. No parent-directory/project inheritance is implied.

Save draft also works before any execution, and without a matching result. Saving
is explicit and never executes. Memory names are unique per input kind and scope.
A memory recalled into another shell remains editable text: there is no automatic
shell translation or automatic choice between Run and Send.

## Persistence and privacy

Automatic history is session-only by default. Enable durable compose history in
`settings.conf`:

```text
history_persistence = true
```

Named saves always request local persistence explicitly. Disabling automatic
persistence controls new writes; it does not delete old records, and existing
persisted history remains searchable. Submissions starting with a space are
excluded from automatic memory, but may still be explicitly saved. This is not a
secret scanner, nor does it change shell history or full terminal recordings.

Default memory directories:

| Platform | Directory |
| --- | --- |
| Linux | `$XDG_DATA_HOME/kea/input-memory`, otherwise `~/.local/share/kea/input-memory` |
| macOS | `~/Library/Application Support/Kea/input-memory` |
| Windows | `%LOCALAPPDATA%\Kea\input-memory` |

`KEA_MEMORY_DIR` overrides the directory and must be absolute. Loading a missing
directory does not create it. The directory is created on a write; Unix uses
0700 for the directory and 0600 for files. On Windows the application-data
location's inherited ACL applies; Unix permission bits are not claimed there.
All stored text is **unencrypted**.

Forgetting asks for confirmation and removes input-memory records only. It does
not erase terminal output, shell history, recordings, backups, or filesystem
snapshots; it is not secure erasure. Execution and the draft remain usable if
memory storage fails. A warning reports a dropped/unavailable memory operation
rather than silently claiming complete history.

## Boundaries and storage

`reverse_search.rs` is a std-only model and bounded fuzzy ranking implementation.
`reverse_search_store.rs` handles versioned, length-checked files.
`reverse_search_worker.rs` owns I/O and search on a bounded channel.
`reverse_search_view.rs` is a GPUI component with no PTY or execution capability.
The host observes successful Run/Send submissions and provides the current editor
and explicitly known directory context. No replay-core dependencies change.

Retention is limited per loaded library to 5,000 records and 16 MiB of accounted
text/metadata, with a 64 KiB per-input limit. Search returns at most 80 matches and
accepts at most 512 query characters. The UI pages six rows. Exhaustion is surfaced
instead of dropping old user-saved definitions silently. Concurrent instances use
independent per-record files and refresh on opening the popup; they do not replace
one shared snapshot. Simultaneous writes may temporarily exceed a per-library
quota on disk, and conflicting edits to the same definition are last-writer-wins.

Each `.kmem` file has `KEA-MEMORY\0` plus version byte 1, an input-kind byte,
little-endian u64 timestamp, and bounded length-prefixed UTF-8 fields. Optional
fields carry a presence byte. Writes use a same-directory temporary file and
rename. Unknown versions, truncated data, unsafe identifiers, symlinked data
directories, and oversized data are rejected with warnings. Do not edit these
files as shell scripts or treat them as authenticated provenance.

This change deliberately does **not** claim full tmem parity. Commands typed
straight into the terminal are not reconstructed from keystrokes or screen text.
Parameterized templates, ordered command-group editing/execution, project-scope
inheritance, tmem migration/import/export, and historical output search require
separate work. They should build behind this same small Ctrl-R interaction rather
than adding a competing execution workflow.

## Validation

Unit tests cover fuzzy/Unicode search, scoped matching, grouping, ranking, input
bounds, deletion, storage round trips, malformed/truncated records, concurrent
independent writes, Unix permissions, worker failure handling, and generation
propagation. GPUI component tests cover non-destructive cancellation, Unicode
replacement, changed-draft rejection, composition, and shortcut scoping.

Run the repository's existing formatting, portable-engine, app-library, desktop
build and Linux smoke checks. The portable memory tests can also be run without
GPUI dependencies with `bash scripts/test-reverse-search-core.sh`.

Graphical acceptance additionally needs these checks on real builds:

1. Run and Send several multiline drafts; search, move, insert, undo, and cancel
   using only the keyboard. Confirm Enter never runs a match.
2. Repeat with `run_shell = enter` and with a remapped/unbound `reverse_search`.
3. While Bash/OpenCode/Vim owns the terminal, confirm Ctrl-R still reaches it.
4. Exercise mouse selection, scroll, actions, outside dismissal, and a small or
   resized window. The popup must not resize the PTY or hide below the window.
5. Save, rename, scope, restart, reopen from another Kea instance, and forget a
   memory. Repeat with automatic persistence both enabled and disabled.
6. Test IME composition, embedded Find focus, replay transitions, stale queries,
   a replaced draft, corrupt memory files, and unwritable storage.

Automated component tests are not evidence of real OS/IME or graphical acceptance.
