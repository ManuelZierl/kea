---
title: Alpha.4 acceptance
nav_order: 20
---

# Alpha.4 release-candidate acceptance

**Preparation, not publication.** `0.0.1-alpha.4` must not be tagged or promoted
to `main` until the maintainer signs off on the checks below. The published
release remains `v0.0.1-alpha.3`.

## Automated evidence and limits

Local validation on Linux:

| Check | Result | Scope |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Workspace formatting |
| `cargo test --locked --workspace` | PASS | 286 tests; one ignored documentation example |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS | Kea crates; existing upstream future-compatibility warnings remain |
| `wt validate/test/check --no-global` | PASS | No blocking findings; cwd-label and retained-growth advisories inspected |
| Reverse-search standalone harness | PASS | 19 tests |
| Native Windows icon source check | PASS | Source generation only; hosted CI checks the Windows executable |
| Locked debug and release builds | PASS | Linux x86_64 |
| Isolated X11 debug smoke | PASS | Tabs, tmux lifecycle/error, composer, full terminal/editor suite |
| Extracted Linux candidate archive | PASS | `--help`, executable byte comparison and local SHA-256 manifest |
| Isolated X11 release smoke | PASS | All five suites above plus maximized Settings/title-bar clicks under Openbox |
| Monitored release-app inspection | PASS, limited | Light/system appearance at 1050×780 and minimum-width 760×600; tmux rows and controls visible |

The cwd advisory is guarded by `local_shell_ready()`; the retained event append
is private and preceded by quota checks/rolling eviction. Neither finding was
silenced to obtain a pass.

The local Linux archive contains README, changelog, the Kea license and vendored
dependency licenses/patch notices. It is a candidate artifact, not a published
release; its local checksum is not a signature or a certification of other OSes.

The integration candidate passed the isolated Linux/X11 tmux lifecycle check:
attach, tab-close cancellation/confirmation, detach with the server/session still
alive, and Kill cancellation/confirmation. The discovery-error check preserved
the actual PTY geometry and rejected pointer/wheel input behind the manager.
These scripts use disposable servers; they do not inspect the user's tmux server.

The PTY teardown regression was also checked against the original implementation:
the original failed, and the corrected implementation passed. The fixture uses a
writable portable temporary directory, not a developer-specific path.

Independent code review found no remaining material issue in the updater, tmux
guard/pipe handling, overlay routing or teardown correction. Automated validation
does not certify Windows PowerShell helper execution, a non-admin installer
run, native Wayland, macOS/Windows graphical behavior, physical IME/keyboard
input, accessibility or subjective visual clarity.

One early full-smoke Settings click failed without an established root cause;
later full runs passed. This record does not claim a proven fix for that transient
failure. The tmux session-loss failure, in contrast, was reproduced and traced to
PTY writer destruction injecting newline/VEOF before the child was terminated;
the input sender now survives until child kill/wait completes.

## What needs your judgment

Use sample data, disposable sessions and the exact candidate executable. Close
older Kea instances first. Record commit, executable path, OS/compositor, shell,
keyboard layout, scaling and relevant settings. Mark each check **PASS**, **FAIL**,
**BLOCKED** or **NOT RUN**; a compiled platform is not a tested desktop.

Build and launch from the candidate checkout:

```sh
cargo build --locked --release -p kea-app
./target/release/kea
```

Windows: use the candidate workflow's `kea-windows-x86_64` artifact, not the
published alpha.3 download. The artifact includes the executable and per-user
installer. Record the workflow commit. macOS requires a source build before
publication; the normal candidate matrix only tests the portable engines there.

Download a candidate Windows artifact with GitHub CLI (replace `RUN_ID` with a
successful CI run for the prepared `develop` commit):

```sh
gh run download RUN_ID --repo ManuelZierl/kea --name kea-windows-x86_64 --dir kea-alpha4-windows
```

Do not use the public alpha.3 artifact for alpha.4 acceptance.

### 1. Visual clarity and real desktop — Linux, Windows, macOS

- Resize narrow/wide, maximize/restore, increase font size and move between
  displays at different scaling. Test dark, light and system appearance.
- Judge contrast, clipping and readability in the title bar, tmux overlay,
  confirmation prompts and frozen-selection feedback. Controls and their labels
  must remain reachable; a prompt must make its destructive scope clear.
- Enable Ctrl-C confirmation, run a harmless long-lived child, then press Ctrl-C
  with no selection. The warning should overlay output without changing terminal
  rows or moving the TUI. Escape cancels; a fresh Enter confirms once.
- Repeat Settings shortcut Record/Save and right-side title-bar clicks when
  maximized on your normal compositor. Synthetic X11 is not GNOME/Wayland proof.

### 2. Selection and clipboard — real TUI and separate applications

- In OpenCode or another busy mouse-aware TUI, hold Shift before drag. The visible
  text should freeze while the child keeps running. Copy twice and paste into a
  separate native editor; selected text must remain identical.
- Shift-click and Shift-drag a second endpoint: extend from the original anchor,
  including after scrolling. Test explicit Select text/caret entry too.
