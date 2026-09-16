# Terminal compatibility gate for alpha

Kea should improve input around an existing terminal application without making common terminal interaction unreliable. Alpha is blocked on the compatibility paths below.

## Required behavior

| Area | Alpha contract |
| --- | --- |
| Normal keys | Printable/composed text, Ctrl combinations, Tab/Shift+Tab, arrows, Home/End and F1–F24 reach the child while terminal focus owns input. |
| IME / Unicode | Platform composition is committed before bytes are sent; confirming an IME candidate never triggers a Kea action. |
| AltGr | Completed text wins over treating Ctrl+Alt as a control chord. |
| Paste | Bracketed paste is honored. Multiline application send refuses when the child has not enabled bracketed paste. |
| Keyboard extensions | Kea enables Alacritty's Kitty keyboard negotiation support, but classic terminal encoding remains the fallback. Modified Enter uses CSI-u only after the child negotiated disambiguated keyboard input. |
| Mouse wheel | Legacy, UTF-8 and SGR wheel reports are forwarded when mouse reporting is active. Shift+wheel is always local scrollback. |
| Mouse buttons | Primary press/release is forwarded using the child's negotiated Legacy/UTF-8/SGR encoding. |
| Mouse drag/motion | Motion is forwarded only when Alacritty reports DECSET 1002/1003. Kea does not infer tracking mode from screen text. |
| Local selection | Shift+drag always selects locally even while the child has mouse reporting enabled. |
| Scrollback | Reading old output does not pause the PTY or force the viewport back to the tail on new output. |
| Shell semantics | Kea's shell instrumentation does not change the observable exit status or cwd semantics of editor-run commands. |

## Representative alpha matrix

Before tagging alpha, manually exercise at least:

- Bash and PowerShell: direct typing, Ctrl+C, cwd changes, editor-run multiline commands and `$?`/`$LASTEXITCODE` behavior.
- OpenCode: composer send, Shift+Enter/modified Enter where negotiated, mouse selection/clicking, scrolling, interruption, terminal↔composer keyboard switching, and draft recall.
- Vim or Neovim: alternate screen, arrows/function keys, mouse click/drag when enabled, and clean return to the shell.
- SSH: local Kea composer can send to the remote application literally; Kea must not claim a remote cwd it cannot know.
- One REPL: native Tab/application input remains authoritative while editor completion stays local to the integrated shell.
- German/AltGr layout plus one real IME path on each release OS where available.

## Automated evidence

`terminal_mouse` unit tests cover protocol bytes for SGR/UTF-8/legacy input. `input` tests distinguish classic from negotiated modified Enter. The Linux graphical smoke fixture negotiates SGR drag reporting + Kitty level-1 keyboard input and verifies press, drag, release and Shift+Enter bytes reach the PTY.

## Explicitly not claimed for the first alpha

Image protocols, sixel/graphics rendering, full Kitty higher-level key release/repeat reporting, hyperlinks/actions, and screen-reader certification are separate compatibility milestones. Unsupported advanced protocols must fail by omission rather than corrupting normal terminal input.
