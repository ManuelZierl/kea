---
title: Composer draft tabs and actions
nav_order: 17
---

# Deliberate terminal input

Each terminal tab has one receiver and its own set of independently editable
**draft tabs**. The default is still one editor, shown without a tab strip. Draft
tabs do not create shells, execution queues, working directories, or independent
environments.

## Ctrl-C confirmation

`confirm_ctrl_c = false` preserves native forwarding by default. Enable **Confirm
Ctrl-C before forwarding** in Settings, or use the terminal's Ctrl-C control to
cycle between its inherited default and session-local overrides. An override is
not persisted and affects only that terminal.

The guard runs after local selection/Copy and IME routing. Copying selected text
never opens it. A forwarded Ctrl-C or explicit Interrupt action first displays
**Send Ctrl-C to this terminal?** Nothing is sent until a fresh plain Enter.
Escape cancels; another input cancels and follows ordinary input routing. Repeated
Ctrl-C cannot queue, replace or confirm an interrupt. Confirmation keys and their
held repeats are consumed by Kea. The original encoded terminal input is retained;
confirmation does not send a second Enter or replace input with an OS kill signal.

The guard follows terminal encoding: Ctrl+Shift+C also requires confirmation
when it encodes to the same `0x03` byte as Ctrl+C. A platform text commit of that
single control byte follows the same guard. Copy retains its local selection
policy; explicit clipboard paste remains a separate operation.

Confirmation is bound to its original terminal, focus and reported input-context
generation. Switching tabs, losing focus/window activation, replay, process exit
or a new context report cancels it. Streaming output without a context change
does not cancel it. Kea does not infer whether interruption is safe from process
names or screen text. Receiver reports remain unauthenticated interoperability
metadata, not an atomic guarantee that the foreground process cannot change.

## Draft tabs

**Split at cursor** (`Ctrl+Alt+Enter`), **Split here** on a standalone `---`, and
**+** create a new draft tab after the active one and focus it. A split or add
applied from the **Actions** menu preview keeps the first part active, so a split
plan can be sent in order. With two or more
drafts, a tab strip appears above the editor; only the active draft is shown, at
full composer size.

- **Titles are automatic:** the first meaningful line of the draft, shortened to
  24 characters. Blank lines, code fences, `---` and `::` shorthand are skipped;
  an empty draft is shown as *Draft N*. Hover a tab for its full first line.
- **Switch** by clicking a tab or with `Alt+PageUp` / `Alt+PageDown`.
- **Merge ←** joins the active draft to the previous tab with a newline. Kea does
  not infer shell operators, command boundaries or dependencies.
- **×** closes a draft tab without sending it. **Undo draft tab change** reopens
  it with its text. The last remaining draft cannot be closed.
- **Move to ⇄** in the composer header (shown when there is another terminal
  tab, also with a single draft) moves the active draft
  to another terminal tab. Only the text moves: the target gets a new editor, and
  pending confirmations, completions and recall state never cross tabs. Nothing
  is sent. The moved draft becomes that terminal's active draft when you switch
  to it; the source cannot undo the move.

Ctrl+Enter always submits the whole active draft through the existing guarded
submission path, even if some of its text is selected. What happens to the tab
after successful delivery is configurable:

```text
submitted_draft = close   # default: close it and continue with the next tab
submitted_draft = keep    # keep the tab, emptied
```

With `close`, the tab that takes its place (or the previous one, at the end)
becomes active, so a split plan can be stepped through. The last remaining tab is
never closed; it is emptied instead. **Undo draft tab change** reopens a closed
submitted tab with its text. With `keep`, or for the only tab, the emptied draft's
first Ctrl+Z restores the submitted text as an unsent draft. Neither ever re-sends
anything. `post_submit_focus` still controls editor versus terminal focus. Holding
Submit cannot cascade through the remaining drafts.

Submission success means that Kea's terminal write succeeded, not that the child
command succeeded. Failed, cancelled and uncertain writes preserve all pending
text; nothing is retried automatically. Multiline input still requires negotiated
bracketed paste. Submitted history contains exactly the submitted section, not
its neighbours or the structural separators.

Draft tabs retain their editor entities, selections and native text undo
histories when another draft is sent. They stay with their terminal across tab
switches. Closing a terminal checks all of its drafts, including inactive ones.

