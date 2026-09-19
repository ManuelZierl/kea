#!/usr/bin/env bash
set -Eeuo pipefail
export PYTHONDONTWRITEBYTECODE=1
mkdir -p smoke-artifacts
exec > >(tee -a smoke-artifacts/acceptance.log) 2>&1
# Fixed click positions require the same UI font as the fixture. Without Ubuntu,
# GPUI falls back to wider fonts and wrapped dialog text moves fields downward.
if [[ "$(fc-match -f '%{family}' Ubuntu)" != Ubuntu ]]; then
  echo 'The Linux smoke fixture requires the Ubuntu font (install fonts-ubuntu).' >&2
  exit 1
fi
trap 'printf "Smoke assertion failed at line %s: %s\n" "$LINENO" "$BASH_COMMAND" >&2' ERR
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export KEA_MEMORY_DIR="$XDG_RUNTIME_DIR/input-memory"
printf 'theme = dark\nshow_blocks = true\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
unset WAYLAND_DISPLAY
kea_pid=''; window=''
cleanup_app() { if [[ -n "$kea_pid" ]]; then kill "$kea_pid" 2>/dev/null || true; wait "$kea_pid" 2>/dev/null || true; kea_pid=''; fi; }
cleanup() { local code=$?; if [[ "$code" -ne 0 && -n "$window" ]]; then import -window "$window" smoke-artifacts/failure.png 2>/dev/null || true; fi; cleanup_app; rm -rf "$XDG_RUNTIME_DIR"; }
trap cleanup EXIT
wait_window() {
  for _ in $(seq 1 100); do
    kill -0 "$kea_pid" || { cat "$1"; exit 1; }
    window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    if [[ -n "$window" ]]; then
      xdotool windowfocus --sync "$window"
      eval "$(xdotool getwindowgeometry --shell "$window")"
      sleep 1
      return
    fi
    sleep .2
  done
  echo 'No Kea window'; exit 1
}
key() {
  # Let each chord finish dispatching before the next one. xdotool's default
  # 12ms between batched chords can overtake a focus/selection frame on CI.
  local chord
  for chord in "$@"; do
    xdotool key --clearmodifiers --delay 50 "$chord"
    sleep .15
  done
}
fkey() {
  local code
  case "$1" in
    F4) code=70 ;;
    F6) code=72 ;;
    F7) code=73 ;;
    F8) code=74 ;;
    F9) code=75 ;;
    F10) code=76 ;;
    *) echo "Unsupported function key: $1" >&2; return 1 ;;
  esac
  xdotool key --clearmodifiers "$code"
  sleep .15
}
clipboard() { timeout 3s xclip -selection clipboard -t UTF8_STRING -o; }
put_clipboard() { printf '%s' "$1" | xclip -selection clipboard; sleep .15; }
assert_clipboard() {
  local actual='(clipboard unavailable)'
  # Copy/selection ownership is asynchronous. Observe the result without
  # replaying input, and still fail if the exact expected text never arrives.
  for _ in $(seq 1 30); do
    if actual=$(timeout .5s xclip -selection clipboard -t UTF8_STRING -o 2>/dev/null) && [[ "$actual" == "$1" ]]; then
      return
    fi
    sleep .1
  done
  printf 'Expected <%s>; got <%s>\n' "$1" "$actual"
  exit 1
}
# The Composer button stays in the toolbar across resize; a bottom-relative
# canvas click can land in the terminal when the composer hits its minimum size.
focus_editor() { xdotool mousemove --window "$window" 130 51 click 1; sleep .2; }
# A dialog can start painting later on software-rendered/loaded runners. Wait
# for its header to change and finish animating before targeting its contents.
# The crop excludes the terminal and editor carets, which blink independently.
settings_header_frame() {
  import -silent -window "$window" -crop "720x100+$((WIDTH/2-360))+30" -depth 8 rgb:- | sha256sum
}
settings_transition() {
  local before previous current stable=0
  before=$(settings_header_frame)
  previous=$before
  xdotool mousemove --window "$window" "$1" "$2" click 1
  for _ in $(seq 1 50); do
    sleep .1
    kill -0 "$kea_pid"
    current=$(settings_header_frame)
    if [[ "$current" != "$before" && "$current" == "$previous" ]]; then
      stable=$((stable+1))
      if [[ "$stable" -ge 3 ]]; then return; fi
    else
      stable=0
    fi
    previous=$current
  done
  echo 'Settings header did not change and settle within the bounded wait.' >&2
  return 1
}

