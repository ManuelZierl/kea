#!/usr/bin/env bash
# Runs under an isolated Xvfb/DBus session. Covers both terminal-history replay
# and the document-native command/output path.
set -euo pipefail
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY
kea_pid=''

cleanup_app() {
  if [[ -n "$kea_pid" ]]; then
    kill "$kea_pid" 2>/dev/null || true
    wait "$kea_pid" 2>/dev/null || true
    kea_pid=''
  fi
}
cleanup() {
  cleanup_app
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT

wait_for_window() {
  local window=''
  for _ in $(seq 1 80); do
    kill -0 "$kea_pid" || { cat "$1"; exit 1; }
    window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    if [[ -n "$window" ]]; then
      printf '%s' "$window"
      return 0
    fi
    sleep 0.25
  done
  cat "$1"
  echo 'Kea window did not appear'
  exit 1
}

wait_for_exit() {
  for _ in $(seq 1 50); do
    if ! kill -0 "$kea_pid" 2>/dev/null; then
      wait "$kea_pid"
      kea_pid=''
      return 0
    fi
    sleep 0.1
  done
  echo 'Kea did not quit'
  exit 1
}

# 1. Historical terminal state remains inspectable and copyable.
./target/debug/kea --demo >smoke-artifacts/demo.log 2>&1 &
kea_pid=$!
window=$(wait_for_window smoke-artifacts/demo.log)
xdotool windowfocus --sync "$window"
sleep 5
# Five demo events; after playback finishes, step from event 5 to event 2.
xdotool key --clearmodifiers F6 F6 F6
sleep 0.5
xdotool key --clearmodifiers ctrl+c
sleep 0.5
timeout 5s xclip -selection clipboard -o >smoke-artifacts/history.txt
grep -q 'ERROR: connection failed' smoke-artifacts/history.txt
import -window "$window" smoke-artifacts/history.png
xdotool key --clearmodifiers F9
sleep 0.5
xdotool key --clearmodifiers ctrl+c
sleep 0.5
timeout 5s xclip -selection clipboard -o >smoke-artifacts/latest.txt
grep -q 'Ready. The error has disappeared' smoke-artifacts/latest.txt
if grep -q 'ERROR: connection failed' smoke-artifacts/latest.txt; then
  echo 'The latest screen unexpectedly contains the historical error'
  exit 1
fi
xdotool key --clearmodifiers ctrl+shift+q
wait_for_exit

# 2. Document mode owns multiline editing and command boundaries and yields one
# persistent read-only block only when Ctrl+Enter explicitly executes it.
./target/debug/kea -- bash --noprofile --norc >smoke-artifacts/document.log 2>&1 &
kea_pid=$!
window=$(wait_for_window smoke-artifacts/document.log)
xdotool windowfocus --sync "$window"
sleep 1
xdotool type --clearmodifiers --delay 5 'printf kea_doc_one;'
xdotool key --clearmodifiers Return
xdotool type --clearmodifiers --delay 5 'printf kea_doc_two'
# Ordinary Enter above must only edit the local document buffer.
xdotool key --clearmodifiers ctrl+Return

found=''
for _ in $(seq 1 50); do
  sleep 0.2
  xdotool key --clearmodifiers ctrl+c
  sleep 0.05
  if timeout 2s xclip -selection clipboard -o >smoke-artifacts/document.txt 2>/dev/null; then
    if grep -Fq '$ printf kea_doc_one;' smoke-artifacts/document.txt \
      && grep -Fq 'printf kea_doc_two' smoke-artifacts/document.txt \
      && grep -Fq 'kea_doc_onekea_doc_two' smoke-artifacts/document.txt \
      && grep -Fq 'exit 0' smoke-artifacts/document.txt; then
      found=1
      break
    fi
  fi
done
[[ -n "$found" ]] || {
  cat smoke-artifacts/document.log
  cat smoke-artifacts/document.txt 2>/dev/null || true
  echo 'Structured multiline document command block did not appear'
  exit 1
}
import -window "$window" smoke-artifacts/document.png
xdotool key --clearmodifiers ctrl+shift+q
wait_for_exit

echo 'Desktop smoke passed: multiline document blocks, native copy, terminal replay, live return and quit.'
