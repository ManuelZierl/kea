# Human desktop acceptance checklist

Use this on a real desktop with a physical keyboard/mouse, your normal shell
profile, and real terminal applications. Unit tests and synthetic graphical
tests cannot certify the entire OS → focus → widget → terminal → application
path, clipboard ownership, candidate windows, or whether the feedback makes sense.

Some underlying rules have automated coverage. The purpose here is to verify
their **actual user-visible behavior**, especially failures such as Enter being
swallowed or local selection appearing to interact with a TUI.

**Quick regression pass:** setup and tests 01–07, about 20 minutes.
**Broader platform pass:** add 08–15, including an available real IME.
Use disposable commands/sessions and recognizable sample text. Do not use
important running work to test interruption or submission.

## Record the environment first

| Field | Value |
|---|---|
| Date / tester | |
| Commit and any source changes | |
| Exact executable path / build time | |
| OS and version | |
| Linux: native Wayland or X11/XWayland, desktop/compositor | |
| Keyboard layout, physical keyboard and IME | |
| Mouse/trackpad, display scaling and monitors | |
| Shell / OpenCode / Vim or Neovim / tmux versions used | |
| Settings and keybinding overrides | |
| Comparison terminal used for child-native behavior | |

For each numbered test record **PASS**, **FAIL**, **BLOCKED**, or **NOT RUN**.
Do not mark a test passed because its equivalent unit test passed. A missing IME
or an OS-reserved shortcut is a recorded limitation, not a pass.

Start each numbered case in a known application state, with no Kea selection
unless the steps request one. Use Stop selecting to clear local interaction;
reset pending child input/menus separately so effects from an earlier case do
not look like a new failure.

## Setup — avoid testing yesterday's binary

1. Close the Kea instance being tested. Replacing an executable does not update
   a process that is already running.
2. From the repository root, build the profile you will actually launch:

   ```sh
   git rev-parse --short HEAD
   git status --short
   cargo build --locked --release -p kea-app
   ./target/release/kea
   ```

   On Windows, launch `.\target\release\kea.exe`. If testing debug instead,
   build without `--release` and launch `target/debug/kea` explicitly. Do not
   assume a desktop launcher or an executable on PATH points to this build.
3. Record the settings in use. For a default-policy pass, use separate temporary
   files via `KEA_SETTINGS` and `KEA_KEYBINDINGS` rather than overwriting your
   everyday configuration. An empty file selects the built-in defaults.
4. Start with `post_submit_focus = editor`, `shift_mouse_selects_locally = true`,
   and `persist_history = false`. Settings-dialog presentation/input-policy changes
   apply immediately; restart after external file/keybinding edits or a
   draft-history persistence change.
5. Identify the **upper terminal**, **lower composer**, and the terminal's
   **Select text / Stop selecting** button. A Kea selection should have the
   terminal header's **Local selection** feedback; a TUI's highlight alone does
   not establish Kea ownership.

**Shortcut notation:** Copy is Ctrl+C on Linux/Windows and Cmd+C on macOS.
Ctrl+C remains the terminal interrupt chord on macOS. Composer escape is
Ctrl+L / Cmd+L; composer → terminal is Ctrl+Shift+L / Cmd+Shift+L. Use your
recorded remappings if different. Do not substitute Ctrl+Shift+C for Copy.

## 01 — Plain Enter and Tab in the upper terminal

- [x] Click the upper terminal, type `echo KEA_ENTER_ONCE`, and press **Enter**.
  Expect one result line in addition to the shell's command echo. Ctrl+Enter
  must not be required, and focus must remain in the terminal.
- [x] Repeat after switching to the composer and back using the keyboard.
- [x] Start a familiar partial command or filename and press **Tab**. Expect the
  shell's normal completion behavior, not a focus jump or a swallowed key.
- [x] Exercise Space, Backspace and arrows on a partially typed command. Compare
  with the same shell/profile in your comparison terminal if behavior differs.