# A read-only recording has no child; chrome shortcuts can be used here.
./target/debug/kea --demo >smoke-artifacts/demo.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/demo.log
sleep 4
fkey F6
fkey F6
fkey F6
fkey F10
clipboard >smoke-artifacts/history.txt
grep -q 'ERROR: connection failed' smoke-artifacts/history.txt
fkey F9
key ctrl+shift+space
fkey F10
clipboard >smoke-artifacts/latest.txt
grep -q 'Ready. The error has disappeared' smoke-artifacts/latest.txt
cleanup_app

# A focus escape must switch both ways without reaching the child. An explicit
# default focus_terminal line must not change behavior. Reusing a formerly
# default action's chord must work too (old terminal NoAction masks broke this).
focus_case=0
for overrides in '' 'focus_terminal = ctrl-shift-l' $'focus_editor = ctrl-r\nreverse_search = alt-r'; do
  focus_case=$((focus_case+1))
  printf '%s\n' "$overrides" > "$KEA_KEYBINDINGS"
  switch=ctrl+l
  [[ "$focus_case" -ne 3 ]] || switch=ctrl+r
  ./target/debug/kea --direct -- python3 scripts/terminal-fixture.py "smoke-artifacts/focus-switch-$focus_case.bin" >smoke-artifacts/focus-switch.log 2>&1 & kea_pid=$!
  wait_window smoke-artifacts/focus-switch.log
  key "$switch"
  xdotool type --clearmodifiers 'focus-draft'
  key "$switch"
  xdotool type --clearmodifiers 't'
  # This composer-only alternative belongs to the child when terminal-focused.
  key ctrl+shift+l
  key "$switch"
  key ctrl+a ctrl+c
  assert_clipboard focus-draft
  python3 - "$focus_case" <<'PY'
import sys
from pathlib import Path
actual = Path(f'smoke-artifacts/focus-switch-{sys.argv[1]}.bin').read_bytes()
assert actual == b't\x0c', actual
PY
  cleanup_app
done
: > "$KEA_KEYBINDINGS"
echo 'Default, explicit-default and remapped bidirectional focus switch passed without child leakage.'

# Settings must render above the workspace, save through the app-owned path, and
# leave the current editor entity/draft intact while presentation changes live.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/settings-ui.bin >smoke-artifacts/settings-ui.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/settings-ui.log
focus_editor
xdotool type --clearmodifiers --delay 10 'settings-draft'
settings_transition "$((WIDTH-214))" 51
xdotool mousemove --window "$window" "$((WIDTH/2+232))" 274 click 1
sleep .5
grep -q '^theme = light$' "$KEA_SETTINGS"
import -window "$window" smoke-artifacts/settings-light.png
# Keybindings use the component editor, save a validated snapshot, and remain
# restart-scoped. Closing/reopening must show saved values without losing draft.
settings_transition "$((WIDTH/2-200))" 95
xdotool mousemove --window "$window" "$((WIDTH/2))" 404 click 1
key ctrl+a
xdotool type --clearmodifiers 'alt-l'
xdotool mousemove --window "$window" "$((WIDTH/2))" 311 click 1
sleep .3
grep -q '^focus_editor = alt-l$' "$KEA_KEYBINDINGS"
import -window "$window" smoke-artifacts/keybindings-saved.png
key Escape
import -window "$window" smoke-artifacts/settings-draft-preserved.png
focus_editor
key ctrl+a ctrl+c
assert_clipboard settings-draft
# An already-running app retains its current keymap until restart.
key ctrl+l
xdotool type --clearmodifiers 't'
key ctrl+l
key ctrl+a ctrl+c
assert_clipboard settings-draft
python3 - <<'PY'
from pathlib import Path
assert Path('smoke-artifacts/settings-ui.bin').read_bytes() == b't'
PY
settings_transition "$((WIDTH-214))" 51
settings_transition "$((WIDTH/2-200))" 95
xdotool mousemove --window "$window" "$((WIDTH/2))" 404 click 1
key ctrl+a ctrl+c
assert_clipboard alt-l
xdotool windowsize --sync "$window" 760 500
sleep .3
xdotool mousemove --window "$window" 650 360 click --repeat 100 --delay 5 5
import -window "$window" smoke-artifacts/keybindings-small-bottom.png
xdotool mousemove --window "$window" 80 95 click 1
sleep .3
xdotool mousemove --window "$window" 650 360 click --repeat 100 --delay 5 5
import -window "$window" smoke-artifacts/settings-small-bottom.png
# Inspect these screenshots for clipping/overflow; they are visual evidence,
# not an assertion that a particular theme's background has a fixed color.
cleanup_app
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/settings-restarted.bin >smoke-artifacts/settings-restarted.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/settings-restarted.log
key alt+l
xdotool type --clearmodifiers 'restarted-draft'
key alt+l
xdotool type --clearmodifiers 't'
key alt+l ctrl+a ctrl+c
assert_clipboard restarted-draft
python3 - <<'PY'
from pathlib import Path
assert Path('smoke-artifacts/settings-restarted.bin').read_bytes() == b't'
PY
cleanup_app
: > "$KEA_KEYBINDINGS"
echo 'Keybinding settings save/reopen/restart and preserved draft passed; small-dialog screenshots captured.'