- Return live with the visible action or F9 from chrome/composer. Then verify
  normal TUI input. With Shift-local disabled, ordinary Shift mouse gestures must
  reach the child; Ctrl+Shift-drag still forces Kea-local frozen selection.
- In the visible live shell, run this portable OSC 52 command:

  ```sh
  printf '\033]52;c;S0VBX09TQzUyX1RFU1Q=\a'
  ```

  Paste into a different application: expect `KEA_OSC52_TEST`. Repeat through
  your actual SSH/tmux configuration; those tools may filter OSC 52 themselves.
- Viewing a recording/frozen grid or output from an inactive tab must not replace
  the clipboard. A clipboard read request must not disclose local clipboard text
  to the terminal program. Local selection Copy remains a separate explicit action.

### 3. Local tmux lifecycle — disposable server only

Use an isolated default server so **Kill** cannot affect your working sessions:

```sh
qa_dir=$(mktemp -d)
mkdir -m 700 "$qa_dir/tmux"
TMUX_TMPDIR="$qa_dir/tmux" tmux -f /dev/null new-session -d -s kea-qa
TMUX_TMPDIR="$qa_dir/tmux" ./target/release/kea
```

- Open **tmux**, refresh, verify the session name/counts, then **Open**. The new
  tab attaches to that session; native pane/input commands still work.
- Edit distinct drafts in the ordinary shell and tmux tab. Switch/reorder tabs;
  neither draft, undo, completion nor recording state may migrate to the other.
- Close the tmux tab, then reopen it. Its shell/process state must survive.
  Closing the Kea window must likewise leave the persistent tmux session alive.
- Choose **Kill**, cancel, and verify it survives. Then confirm Kill of only the
  disposable session; verify it disappears. Judge whether detach versus destroy
  is sufficiently obvious in the UI.
- Leave an old manager row open, stop/recreate the disposable server externally,
  then try its old Open/Kill action. It must reject the stale target, not attach
  to or kill a replacement session. Refresh should discover the replacement.
- Test absent tmux and a stopped server: visible error or empty-list feedback,
  no UI stall, no damage to existing Kea tabs.

Cleanup only this test server after closing Kea:

```sh
TMUX_TMPDIR="$qa_dir/tmux" tmux kill-server
```

If it is already stopped, tmux's “no server running” error is expected. Do not
run an unqualified `tmux kill-server` or reuse an important session for this test.

### 4. Windows installation/update — real non-admin account

Use [Windows updater acceptance](manual-testing.md#windows-updater--no-elevation-acceptance)
for install, uninstall, Start-menu/Apps entries, no UAC, opt-out, workspace-global
status, cancel/confirm restart and portable migration.

For installation-only acceptance, run the candidate `*-setup.exe` interactively
as the normal account, launch from Start, and check the title-bar version and
`%LOCALAPPDATA%\Programs\Kea\kea.exe`. Then uninstall using Windows **Installed
apps** and confirm its shortcut/application entry is removed without UAC. Keep
sample settings/history separate; uninstall is not permission to erase them.
If a restart handoff fails, retain `%LOCALAPPDATA%\Kea\update-failure.log` with
the test report rather than interpreting a closed window as success.

**Publication dependency:** the production updater uses GitHub releases. Before
alpha.4 is published, alpha.3 has no updater and the candidate cannot update to
itself. You can test candidate installation, manual checks and opt-out now, but
a real newer-version update needs two eligible published versions or a controlled
test harness. Record end-to-end updating as **BLOCKED**, not PASS, until that
condition exists. Do not alter production release tags to manufacture a test.

Digest-mismatch fault injection also requires a controlled test harness; the UI
does not provide a custom update URL. Do not claim this was manually exercised
from the normal production UI.

### 5. Physical input, accessibility and sustained use

Run [desktop checks 01–15](manual-testing.md) on your actual environment,
prioritizing physical Enter/Tab, held-key confirmation, AltGr/dead keys, real IME
candidate navigation/placement, shell profiles, SSH/OpenCode and screen readers.
Work for at least 15 minutes with output, tab switching, selections and recording.
Check responsiveness and reading position. Accessibility and TUI off-screen
document reconstruction remain known limits; do not infer support from visual
labels or synthetic Unicode paste.

## Maintainer sign-off

| Check | Platform/environment | Result | Evidence/issue |
|---|---|---|---|
| Visual clarity / scaling / overlay | | NOT RUN | |
| Busy-TUI selection / external clipboard / OSC 52 | | NOT RUN | |
| tmux detach / kill / stale targets | | NOT RUN | |
| Non-admin Windows install / uninstall | | NOT RUN | |
| Windows newer-version update | | BLOCKED | Needs a newer eligible release or harness |
| Physical keys / AltGr / real IME | | NOT RUN | |
| Accessibility | | NOT RUN | |
| Sustained everyday workflow | | NOT RUN | |

After sign-off, use [the release procedure](releasing.md#tag-and-publish): promote
the tested candidate to `main`, date the changelog, verify hosted CI, and only then
create the matching `v0.0.1-alpha.4` tag. No publication is authorized by this record.