**Failed**: A simple click int the upper terminal the followin happens: The header moves because the text in there changes, with that the terminal moves up and this is then detected always as a drag motion! We must prevent that the upper terminal can move like this!
It feels weird to me that terminal -> compose = ctrl-l but composer -> terminal = ctrl-shift-l? Shouldnt there be a single switch button? And does the Ctr-l switch not conflict with out always send to terminal first policy?

## 02 — Plain Enter in OpenCode or another TUI

1. Start a disposable OpenCode session inside the upper terminal. First perform
   a harmless menu action using its normal Enter binding.
2. Test its normal prompt-submission action with a short benign message such as
   `Reply only KEA_ENTER_OK.` Observe the submitted message count, not just the
   eventual model response. If you skip sending a message, record that subcase
   as NOT RUN.
3. Exercise Tab, Shift+Tab, arrows, Backspace and Escape in the child's own UI.
4. Compare Shift+Enter with the same app/configuration in another terminal. Its
   behavior depends on the child's bindings and negotiated protocol; do not
   assume every shell distinguishes Enter from Shift+Enter.

**Fail:** Ctrl-V is not send to opencode!

## 03 — Copy a Kea selection without interrupting the child

Use a harmless child producing output long enough to observe whether it stops.
For example, in Bash:

```sh
for i in $(seq 1 60); do printf 'KEA_TICK %s\n' "$i"; sleep 1; done
```

PowerShell equivalent:

```powershell
1..60 | ForEach-Object { Write-Output "KEA_TICK $_"; Start-Sleep -Seconds 1 }
```

1. Drag-select `KEA_TIC` within a tick line, leaving its final `K` unselected.
   With mouse reporting active, hold Shift
   **before mouse-down**. Check the Kea header says Local selection.
2. Release the mouse and Shift. Press Copy once, then again.
3. Paste into a separate plain-text editor. Expect only the selected text; the
   child should still be producing ticks. A copy notice alone is insufficient.
4. Switch back with the OS app switcher, without clicking terminal text and
   replacing the selection. Use Shift+Right, then Copy. The selected text should
   become `KEA_TICK`; the child must not receive navigation or interrupt.
5. Use Stop selecting to clear local interaction. Press Ctrl+C in the terminal
   as a positive control: the harmless child should now stop normally.

**Fail:** Copy interrupts the child, stops generation, triggers an exit warning,
copies the whole screen, or fails to reach another application's clipboard.

## 04 — Distinguish OpenCode's copy popup from leaked Ctrl+C

Use a fresh or reset child selection and reset Kea local selection between runs.

| Gesture | Observe before any Copy key |
|---|---|
| Ordinary drag over selectable OpenCode output | Child-native selection/copy behavior; establish the baseline. OpenCode may auto-copy on release. |
| Shift held before press, drag, release, then release Shift | Kea's local highlight and header; no child click/selection side effect. |
| Shift held before press, drag, release Shift while still holding the mouse, continue dragging, then release | Still the same Kea-owned gesture; no child receives the remainder. |
| Start an ordinary child-owned drag, then press Shift partway through | Still child-owned; Kea must not take over a gesture already sent to the child. |

For each Kea-owned case, **wait a second after mouse release before pressing
Copy**. Note any child popup at mouse-down, during motion, at mouse-up, during
that pause, and after Copy. Paste into another editor to check the actual text.

OpenCode's **Copied to clipboard** popup can come from its own mouse-up copy
path and appear asynchronously. It is not, by itself, evidence of a Ctrl+C
interrupt. Record an unexpected child copy popup as a separate failure to
investigate, rather than labeling it a keyboard leak or ignoring it. An actual
generation interruption/exit warning is a different observation.

Repeat a few times at normal speed, including movement after release. Record
the exact modifier/button order and whether Kea's Local selection header was
present. This is important for intermittent reports.

## 05 — First key after selection is never lost

- [ ] At the shell prompt, type `echo KEA_AFTER_SELECTION` but do not submit.
  Select some earlier output, then press plain Enter. The pending command should
  run once on that press and the local selection should clear.