# Unified submission: unknown receivers require explicit confirmation. A held
# first chord must not send, and cancelling input must remain in the draft.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/submit-guard.bin >smoke-artifacts/submit-guard.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/submit-guard.log
focus_editor
xdotool type --clearmodifiers --delay 10 'guarded'
xdotool keydown Control_L keydown Return
sleep .8
[[ ! -s smoke-artifacts/submit-guard.bin ]] || { echo 'Held submission bypassed confirmation'; exit 1; }
xdotool keyup Return keyup Control_L
sleep .2
xdotool type --clearmodifiers 'x'
key ctrl+a ctrl+c
assert_clipboard guardedx
[[ ! -s smoke-artifacts/submit-guard.bin ]] || { echo 'Cancelling key reached the terminal'; exit 1; }
key End ctrl+Return Return
python3 - <<'CHECK'
from scripts.smoke_assert import assert_output_suffix
assert_output_suffix('smoke-artifacts/submit-guard.bin', b'\x1b[200~guardedx\x1b[201~\r')
CHECK
cleanup_app

# U2: real shell metadata, terminal type/delete recovery context, and actual
# composer popup dispatch. Clipboard assertions observe the editor's real value.
mkdir -p "$XDG_RUNTIME_DIR/completion"
touch "$XDG_RUNTIME_DIR/completion/cab" "$XDG_RUNTIME_DIR/completion/café"
printf 'theme = dark\n' > "$KEA_SETTINGS"
HISTFILE="$XDG_RUNTIME_DIR/bash-history" ./target/debug/kea --terminal-focus -- bash --noprofile --norc >smoke-artifacts/completion-ui.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/completion-ui.log
xdotool type --clearmodifiers "cd '$XDG_RUNTIME_DIR/completion'"
key Return
sleep .5
xdotool type --clearmodifiers 'abc'
key BackSpace BackSpace BackSpace ctrl+c
sleep .3
key ctrl+l
put_clipboard 'echo 😀 ca suffix'
key ctrl+v Home
xdotool key --clearmodifiers --repeat 9 --delay 60 Right
key Tab
sleep .5
import -window "$window" smoke-artifacts/completion-keyboard.png
# cab is first, café second. Right navigates the horizontal menu, not the caret.
key Right Return ctrl+a ctrl+c
assert_clipboard 'echo 😀 café suffix'
key ctrl+z ctrl+a ctrl+c
assert_clipboard 'echo 😀 ca suffix'
key End
xdotool key --clearmodifiers --repeat 7 --delay 60 Left
key Tab
sleep .5
key Right Left Return ctrl+a ctrl+c
assert_clipboard 'echo 😀 cab suffix'
# Escape cancels; a later Return belongs to the editor and inserts a newline.
key ctrl+a
put_clipboard 'echo ca'
key ctrl+v Tab
sleep .5
key Escape Return
xdotool type --clearmodifiers 'after'
key ctrl+a ctrl+c
assert_clipboard $'echo ca\nafter'
# Text and cursor changes invalidate candidates, as does a focus round trip.
key ctrl+a
put_clipboard 'echo ca'
key ctrl+v Tab
sleep .5
key Escape Left Return
xdotool type --clearmodifiers 'x'
key ctrl+a ctrl+c
assert_clipboard $'echo c\nxa'
key ctrl+a
put_clipboard 'echo ca'
key ctrl+v Tab
sleep .5
key ctrl+l ctrl+l Return
xdotool type --clearmodifiers 'x'
key ctrl+a ctrl+c
assert_clipboard $'echo ca\nx'
# Mouse choice remains available, without moving editor focus out of the popup.
key ctrl+a
put_clipboard 'echo ca'
key ctrl+v Tab
sleep .5
xdotool mousemove --window "$window" 150 "$((HEIGHT-44))" click 1
key ctrl+a ctrl+c
assert_clipboard 'echo café'
# Long candidates wrap beyond the popup viewport: keyboard selection must scroll.
for n in $(seq -w 1 12); do
  touch "$XDG_RUNTIME_DIR/completion/long_${n}_abcdefghijklmnopqrstuvwxyz_abcdefghijklmnopqrstuvwxyz"
