#!/usr/bin/env bash
# Runs under an isolated Xvfb/DBus session. Uses only Kea's synthetic demo.
set -euo pipefail
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY
./target/debug/kea --demo >smoke-artifacts/desktop.log 2>&1 &
kea_pid=$!
cleanup() {
  kill "$kea_pid" 2>/dev/null || true
  wait "$kea_pid" 2>/dev/null || true
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
window=''
for _ in $(seq 1 60); do
  kill -0 "$kea_pid" || { cat smoke-artifacts/desktop.log; exit 1; }
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | head -1 || true)
  if [[ -n "$window" ]]; then break; fi
  sleep 0.25
done
[[ -n "$window" ]] || { cat smoke-artifacts/desktop.log; echo 'Kea window did not appear'; exit 1; }
xdotool windowfocus --sync "$window"
sleep 5
# Five demo events; after playback finishes, step from event 5 to event 2.
xdotool key --clearmodifiers F6 F6 F6
sleep 0.5
# Linux/Windows default copy is now the native Ctrl+C action.
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
  echo 'The latest screen unexpectedly contains the historical error'; exit 1
fi
import -window "$window" smoke-artifacts/latest.png
xdotool key --clearmodifiers ctrl+shift+q
for _ in $(seq 1 40); do
  if ! kill -0 "$kea_pid" 2>/dev/null; then
    wait "$kea_pid"
    echo 'Desktop smoke passed: window, replay, live, clipboard, keyboard and quit.'
    exit 0
  fi
  sleep 0.1
done
echo 'Kea did not quit'; exit 1