- [ ] Select again, then type one harmless character into the shell/TUI input.
  The first character should appear exactly once and clear local interaction.
- [ ] Repeat with Tab and Backspace where their child-side effects are visible.
- [ ] With a local selection and a harmless child menu open, press Esc once.
  Only Kea's selection should clear. A second Esc should perform the child's
  normal Escape action.

**Fail:** first input is swallowed, duplicated, requires a second press, or a
local command is still captured after Kea's interaction has visibly ended.

## 06 — Explicit selection entry and focus

1. While in a mouse-aware TUI, click Select text. Expect a visible local caret
   distinct from the application's cursor, terminal focus, and a pressed/textual
   control state.
2. With no selected range, press Copy. Expect a no-selection notice and unchanged
   clipboard contents—not an interrupt or whole-screen copy.
3. Move with arrows, extend with Shift+arrows, and Copy. Check the result in a
   separate application.
4. Click to place a caret, then Shift+click another endpoint. Select without
   holding down a continuous drag.
5. Clear the range using Stop selecting, then repeat entry keyboard-only:
   composer escape, then F4. F4 from live-terminal
   focus itself belongs to the child, not this action. Account for any OS/Fn-key
   requirement on your keyboard.
6. Click Stop selecting with a caret active. It should turn off, not immediately
   re-enter selection because of a focus transition.
7. With only a caret, focus the composer: caret-only interaction exits. With a
   non-empty range, focus the composer: the terminal range may remain, but Copy
   now acts on the focused composer's selection.

## 07 — Clipboard ownership across real applications

- [ ] Copy terminal text containing spaces, multiple lines, `ä`, `界`, and an
  emoji. Paste into a native text editor and a browser text field. Check exact
  characters and line boundaries, not just a success notification.
- [ ] Copy a different selection and paste again; check that stale clipboard
  contents are not reused. Switch away and back before pasting too.
- [ ] Copy text outside Kea and use the terminal's visible Paste button. It should
  reach the child once and clear any local selection. Keyboard Ctrl+V in a live
  terminal remains child input; do not assume it is the same action.
- [ ] Try multiline Paste in a child with bracketed-paste support. Text should
  arrive as one paste rather than executing line by line. In an unsupported
  child, expect visible refusal and no partial submission.

On Linux, test your actual Wayland/X11 clipboard manager. Primary-selection
middle-click behavior is separate from Copy/Paste and is not a substitute for
these checks. Also record what happens to clipboard contents if Kea exits;
clipboard-manager ownership after application exit varies by desktop.

## 08 — Selection during output, scrolling and resizing

1. Produce numbered lines, scroll above the tail, select an older line, and let
   new output continue. Reading position and copied text should remain stable
   while that content is retained.
2. Scroll while extending a selection across the viewport; copy and inspect its
   endpoints. Try a wrapped line and a line containing wide characters.
3. Resize the window and drag the terminal/composer divider. Valid selections
   should remain meaningful. If content/reflow invalidates one, expect a visible
   recovery caret and explanation, not a stale highlight or silent interruption
   on the next Copy.
4. In a disposable TUI, trigger a redraw or alternate-screen transition while
   locally selected. Check the same recovery behavior.
5. Drag outside the terminal/window and release, then return the pointer. Check
   for stuck dragging, unexpected child clicks, or selection that keeps growing
   after release. Record edge autoscroll availability separately; do not assume
   drag-past-edge autoscroll is implemented.

## 09 — Mouse override and block selection on your desktop

1. With default settings, hold Shift before dragging in a mouse-aware TUI; local
   selection should be immediate. Shift+click extends an existing Kea range.
2. Set `shift_mouse_selects_locally = false`, restart the exact same binary and
   application, and repeat. The child should receive the complete gesture.
3. With that override disabled, use Select text. Local selection must still be
   possible without Shift. Restore the default setting afterward.
4. When mouse reporting is off, Alt-drag over a small text table. Copy should
   contain a rectangle, not a linear span of whole intervening lines. During
   mouse reporting use Shift+Alt-drag, or explicit Select text then Alt-drag.
