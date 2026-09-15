#!/usr/bin/env bash
# End-to-end input delivery, safe completion, and tail-layout diagnostics.
set -Eeuo pipefail
root=$(pwd)
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR=$(mktemp -d)
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
printf 'theme = dark\n' >"$KEA_SETTINGS"
unset WAYLAND_DISPLAY
pid=''; window=''
cleanup() {
  local result=$?
  set +e
  if (( result != 0 )); then
    [[ -z "$window" ]] || import -window "$window" "$root/smoke-artifacts/feedback-failure.png"
  fi
  [[ -z "$pid" ]] || kill "$pid" 2>/dev/null
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
wait_window() {
  for _ in $(seq 1 100); do
    window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    if [[ -n "$window" ]]; then xdotool windowfocus --sync "$window"; sleep 1; return; fi
    kill -0 "$pid" || { cat "$root/smoke-artifacts/feedback-app.log"; exit 1; }
    sleep .2
  done
  exit 1
}
key() { xdotool key --clearmodifiers "$@"; sleep .2; }
clip() { timeout 3s xclip -selection clipboard -o; }
cat >"$XDG_RUNTIME_DIR/child.py" <<'PY'
import os, pathlib, sys, time, tty
root=pathlib.Path(sys.argv[1]); tty.setraw(0)
time.sleep(.5)
rows=os.get_terminal_size(0).lines
print('\x1b[2J\x1b[H' + '\r\n'.join(f'Row {i}' for i in range(1,rows)) + '\r\nTERMINAL_BOTTOM',end='',flush=True)
(root/'ready').touch()
expected=b'abc def\x03\x16\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[21~\x00\t\x1b[Z'
actual=b''
while len(actual)<len(expected): actual += os.read(0, len(expected)-len(actual))
(root/'received').write_bytes(actual)
assert actual == expected, (actual, expected)
PY
"$root/target/debug/kea" --direct -- python3 "$XDG_RUNTIME_DIR/child.py" "$XDG_RUNTIME_DIR" >smoke-artifacts/feedback-app.log 2>&1 &
pid=$!; wait_window
for _ in $(seq 1 50); do [[ ! -f "$XDG_RUNTIME_DIR/ready" ]] || break; sleep .1; done
[[ -f "$XDG_RUNTIME_DIR/ready" ]]
import -window "$window" smoke-artifacts/terminal-bottom.png
xdotool type --clearmodifiers --delay 30 'abc def'
key ctrl+c ctrl+v F6 F7 F8 F9 F10 ctrl+shift+space Tab shift+Tab
for _ in $(seq 1 50); do [[ ! -f "$XDG_RUNTIME_DIR/received" ]] || break; sleep .1; done
[[ -f "$XDG_RUNTIME_DIR/received" ]]
python3 - "$XDG_RUNTIME_DIR/received" <<'PY'
import pathlib,sys
actual=pathlib.Path(sys.argv[1]).read_bytes()
expected=b'abc def\x03\x16\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[21~\x00\t\x1b[Z'
assert actual == expected, (actual, expected)
print('Direct keyboard delivery passed:',repr(actual))
PY
sleep .5
# Once the child exits, Kea's observation shortcuts become available again.
key ctrl+shift+q
wait "$pid"; pid=''; window=''
printf 'COMPLETION_OUTPUT\n' >"$XDG_RUNTIME_DIR/kea_unique_completion.txt"
(cd "$XDG_RUNTIME_DIR"; "$root/target/debug/kea" -- sh) >smoke-artifacts/feedback-app.log 2>&1 &
pid=$!; wait_window
sleep .5
xdotool type --clearmodifiers --delay 30 'cat kea_unique_comp'
key Tab
sleep 1
key ctrl+a ctrl+c
[[ "$(clip)" == 'cat kea_unique_completion.txt' ]]
key ctrl+Return
sleep 1
key F10
clip >smoke-artifacts/completion-document.txt
grep -q 'COMPLETION_OUTPUT' smoke-artifacts/completion-document.txt
grep -q 'exit 0' smoke-artifacts/completion-document.txt
printf 'seq 1 200; printf AUTOSCROLL_TAIL' | xclip -selection clipboard
key ctrl+v ctrl+Return
sleep 2
import -window "$window" smoke-artifacts/block-bottom.png
key F10
clip >smoke-artifacts/feedback-document.txt
grep -q 'AUTOSCROLL_TAIL' smoke-artifacts/feedback-document.txt
[[ $(grep -c '^exit ' smoke-artifacts/feedback-document.txt) -eq 2 ]]
key ctrl+shift+q
wait "$pid"; pid=''; window=''
echo 'Feedback smoke passed: literal Space, child-owned Ctrl/F keys, Tab, completion, execution, and tail screenshots.'