done
key ctrl+a
put_clipboard 'echo long_'
key ctrl+v Tab
sleep .5
key Up
import -window "$window" smoke-artifacts/completion-scroll-last.png
key Return ctrl+a ctrl+c
assert_clipboard 'echo long_12_abcdefghijklmnopqrstuvwxyz_abcdefghijklmnopqrstuvwxyz'
cleanup_app
echo 'U2 keyboard completion, Unicode replacement/undo, explicit prompt recovery, Escape and stale cursor/focus passed.'

# A configured Enter-to-run policy must accept an open completion first.
printf 'run_shell = enter\nnewline = shift-enter\n' > "$KEA_KEYBINDINGS"
HISTFILE="$XDG_RUNTIME_DIR/bash-history" ./target/debug/kea --terminal-focus -- bash --noprofile --norc >smoke-artifacts/completion-enter.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/completion-enter.log
xdotool type --clearmodifiers "cd '$XDG_RUNTIME_DIR/completion'"
key Return
sleep .5
key ctrl+l
put_clipboard 'echo ca'
key ctrl+v Tab
sleep .5
key Right Return ctrl+a ctrl+c
assert_clipboard 'echo café'
cleanup_app
: > "$KEA_KEYBINDINGS"
echo 'Completion mouse selection, scrolling and configured Enter-to-run acceptance passed.'