5. Check ordinary wheel scrolling versus Shift+wheel. The latter is Kea's local
   scrollback override regardless of the Shift-drag setting. Test your trackpad's
   fractional scrolling and direction as well as a wheel if available.

If your window manager captures Alt-drag, record that conflict. Do not count a
window move as a successful column-selection test.

## 10 — Real IME, dead keys and AltGr

Use an actual available IME: for example IBus/Fcitx on Linux, or a macOS/Windows
input method. Record the engine and layout. Repeat in both composer and terminal.

1. Begin composition and change candidates with arrows/Space. Preedit should not
   reach or execute in the child. The candidate window should be near the active
   input location, including after window movement or display scaling changes.
2. Confirm with Enter. It must confirm the candidate, not also run the draft or
   submit to the child. A subsequent plain Enter follows the focused surface's
   normal behavior.
3. Cancel a composition with Esc. It should cancel preedit before any terminal
   selection or child Escape action.
4. Repeat with a local terminal selection/caret present. Candidate navigation
   must not move that selection. Committing text clears local interaction and
   inserts the committed text exactly once into the child.
5. Switch between terminal and composer during composition; then resume typing.
   Look for ghost preedit, wrong candidate placement or lost/duplicated text.
6. Try a dead-key combination and layout-specific AltGr characters such as `@`,
   `€` or braces. Expect characters rather than unintended Ctrl/Alt commands.

If the IME is unavailable, mark its cases BLOCKED. Synthetic Unicode insertion
is not a substitute for candidate navigation, cancellation and commitment.

## 11 — Composer workflow and physical shortcuts

- [ ] With default settings, Enter in the lower composer inserts a newline and
  does not execute. Explicit Run/Send acts once on the intended child.
- [ ] After successful submission, a fresh composer is focused by default. Type
  immediately: it should edit the new draft, not leak into the TUI. Undo should
  affect only this draft, never the previous command/output.
- [ ] Recall two earlier submissions with Ctrl+Up/Down while retaining a scratch
  draft. Return to the scratch draft and verify exact text and caret usability.
- [ ] Set `post_submit_focus = terminal`, restart, and repeat: immediate typing
  should now go to the child. Restore your preferred policy afterward.
- [ ] With `run_shell = enter` and `newline = shift-enter` in a separate test
  keybindings file, restart and verify both physical chords in the composer.
  Then focus the upper terminal: plain Enter must still be child input.
- [ ] Fill the composer with enough lines to scroll, move away from a distinctive
  line, then use Ctrl/Cmd+F. Typing its text and navigating previous/next should
  reveal the active match without moving the draft caret or changing Undo.

Include your usual OS shortcuts, Fn layer and keyboard layout. Record conflicts
instead of inferring physical-key support from a keybinding label.

## 12 — Real shell, TUI and remote transitions

Use the applications you actually work with; mark unavailable ones NOT RUN.

| Environment | Human check |
|---|---|
| Normal shell/profile | Prompt appearance, aliases and native Tab still work. Type `cd` directly, press Enter, then check Current shell directory. Repeat via Run. |
| OpenCode | Send a multiline composer draft, work directly in its input, return to composer, recall/edit/send. No surprise focus change or hidden Run/Send target. |
| Vim/Neovim | Enter/leave alternate screen, insert Unicode, navigate, enable native mouse selection, then use Kea Shift-selection. Exit without broken display/input. |
| tmux | Exercise native pane clicks, scrolling and application shortcuts, then Kea's override. No mixed gesture or stuck button state. |
| SSH / REPL | Native Tab and input remain application-owned; Send is literal. Cwd is last reported, not an invented remote/application directory. |

While a child owns stdin, Run must not inject a shell wrapper. After returning to
the actual integrated-shell prompt, Run should become available again. An empty
looking line alone must not be treated as readiness; observe the displayed
reason if submission is refused.

## 13 — Layout, scaling and sustained use

- [ ] Resize narrow/wide, maximize/restore, drag the split and toggle blocks.
  Prompts/cursors, action buttons and important feedback should stay reachable.
