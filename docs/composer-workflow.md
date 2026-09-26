---
title: Composer sections and actions
nav_order: 17
---

# Deliberate terminal input

Each terminal tab has one receiver and a stack of independently editable drafts.
The default is still one editor. Sections do not create shells, execution queues,
working directories, or independent environments.

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

Confirmation is bound to its original terminal, focus and reported input-context
generation. Switching tabs, losing focus/window activation, replay, process exit
or a new context report cancels it. Streaming output without a context change
does not cancel it. Kea does not infer whether interruption is safe from process
names or screen text. Receiver reports remain unauthenticated interoperability
metadata, not an atomic guarantee that the foreground process cannot change.

## Independent pending drafts

**Split at cursor** (`Ctrl+Alt+Enter`) creates a structural boundary. The active
section is visibly identified. `Alt+PageUp` / `Alt+PageDown` focus the previous or
next section. **Merge with previous** inserts a newline between the two drafts;
Kea does not infer shell operators, command boundaries or dependencies.

Ctrl+Enter always submits the whole active section through the existing guarded
submission path, even if some of its text is selected. After successful delivery,
only that section is removed. The next section becomes active; when there is no
following section, a fresh empty draft is created. Earlier drafts remain intact.
`post_submit_focus` still controls editor versus terminal focus. Holding Submit
cannot cascade through the remaining drafts.

Submission success means that Kea's terminal write succeeded, not that the child
command succeeded. Failed, cancelled and uncertain writes preserve all pending
text; nothing is retried automatically. Multiline input still requires negotiated
bracketed paste. Submitted history contains exactly the submitted section, not
its neighbours or the structural separators.

Sections retain their editor entities, selections and native text undo histories
when another section is sent. They stay with their terminal across tab switches.
Closing a terminal checks all of its drafts, including inactive ones.

**Send selected text (keep draft)** is a separate explicit action, unbound by
default. It uses the same readiness/confirmation policy, records only the selected
payload on success, and leaves the original text intact. Selection alone never
changes Ctrl+Enter's submission scope.

### Structural undo

**Undo composer split / merge** and **Redo composer split / merge** are available
in the action menu and as configurable shortcuts. Ordinary Undo/Redo remains the
component's text undo. Structural undo restores the retained editor entities and
literal text, including an accepted separator. It refuses to discard intervening
text edits; undo those first. Successful submission clears structural undo/redo,
so it cannot resurrect a submitted draft or pretend to reverse process effects.

There are at most 32 live sections, eight retained structural edits, and an 8 MiB
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

Test two tabs with different multi-section drafts, independent selection/undo,
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