# Ctrl-R recalls into the draft without sending another byte to the child.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/reverse-search.bin >smoke-artifacts/reverse-search.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/reverse-search.log
key ctrl+r
focus_editor
xdotool type --clearmodifiers --delay 10 'reverse-search-probe'
key ctrl+shift+Return Return
sleep .3
key ctrl+r
sleep .5
import -window "$window" smoke-artifacts/reverse-search.png
key Return
key ctrl+a ctrl+c
assert_clipboard 'reverse-search-probe'
key ctrl+z
key ctrl+a
put_clipboard 'unchanged-empty-draft'
key ctrl+c
assert_clipboard 'unchanged-empty-draft'
xdotool type --clearmodifiers --delay 10 'scratch-draft'
key ctrl+r
sleep .3
key ctrl+a
xdotool type --clearmodifiers --delay 10 'different-query'
key Escape
key ctrl+a ctrl+c
assert_clipboard 'scratch-draft'
# Save a memory, mix mouse and keyboard navigation, then forget via confirmation.
key ctrl+r ctrl+s
xdotool type --clearmodifiers --delay 10 'Smoke memory'
key Return
sleep .5
shopt -s nullglob
memory_files=("$KEA_MEMORY_DIR"/*.kmem)
[[ ${#memory_files[@]} -eq 1 ]] || { echo 'Named memory was not persisted'; exit 1; }
key ctrl+a BackSpace
sleep .4
# The last result is history; Up must take selection back to the saved memory
# even though the pointer remains over the historical row.
xdotool mousemove --window "$window" 90 "$((HEIGHT-303))"
sleep .2
key Up
import -window "$window" smoke-artifacts/reverse-search-mouse-keyboard.png
key Return
key ctrl+a ctrl+c
assert_clipboard 'scratch-draft'
key ctrl+a BackSpace
key ctrl+r
sleep .4
# Open Actions with the mouse, select Forget by keyboard, then confirm by mouse.
xdotool mousemove --window "$window" 590 "$((HEIGHT-270))" click 1
sleep .3
key Up Return
import -window "$window" smoke-artifacts/reverse-search-forget-confirm.png
xdotool mousemove --window "$window" 328 "$((HEIGHT-270))" click 1
for _ in $(seq 1 40); do
  memory_files=("$KEA_MEMORY_DIR"/*.kmem)
  [[ ${#memory_files[@]} -eq 0 ]] && break
  sleep .05
done
[[ ${#memory_files[@]} -eq 0 ]] || { echo 'Confirmed Forget did not delete the saved memory'; exit 1; }
shopt -u nullglob
sleep .3
import -window "$window" smoke-artifacts/reverse-search-forgotten.png
key Escape
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/reverse-search.bin').read_bytes()
assert actual == b'\x12\x1b[200~reverse-search-probe\x1b[201~\r', actual
PY
echo 'Reverse-search recall, undo, cancellation, mouse/keyboard handoff, confirmed Forget and terminal Ctrl-R passthrough passed.'
cleanup_app

# Capture actual bytes in a raw child before any editor interaction.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/keys.bin >smoke-artifacts/terminal.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/terminal.log
key space ctrl+c ctrl+v Return Tab
fkey F6
fkey F7
fkey F8
fkey F9
fkey F10
key ctrl+shift+space
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/keys.bin').read_bytes()
expected = b' \x03\x16\r\t\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[21~\0'
assert actual == expected, (actual, expected)
PY
echo 'Actual Space/Ctrl/Enter/Tab/function-key delivery passed.'
# Supported minimum width: a long status message must remain inside the fixed
# status row instead of wrapping character-by-character over the composer.
xdotool windowsize --sync "$window" 760 500
eval "$(xdotool getwindowgeometry --shell "$window")"
focus_editor
xdotool type --clearmodifiers --delay 10 'layout probe'
key ctrl+Return
key ctrl+a ctrl+c
assert_clipboard 'layout probe'
import -window "$window" smoke-artifacts/narrow-layout.png
export STATUS_OVERFLOW_MEAN="$(convert smoke-artifacts/narrow-layout.png -crop "${WIDTH}x40+0+$((HEIGHT-68))" -colorspace gray -threshold 30% -format '%[fx:mean]' info:)"
python3 - <<'PY'
import os
mean = float(os.environ['STATUS_OVERFLOW_MEAN'])
assert mean == 0.0, f'status text escaped its row: crop mean={mean}'
PY
key ctrl+a BackSpace
xdotool windowsize --sync "$window" 1050 780
eval "$(xdotool getwindowgeometry --shell "$window")"
sleep .3
focus_editor
import -window "$window" smoke-artifacts/terminal-editor-focus.png
put_clipboard $'message one\nmessage two'
key ctrl+v ctrl+shift+Return Return
python3 - <<'PY'
from scripts.smoke_assert import assert_output_suffix
assert_output_suffix('smoke-artifacts/keys.bin', b'\x1b[200~message one\nmessage two\x1b[201~\r')
PY

# Composer-first default: successful submission leaves the fresh editor ready without
# requiring a mouse or a second focus shortcut.
xdotool type --clearmodifiers --delay 10 'scratch'
key ctrl+a ctrl+c
assert_clipboard scratch

# Symmetric keyboard focus: composer -> terminal uses Ctrl+Shift+L, while terminal ->
# composer remains Ctrl+L. The terminal shortcut itself is not forwarded to the child.
key ctrl+shift+l
xdotool type --clearmodifiers --delay 10 'x'
python3 - <<'PY'
from scripts.smoke_assert import assert_output_suffix
assert_output_suffix('smoke-artifacts/keys.bin', b'\rx')
PY
key ctrl+l

# Exact application submission remains recoverable while an in-progress scratch draft
# survives backwards/forwards history navigation.
key ctrl+Up ctrl+a ctrl+c
assert_clipboard $'message one\nmessage two'
key ctrl+Down ctrl+a ctrl+c
assert_clipboard scratch
import -window "$window" smoke-artifacts/terminal-and-editor.png
cleanup_app

# A TUI that negotiates SGR drag mouse reporting gets press/motion/release instead of
# Kea swallowing the click. Modified Enter becomes CSI-u only after negotiation.
printf 'theme = dark\npost_submit_focus = terminal\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-compat-fixture.py smoke-artifacts/compat.bin >smoke-artifacts/compat.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/compat.log
xdotool mousemove --window "$window" 220 160
xdotool mousedown 1
xdotool mousemove --window "$window" 260 170
xdotool mouseup 1
key Return shift+Return
sleep .3
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/compat.bin').read_bytes()
assert b'\x1b[<0;' in actual and b'M' in actual, actual
assert b'\x1b[<32;' in actual, actual
assert b'\x1b[<0;' in actual and b'm' in actual, actual
assert b'\x1b[13;2u' in actual, actual
assert b'\r' in actual, actual
PY
cleanup_app

# Manual acceptance F3: without a negotiated keyboard protocol, modified Enter
# must collapse to classic CR instead of leaking CSI-u or vanishing.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/classic-enter.bin >smoke-artifacts/classic-enter.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/classic-enter.log
key Return shift+Return
sleep .3
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/classic-enter.bin').read_bytes()
assert actual == b'\r\r', actual
assert b'13;2u' not in actual, actual
PY
cleanup_app
printf 'theme = dark\npost_submit_focus = terminal\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/terminal-focus-policy.bin >smoke-artifacts/terminal-policy.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/terminal-policy.log
focus_editor
xdotool type --clearmodifiers --delay 10 'policy'
key ctrl+shift+Return Return
xdotool type --clearmodifiers --delay 10 'z'
python3 - <<'PY'
from scripts.smoke_assert import assert_output_suffix
assert_output_suffix('smoke-artifacts/terminal-focus-policy.bin', b'\x1b[200~policy\x1b[201~\rz')
PY
cleanup_app

# Explicit terminal text selection from the composer must create local ownership
# without changing the child's input stream. Keyboard extension selects the
# stable first output line, so clipboard content proves this is real app text,
# not a local action that merely reports success.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-selection-fixture.py smoke-artifacts/selection.bin >smoke-artifacts/selection.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/selection.log
focus_editor
fkey F4
key Home shift+End ctrl+c
selection_before=$(wc -c < smoke-artifacts/selection.bin)
selection_clipboard=$(clipboard)
[[ "$selection_clipboard" == '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ' ]] || {
  printf 'Expected fixture text in terminal selection; got <%s>\n' "$selection_clipboard"
  exit 1
}
# Navigation and the first Escape remain local. The second Escape is forwarded.
key Left Escape
[[ "$(wc -c < smoke-artifacts/selection.bin)" -eq "$selection_before" ]] || {
  echo 'Local navigation or first Escape reached the child'
  exit 1
}
key Escape
python3 - <<'PY'
from scripts.smoke_assert import assert_output_suffix
assert_output_suffix('smoke-artifacts/selection.bin', b'\x1b')
PY
# Once local selection is active, printable input exits local ownership and is
# forwarded exactly once rather than being consumed by the selection handler.
key ctrl+l
fkey F4
key Home shift+End
selection_before=$(wc -c < smoke-artifacts/selection.bin)
xdotool type --clearmodifiers q
sleep .2
export SELECTION_BEFORE="$selection_before"
python3 - <<'PY'
import os
from pathlib import Path
before = int(os.environ['SELECTION_BEFORE'])
actual = Path('smoke-artifacts/selection.bin').read_bytes()
assert actual[before:] == b'q', actual[before:]
PY
# With reporting off, Alt drag creates a column selection. Identical fixture
# rows make equal copied lines distinguish a rectangle from a linear range.
selection_before=$(wc -c < smoke-artifacts/selection.bin)
put_clipboard selection-sentinel
xdotool keydown Alt
xdotool mousemove --window "$window" 35 160
xdotool mousedown 1
xdotool mousemove --window "$window" 150 180
xdotool mouseup 1
xdotool keyup Alt
key ctrl+c
export ALT_SELECTION="$(clipboard)"
python3 - <<'PY'
import os
rows = os.environ['ALT_SELECTION'].splitlines()
assert len(rows) >= 2 and rows[0] and len(set(rows)) == 1, rows
assert rows[0] in '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ', rows
PY
[[ "$(wc -c < smoke-artifacts/selection.bin)" -eq "$selection_before" ]] || {
  echo 'Alt local selection reached the child'
  exit 1
}
cleanup_app

# With mouse reporting active, the default Shift gesture is wholly local and
# emits no partial press/release to the child. Disabling the policy forwards the
# same gesture with the current Shift modifier encoded in both SGR events.
printf 'theme = dark\nshift_mouse_selects_locally = true\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-selection-fixture.py smoke-artifacts/selection-default.bin --motion >smoke-artifacts/selection-default.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/selection-default.log
selection_before=$(wc -c < smoke-artifacts/selection-default.bin)
xdotool keydown Shift
xdotool mousemove --window "$window" 25 160
xdotool mousedown 1
xdotool mousemove --window "$window" 190 160
xdotool keyup Shift
xdotool mousemove --window "$window" 200 160
xdotool mouseup 1
[[ "$(wc -c < smoke-artifacts/selection-default.bin)" -eq "$selection_before" ]] || {
  echo 'Default Shift drag reached the mouse-reporting child'
  exit 1
}
key ctrl+c
default_clipboard=$(clipboard)
[[ -n "$default_clipboard" && '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ' == *"$default_clipboard"* ]] || {
  printf 'Expected local copy after default Shift drag; got <%s>\n' "$default_clipboard"
  exit 1
}
# Regression: a mouse-created range must own both Copy and Shift+arrow, not
# merely explicit F4 selections. Check clipboard growth and the actual PTY log.
key shift+Right ctrl+c
export SELECTION_BEFORE_TEXT="$default_clipboard"
export SELECTION_AFTER_TEXT="$(clipboard)"
python3 - <<'PY'
import os
from pathlib import Path
before, after = os.environ['SELECTION_BEFORE_TEXT'], os.environ['SELECTION_AFTER_TEXT']
assert after.startswith(before) and len(after) == len(before) + 1, (before, after)
assert Path('smoke-artifacts/selection-default.bin').read_bytes() == b''
PY
# Keyboard navigation promotes the range to explicit local interaction. Exit it
# before exercising a new child-owned gesture.
key Escape
# Adding Shift after a child-owned press cannot turn its remaining events local.
xdotool mousemove --window "$window" 25 160
sleep .1
export SELECTION_BEFORE="$(wc -c < smoke-artifacts/selection-default.bin)"
xdotool mousedown 1
xdotool keydown Shift
xdotool mousemove --window "$window" 190 160 mouseup 1
xdotool keyup Shift
sleep .2
python3 - <<'PY'
import os
import re
from pathlib import Path
actual = Path('smoke-artifacts/selection-default.bin').read_bytes()[int(os.environ['SELECTION_BEFORE']):]
reports = re.findall(rb'\x1b\[<(\d+);\d+;\d+([Mm])', actual)
assert reports[0] == (b'0', b'M') and reports[-1] == (b'4', b'm'), actual
assert (b'36', b'M') in reports, actual
PY
cleanup_app

printf 'theme = dark\nshift_mouse_selects_locally = false\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-selection-fixture.py smoke-artifacts/selection-forwarded.bin --motion >smoke-artifacts/selection-forwarded.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/selection-forwarded.log
xdotool keydown Shift
xdotool mousemove --window "$window" 25 160
xdotool mousedown 1
xdotool mousemove --window "$window" 190 160
xdotool mouseup 1
xdotool keyup Shift
sleep .2
python3 - <<'PY'
from pathlib import Path
import re
actual = Path('smoke-artifacts/selection-forwarded.bin').read_bytes()
reports = re.findall(rb'\x1b\[<(\d+);\d+;\d+([Mm])', actual)
assert reports[0] == (b'39', b'M'), actual
assert (b'4', b'M') in reports and reports[-1] == (b'4', b'm'), actual
assert (b'36', b'M') in reports, actual
PY
cleanup_app

printf 'theme = dark\nshow_blocks = true\n' > "$KEA_SETTINGS"
./target/debug/kea -- bash --noprofile --norc >smoke-artifacts/document.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/document.log
focus_editor
xdotool type --clearmodifiers --delay 20 'abcXYZ'
sleep 1.2
key End shift+Left shift+Left shift+Left ctrl+c
assert_clipboard XYZ
key ctrl+x ctrl+a ctrl+c; assert_clipboard abc
key ctrl+z ctrl+a ctrl+c; assert_clipboard abcXYZ
key ctrl+shift+z ctrl+a ctrl+c; assert_clipboard abc
key ctrl+a BackSpace
put_clipboard $'printf "kea_doc_one ä\\n";\nprintf "kea_doc_two\\n"'
key ctrl+v
fkey F10
[[ -z "$(clipboard)" ]] || { echo 'Paste executed a command'; exit 1; }
key ctrl+Return
# Default post-submit focus is the fresh composer, so no refocus click is needed.
found=''
for _ in $(seq 1 60); do
  fkey F10
  clipboard >smoke-artifacts/document.txt
  if grep -Fq 'kea_doc_one ä' smoke-artifacts/document.txt && grep -Fq 'exit 0' smoke-artifacts/document.txt; then found=1; break; fi
  sleep .1
done
[[ -n "$found" ]] || { cat smoke-artifacts/document.log; exit 1; }
put_clipboard KEEP
key ctrl+z ctrl+a ctrl+c; assert_clipboard KEEP
xdotool mousemove --window "$window" "$((WIDTH-320))" 200 click 1
sleep .3
key ctrl+a ctrl+c
clipboard >smoke-artifacts/selected-block.txt
grep -Fq 'kea_doc_one' smoke-artifacts/selected-block.txt
key BackSpace ctrl+x ctrl+v ctrl+a ctrl+c
clipboard >smoke-artifacts/selected-block-after.txt
cmp smoke-artifacts/selected-block.txt smoke-artifacts/selected-block-after.txt
focus_editor
import -window "$window" smoke-artifacts/editor-focus-after-block.png
# The composer editor must consume the panel's available height. Enter enough
# lines to expose the old 72 px collapse, then click the first visible row: it
# must still be LINE-1 rather than a prematurely scrolled later line.
xdotool type --clearmodifiers --delay 10 'LINE-1'
for line in 2 3 4 5 6; do
  key Return
  xdotool type --clearmodifiers --delay 10 "LINE-$line"
done
xdotool mousemove --window "$window" 85 "$((HEIGHT-170))" click 1
key Home shift+End ctrl+c
assert_clipboard LINE-1
key ctrl+a BackSpace
xdotool type --clearmodifiers --delay 10 'printf first'
key Return
xdotool type --clearmodifiers --delay 10 'printf second'
key ctrl+a ctrl+c; assert_clipboard $'printf first\nprintf second'
fkey F10
clipboard >smoke-artifacts/after-newline.txt
[[ "$(grep -c '^exit ' smoke-artifacts/after-newline.txt)" -eq 1 ]]
import -window "$window" smoke-artifacts/document.png
cleanup_app
echo 'Passed: composer workflow, negotiated mouse + keyboard input, draft recall, raw terminal keys, editing/undo/Unicode, read-only output and replay.'