- [ ] Move between displays with different scaling. Check mouse-to-cell alignment,
  selection endpoints, caret positioning, clipboard text and IME candidates.
- [ ] Change the OS appearance or increase the configured font size, then test
  contrast, wrapping and clipping. Judge the terminal grid and editor separately.
- [ ] Open Settings from the toolbar, switch between System/Light/Dark, close and
  reopen Kea, and verify the saved choice. With System selected, change the OS
  appearance and verify Kea follows it without reopening Settings.
- [ ] Work for 10–15 minutes with realistic output, scrollback and focus switches.
  Watch for UI stalls, reading-position jumps, lost selections and stale notices.

Judge whether you can tell **who receives the next key** without guessing from
colors alone. Record confusing feedback even if the eventual bytes are correct.

## 14 — History and saving as a user workflow

1. Start a temporary session and produce recognizable output. Save session,
   produce more output, and check that the visible saving state/path is useful.
2. Enter History while the live child continues. It must be visibly read-only;
   typing must not reach the hidden live process. Find the way back to Live and
   confirm input works immediately there.
3. Close and reopen the saved recording. Find both periods of output and verify
   that the original command/application is not launched again.
4. If an actual save failure occurs during testing, check that the warning is
   visible and understandable and that live input still works. Do not fill your
   real disk to manufacture this case; deterministic fault injection belongs in
   the automated persistence tests.

Draft recall persistence is a separate opt-in feature. If you use it, also
check a restart with sample submitted drafts. Its current failure-reporting
limitations are documented in [session persistence](session-persistence.md#submitted-draft-history).

## 15 — Keyboard-only operation and assistive technology

- [ ] Complete the compose → submit → terminal → composer loop without a mouse.
- [ ] Enter local selection via composer escape then F4; navigate, extend, copy
  and exit without dragging. Confirm a visible focus/caret indication throughout.
- [ ] With your actual screen reader or other assistive technology, check whether
  Terminal, Composer, Select text, selection state and errors are discoverable
  and announced meaningfully. Also test enlarged text and OS contrast settings.

Known boundary: the pinned GPUI stack does not expose semantic accessibility
name/value support for the selection control. Visible labels and pressed
styling do not establish screen-reader support. Record this as an explicit gap
where observed, not a passed accessibility check.

## Record a failure so it can be reproduced

```text
Test ID and result:
Commit / exact binary / restarted after build?:
OS / Wayland-X11 / layout / IME / scaling:
Child app + version and relevant settings:
Focused surface and visible Kea selection/caret state:
Exact event order (Shift down, mouse down, move, Shift up, mouse up, Ctrl+C):
Expected result:
Actual result, including popup timing and clipboard text:
Frequency (for example 3 failures in 10 attempts):
Same steps in comparison terminal:
Screenshot or short screen recording with sample/non-sensitive content:
```

## Results

| Test | PASS / FAIL / BLOCKED / NOT RUN | Notes or issue |
|---|---|---|
| 01 — Shell Enter/Tab | | |
| 02 — TUI Enter | | |
| 03 — Copy without interrupt | | |
| 04 — Child copy popup / gesture ownership | | |
| 05 — First input after selection | | |
| 06 — Explicit selection / focus | | |
| 07 — Cross-application clipboard | | |
| 08 — Output / scroll / resize | | |
| 09 — Override / block selection | | |
| 10 — Real IME / AltGr | | |
| 11 — Composer / shortcuts | | |
| 12 — Shell / TUI / remote transitions | | |
| 13 — Layout / scaling / sustained use | | |
| 14 — History / saving | | |
| 15 — Keyboard-only / accessibility | | |

Distinguish **confirmed child action** from **a popup that looked like one**, and
a failed assertion from a blocked environment. Do not conclude that the entire
platform passes from one successful shell or one successful input method.

Current contracts: [interaction](unified-session.md),
[terminal selection](terminal-text-selection.md),
[platform/editor integration](editor-integration.md), and
[terminal compatibility](terminal-compatibility-alpha.md).