**Send selected text (keep draft)** is a separate explicit action, unbound by
default. It uses the same readiness/confirmation policy, records only the selected
payload on success, and leaves the original text intact. Selection alone never
changes Ctrl+Enter's submission scope.

### Structural undo

**Undo draft tab change** and **Redo draft tab change** (split, merge, add,
close) are available in the action menu and as configurable shortcuts. Ordinary
Undo/Redo remains the component's text undo. Structural undo restores the
retained editor entities and literal text, including an accepted separator. It
refuses to discard intervening text edits; undo those first. A successful
submission clears earlier structural history; with `submitted_draft = close` it
records only the reopening of that submitted tab. Reopening restores text, never
process effects.

There are at most 32 live draft tabs, eight retained structural edits, and an 8 MiB
accounting budget for retained structural text snapshots. Native editor undo has
its own component-managed allocation. Transformations analyze at most 256 KiB per
draft; oversized drafts remain editable and follow the existing submission limits.

## Recommendations are not syntax

The composer may show a small suggestion when text resembles an editor action:

| Input | Offered action |
| --- | --- |
| Standalone `---` on the current line | Replace the marker with a composer split |
| One complete enclosing code fence | Remove the surrounding fence |
| Several complete fenced blocks | Separate code and all intervening prose into drafts |
| Named placeholders such as `{{host}}` | Preview literal replacement of matching placeholders |
| A standalone `::` or `::split` | Open or filter composer actions |

Typing and pasting use the same passive recognition. A suggestion never changes
text, intercepts Enter/Tab, removes a marker or sends bytes. Ignoring it submits
the original input under the normal submission policy. Pasted YAML separators,
Markdown prompts, here-documents and literal templates are not special syntax.

Click **Split here** on a `---` suggestion to split immediately, remove that
marker and focus the following draft. No menu or preview is required. This is an
explicit, undoable edit, never a submission; ignoring the suggestion still leaves
the marker literal.

Use **Actions** or `Ctrl+.` to deliberately open the action menu. Only that menu
owns Up/Down, Enter and Escape. Transformations show the proposed replacement
before **Apply transformation**. Placeholder replacement is literal: Kea performs
no template evaluation or shell quoting. Inspect the resulting command before
submitting it. The menu validates the original editor, exact text and caret again
before applying; stale recommendations do not mutate a changed draft.

The same editor operations are accessible through shortcuts and mouse controls.
Pattern recognizers only recommend those operations; they are not separate
execution paths. Applying a transformation never submits it. History/memory
insertion uses the existing nonexecuting reverse-search workflow. Shorthand
supports split, add, and memory/history insertion; the full action menu also
provides merge, navigation, structural undo and explicit selection submission.

### Configuration

```text
confirm_ctrl_c = false
composer_suggestions = true
composer_action_prefix = ::
```

`composer_suggestions = false` disables passive hints, not the explicit action
menu. `composer_action_prefix = none` disables shorthand recognition. A custom
prefix must contain at most 16 ASCII punctuation characters, excluding `#` and
`=` which belong to the settings-file grammar. The prefix remains ordinary input
until the associated action is explicitly accepted.

```text
composer_actions = ctrl-.
split_composer = ctrl-alt-enter
previous_composer = alt-pageup
next_composer = alt-pagedown
merge_composer = none
send_selection = none
undo_composer_layout = none
redo_composer_layout = none
```

These bindings apply to the composer, not to terminal/TUI focus. Existing Copy,
Tab completion, Enter/newline, focus escape and platform IME behavior are retained.

## Manual acceptance

Test two terminal tabs with different sets of draft tabs, moving a draft between
them, independent selection/undo,
background output, close confirmation and retained drafts after failed sends.
Check unknown/ready receivers, SSH and an arbitrary TUI, multiline paste support,
held Submit after focus switches, and selected-text sends. Test Ctrl-C with and
without a local terminal selection, disabled/enabled/overridden policy, held
confirmation keys, window focus loss, context changes and live streaming output.

Paste commands, YAML, here-documents and Markdown containing each trigger. Ignore,
cancel and accept recommendations; compare actual submitted bytes. Check action
preview invalidation, Unicode/UTF-16 replacement, literal braces, IME composition,
structural undo and keyboard-only navigation. Real Windows, macOS and Linux input
methods and terminal applications require desktop acceptance beyond compilation
and synthetic component tests.
