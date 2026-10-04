---
title: Alpha.3 acceptance
nav_order: 19
---

# Alpha.3 acceptance record — 2026-10-04

This is evidence for `0.0.1-alpha.3`, not cross-platform desktop certification.
The release combines PRs #28 (rolling retention/frozen selection/input guards),
#29 (shortcut recorder visibility) and #30 (selection autoscroll/navigation and
history popup isolation), with the frozen/clipped interaction correction below.

## Environment and evidence boundary

- Linux x86_64; isolated X11/Xvfb and Openbox, software Vulkan/Lavapipe.
- Exact repository-built `target/debug/kea`, restarted after rebuilding, with
  disposable settings, keybindings and synthetic output. No normal user session
  or configuration was changed.
- Interactive QA used the desktop QA workspace/viewer, synthetic pointer/key
  input, screenshots and actual clipboard content. Automated smoke suites used
  independent isolated displays and checked exact child input bytes.
- Default shell/profile was preserved in the interactive Bash session. Layouts
  included 1050×780, 1050×600 and 760×500; the maximized smoke used 1440×900.
- These are agent-driven graphical checks, **not physical-keyboard, real IME,
  host Wayland, or human assistive-technology acceptance**.

## Results

| Area | Evidence |
| --- | --- |
| Shell input and cwd | Interactive composer Submit produced 250 numbered output lines; native terminal `cd` immediately updated Current shell directory. |
| Unknown REPL input | Python REPL: first Submit showed confirmation without sending text; confirmation delivered literal `print(...)`. Python's pasted-input continuation required a native terminal Enter to execute, yielding the expected single result. Cwd remained last reported, not current. |
| Long-output navigation | Interactive Oldest reached the retained beginning. Graphical smoke checked page navigation and preservation of existing selections without child input. |
| Local selection | Graphical smoke checked mouse reporting off/on, edge growth, stationary held-wheel extension, release and exact zero child bytes. |
| Frozen busy output | Composer smoke checked unchanged copied frame, continuing child ticks, zero pointer/keyboard leakage and Return live recovery, with Shift override disabled. |
| Frozen resize | Interactive shrinking reproduced a hidden-edge bug; corrected visible-edge drag grew copied text from 309 to 597 bytes, with stable text after release. Geometry tests cover hidden cells, partial cells, enlarged blank canvas and forwarded-gesture exclusion. |
| Settings recorder | Interactive Record displayed `alt-l` and Save persisted exactly `focus_editor = alt-l`. Component and maximized-dialog smoke cover capture/release/refocus, zero-width layout and persistence. |
| Guarded composer and tabs | Independent smoke checks literal text, explicit previews, protected interrupts, held-key confirmation, independent drafts, reorder/close cancellation and child cleanup. |
| History popup | Full smoke checks recall/undo/cancellation, pointer/keyboard handoff, confirmed filesystem Forget and native terminal Ctrl-R passthrough. |
| Persistence and replay | Full graphical smoke plus portable tests cover read-only history and replay; v1/v2/v3 format tests include maximum submission/output sizes and oversized-frame rejection. |

## Corrections and rejected leads

- Frozen snapshots intentionally retain their original grid when resized.
  The confirmed fault was using that hidden grid edge for dragging: after
  shrinking, a held drag at the new visible bottom did not scroll, whereas
  dragging into the composer to the old bottom did. The fix intersects canvas
  and grid for pointer mapping and edge thresholds; it does not reflow the frame.
- The old busy-output fixture selected its first row, which became an intentional
  edge-autoscroll gesture after combining the PRs. Its frame sample now sits on
  the third row. Stability, continued output, confirmation and child-byte
  assertions were preserved, not relaxed.
- A suspected maximum-submission serialization defect was disproved:
  `MAX_OUTPUT` is 1 MiB, not 64 KiB. New boundary tests pass against the original
  format implementation; no speculative serializer change was retained.
- One final smoke run selected only 40 lines before its deadline during concurrent
  release compilation. Three bounded diagnostic passes succeeded without load.
  Controlled 12-worker runs selected 52 and 47 lines before the old deadline;
  instrumentation showed correct coordinates, continued timer progress and no
  cancellation. Maximum-speed dragging plus a longer bounded observation reached
  the unchanged size threshold, then exposed a stale clipboard baseline at the
  wheel/navigation boundary. Copy-dispatch logging distinguished the phases:
  release stability passed at 63 lines, the wheel selection was 19 lines, but the
  test read the older 63-line clipboard value before wheel Copy completed. Fresh
  sentinels now cover wheel/frozen growth and release/stability copies; the smoke
  also waits for the painted drag indicator to acknowledge release before taking
  its baseline. The original size, continuity, release and zero-child-input
  assertions remain intact; temporary diagnostics and all speculative production
  timing changes were excluded.

## Outstanding acceptance, not claimed passed

Real Windows/macOS GUI; native Wayland/Mutter/fractional scaling; physical
keyboard/layout and AltGr; real IME preedit/candidate placement/commit; real
cross-application clipboard managers; screen readers and other assistive
technology; remote SSH/application-provider combinations; full OpenCode/Vim
acceptance; multi-monitor scaling and sustained human use remain unverified.

No new generic TUI document reconstruction, higher keyboard-protocol levels,
image protocols, signing or native installers are claimed. See the
[compatibility gate](terminal-compatibility-alpha.md),
[manual checklist](manual-testing.md) and [roadmap](roadmap.md).
