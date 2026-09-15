#!/usr/bin/env bash
# Real GPUI application under isolated X11/DBus; this is not OS IME certification.
set -Eeuo pipefail
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
printf 'theme = dark\n' > "$KEA_SETTINGS"
unset WAYLAND_DISPLAY
kea_pid=''
window=''
phase='initialization'
cleanup_app() {
  if [[ -n "$kea_pid" ]]; then kill "$kea_pid" 2>/dev/null || true; wait "$kea_pid" 2>/dev/null || true; kea_pid=''; fi
}
cleanup() {
  local status=$?
  set +e
  if (( status != 0 )); then
    printf 'Failed during %s (exit %s)\n' "$phase" "$status" | tee smoke-artifacts/failure.txt
    [[ -z "$window" ]] || import -window "$window" smoke-artifacts/failure.png
  fi
  cleanup_app
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
wait_for_window() {
  for _ in $(seq 1 80); do
    kill -0 "$kea_pid" || { cat "$1"; exit 1; }
    local found
    found=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    if [[ -n "$found" ]]; then printf '%s' "$found"; return 0; fi
    sleep 0.25
  done
  cat "$1"; echo 'Kea window did not appear'; exit 1
}
wait_for_exit() {
  for _ in $(seq 1 50); do
    if ! kill -0 "$kea_pid" 2>/dev/null; then wait "$kea_pid"; kea_pid=''; window=''; return; fi
    sleep 0.1
  done
  echo 'Kea did not quit'; exit 1
}
key() { printf 'key %s\n' "$*" >>smoke-artifacts/input.log; xdotool key --clearmodifiers "$@"; sleep 0.2; }
clipboard() { timeout 3s xclip -selection clipboard -o; }
put_clipboard() { printf '%s' "$1" | xclip -selection clipboard; sleep 0.1; }
assert_clipboard() {
  local actual
  actual=$(clipboard)
  if [[ "$actual" != "$1" ]]; then printf 'Clipboard mismatch: expected <%s>, got <%s>\n' "$1" "$actual"; exit 1; fi
}
phase='terminal history'
./target/debug/kea --demo >smoke-artifacts/demo.log 2>&1 &
kea_pid=$!; window=$(wait_for_window smoke-artifacts/demo.log)
xdotool windowfocus --sync "$window"
sleep 5
key F6 F6 F6
key F10
clipboard >smoke-artifacts/history.txt
grep -q 'ERROR: connection failed' smoke-artifacts/history.txt
import -window "$window" smoke-artifacts/history.png
key F9 F10
clipboard >smoke-artifacts/latest.txt
grep -q 'Ready. The error has disappeared' smoke-artifacts/latest.txt
! grep -q 'ERROR: connection failed' smoke-artifacts/latest.txt
key ctrl+shift+q; wait_for_exit

phase='editor selection-copy'
./target/debug/kea -- bash --noprofile --norc >smoke-artifacts/document.log 2>&1 &
kea_pid=$!; window=$(wait_for_window smoke-artifacts/document.log)
xdotool windowfocus --sync "$window"
sleep 1
put_clipboard UNCHANGED
xdotool type --clearmodifiers --delay 20 'abcXYZ'
sleep 1.2
import -window "$window" smoke-artifacts/typed.png
key End shift+Left shift+Left shift+Left
import -window "$window" smoke-artifacts/selection.png
key ctrl+c
assert_clipboard 'XYZ'
phase='editor cut and undo'
key ctrl+x
key ctrl+a ctrl+c
assert_clipboard 'abc'
key ctrl+z ctrl+a ctrl+c
assert_clipboard 'abcXYZ'
key ctrl+shift+z ctrl+a ctrl+c
assert_clipboard 'abc'
phase='multiline Unicode paste and execution'
key ctrl+a BackSpace
put_clipboard $'printf "kea_doc_one ä\\n";\nprintf "kea_doc_two\\n"'
key ctrl+v
key F10
[[ -z "$(clipboard)" ]] || { echo 'Paste unexpectedly created an execution'; exit 1; }
key ctrl+Return
found=''
for _ in $(seq 1 50); do
  key F10
  clipboard >smoke-artifacts/document.txt
  if grep -Fq 'kea_doc_one ä' smoke-artifacts/document.txt && grep -Fq 'exit 0' smoke-artifacts/document.txt; then found=1; break; fi
  sleep 0.1
done
[[ -n "$found" ]] || { cat smoke-artifacts/document.log; cat smoke-artifacts/document.txt; exit 1; }
phase='fresh draft undo history'
put_clipboard KEEP
key ctrl+z ctrl+a ctrl+c
assert_clipboard KEEP
phase='read-only output'
import -window "$window" smoke-artifacts/before-output-selection.png
xdotool mousemove --window "$window" 140 200 click 1
sleep 0.2
key ctrl+a ctrl+c
clipboard >smoke-artifacts/selected-block.txt
grep -Fq 'kea_doc_one' smoke-artifacts/selected-block.txt
key BackSpace ctrl+x ctrl+v
key ctrl+a ctrl+c
clipboard >smoke-artifacts/selected-block-after.txt
cmp smoke-artifacts/selected-block.txt smoke-artifacts/selected-block-after.txt
phase='Enter remains a newline'
key ctrl+l
xdotool type --clearmodifiers --delay 10 'printf first'
key Return
xdotool type --clearmodifiers --delay 10 'printf second'
key ctrl+a ctrl+c
assert_clipboard $'printf first\nprintf second'
key F10
clipboard >smoke-artifacts/document-after-edit.txt
[[ "$(grep -c '^exit ' smoke-artifacts/document-after-edit.txt)" -eq 1 ]]
import -window "$window" smoke-artifacts/document.png
key ctrl+shift+q; wait_for_exit

echo 'Desktop smoke passed: selection, clipboard, cut, undo/redo, Unicode multiline draft, explicit execute, read-only output, fresh-draft history, and terminal replay.'
