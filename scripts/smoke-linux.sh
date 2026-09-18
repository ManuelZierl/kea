#!/usr/bin/env bash
set -Eeuo pipefail
mkdir -p smoke-artifacts
exec > >(tee -a smoke-artifacts/acceptance.log) 2>&1
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
key() { xdotool key --clearmodifiers "$@"; sleep .15; }
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
assert_clipboard() { local actual; actual=$(clipboard); [[ "$actual" == "$1" ]] || { printf 'Expected <%s>; got <%s>\n' "$1" "$actual"; exit 1; }; }
focus_editor() { xdotool mousemove --window "$window" 120 "$((HEIGHT-170))" click 1; sleep .2; }

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

# Settings must render above the workspace, save through the app-owned path, and
# leave the current editor entity/draft intact while presentation changes live.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/settings-ui.bin >smoke-artifacts/settings-ui.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/settings-ui.log
focus_editor
xdotool type --clearmodifiers --delay 10 'settings-draft'
xdotool mousemove --window "$window" "$((WIDTH-214))" 51 click 1
sleep .5
xdotool mousemove --window "$window" "$((WIDTH/2+217))" 332 click 1
sleep .5
grep -q '^theme = light$' "$KEA_SETTINGS"
import -window "$window" smoke-artifacts/settings-light.png
key Escape
import -window "$window" smoke-artifacts/settings-draft-preserved.png
cleanup_app

# Ctrl-R recalls into the draft without sending another byte to the child.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/reverse-search.bin >smoke-artifacts/reverse-search.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/reverse-search.log
key ctrl+r
focus_editor
xdotool type --clearmodifiers --delay 10 'reverse-search-probe'
key ctrl+shift+Return
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
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/reverse-search.bin').read_bytes()
assert actual == b'\x12\x1b[200~reverse-search-probe\x1b[201~\r', actual
PY
echo 'Reverse-search recall, undo, cancellation and terminal Ctrl-R passthrough passed.'
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
key ctrl+v ctrl+shift+Return
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/keys.bin').read_bytes()
assert actual.endswith(b'\x1b[200~message one\nmessage two\x1b[201~\r'), actual
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
from pathlib import Path
actual = Path('smoke-artifacts/keys.bin').read_bytes()
assert actual.endswith(b'\r' + b'x'), actual
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
key ctrl+shift+Return
xdotool type --clearmodifiers --delay 10 'z'
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/terminal-focus-policy.bin').read_bytes()
assert actual.endswith(b'\x1b[200~policy\x1b[201~\rz'), actual
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
from pathlib import Path
actual = Path('smoke-artifacts/selection.bin').read_bytes()
assert actual.endswith(b'\x1b'), actual
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
