#!/usr/bin/env bash
set -Eeuo pipefail
mkdir -p smoke-artifacts
exec > >(tee -a smoke-artifacts/acceptance.log) 2>&1
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
printf 'theme = dark\nshow_blocks = true\n' > "$KEA_SETTINGS"
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
clipboard() { timeout 3s xclip -selection clipboard -t UTF8_STRING -o; }
put_clipboard() { printf '%s' "$1" | xclip -selection clipboard; sleep .15; }
assert_clipboard() { local actual; actual=$(clipboard); [[ "$actual" == "$1" ]] || { printf 'Expected <%s>; got <%s>\n' "$1" "$actual"; exit 1; }; }
focus_editor() { xdotool mousemove --window "$window" 120 "$((HEIGHT-170))" click 1; sleep .2; }

# A read-only recording has no child; chrome shortcuts can be used here.
./target/debug/kea --demo >smoke-artifacts/demo.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/demo.log
sleep 4
key F6 F6 F6 F10
clipboard >smoke-artifacts/history.txt
grep -q 'ERROR: connection failed' smoke-artifacts/history.txt
key F9
key ctrl+shift+space F10
clipboard >smoke-artifacts/latest.txt
grep -q 'Ready. The error has disappeared' smoke-artifacts/latest.txt
cleanup_app

# Capture actual bytes in a raw child before any editor interaction.
printf 'theme = dark\n' > "$KEA_SETTINGS"
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/keys.bin >smoke-artifacts/terminal.log 2>&1 & kea_pid=$!
wait_window smoke-artifacts/terminal.log
key space ctrl+c ctrl+v Tab F6 F7 F8 F9 F10 ctrl+shift+space
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/keys.bin').read_bytes()
expected = b' \x03\x16\t\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[21~\0'
assert actual == expected, (actual, expected)
PY
echo 'Actual Space/Ctrl/Tab/function-key delivery passed.'
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
# composer remains Ctrl+L from the previous daily-driver change. The terminal shortcut
# itself is not forwarded to the child.
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

# Users can explicitly choose the older terminal-after-submit policy.
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
assert actual.endswith(b'policy\rz'), actual
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
key ctrl+v F10
[[ -z "$(clipboard)" ]] || { echo 'Paste executed a command'; exit 1; }
key ctrl+Return
# Default post-submit focus is now the fresh composer, so no refocus click is needed.
found=''
for _ in $(seq 1 60); do
  key F10
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
xdotool type --clearmodifiers --delay 10 'printf first'
key Return
xdotool type --clearmodifiers --delay 10 'printf second'
key ctrl+a ctrl+c; assert_clipboard $'printf first\nprintf second'
key F10
clipboard >smoke-artifacts/after-newline.txt
[[ "$(grep -c '^exit ' smoke-artifacts/after-newline.txt)" -eq 1 ]]
import -window "$window" smoke-artifacts/document.png
cleanup_app
echo 'Passed: composer-first submission, keyboard focus switching, draft recall, raw terminal keys, editing/undo/Unicode, read-only output and replay.'
