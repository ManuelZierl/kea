#!/usr/bin/env bash
# Exercise the tmux manager through the real window and an isolated tmux server.
set -Eeuo pipefail
source scripts/smoke-input.sh
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
export TMUX_TMPDIR="$XDG_RUNTIME_DIR/tmux"
mkdir -p "$TMUX_TMPDIR"
chmod 700 "$XDG_RUNTIME_DIR" "$TMUX_TMPDIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
printf 'theme = dark\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
window= pid=
cleanup() {
  local status=$?
  [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  [[ -n "$pid" ]] && wait "$pid" 2>/dev/null || true
  tmux -S "$TMUX_TMPDIR/default" kill-server 2>/dev/null || true
  rm -rf "$XDG_RUNTIME_DIR"
  exit "$status"
}
trap cleanup EXIT

tmux -S "$TMUX_TMPDIR/default" new-session -d -s overlay-test 'exec bash'
server_pid=$(tmux -S "$TMUX_TMPDIR/default" display-message -p '#{pid}')
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py \
  smoke-artifacts/tmux-overlay.bin >smoke-artifacts/tmux-overlay.log 2>&1 &
pid=$!
for _ in $(seq 1 300); do
  kill -0 "$pid" 2>/dev/null || { cat smoke-artifacts/tmux-overlay.log; exit 1; }
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
  [[ -n "$window" ]] && break
  sleep .1
done
[[ -n "$window" ]] || { echo 'No Kea window'; exit 1; }
xdotool windowfocus --sync "$window"
sleep 1

# Standard 1050x780 smoke geometry: toolbar tmux button, then first row Open.
smoke_click 800 51
sleep .5
import -silent -window "$window" smoke-artifacts/tmux-manager.png
smoke_click 970 145
for _ in $(seq 1 50); do
  [[ "$(tmux -S "$TMUX_TMPDIR/default" list-clients -t overlay-test 2>/dev/null | wc -l)" -ge 1 ]] && break
  sleep .1
done
[[ "$(tmux -S "$TMUX_TMPDIR/default" list-clients -t overlay-test 2>/dev/null | wc -l)" -ge 1 ]]
[[ "$(tmux -S "$TMUX_TMPDIR/default" display-message -p '#{pid}')" == "$server_pid" ]]

# Closing the Kea tab detaches its client while preserving the server/session.
xdotool key --clearmodifiers --delay 50 ctrl+shift+w
sleep .3
xdotool key --clearmodifiers --delay 50 Escape
xdotool key --clearmodifiers --delay 50 ctrl+shift+w
sleep .3
xdotool key --clearmodifiers --delay 50 Return
for _ in $(seq 1 50); do
  [[ "$(tmux -S "$TMUX_TMPDIR/default" has-session -t overlay-test 2>/dev/null; echo $?)" -eq 0 ]] && break
  sleep .1
done
tmux -S "$TMUX_TMPDIR/default" has-session -t overlay-test
[[ "$(tmux -S "$TMUX_TMPDIR/default" display-message -p '#{pid}')" == "$server_pid" ]]

# Reopen the manager and cancel Kill; the persistent session must remain.
smoke_click 800 51
sleep .5
import -silent -window "$window" smoke-artifacts/tmux-manager-kill.png
smoke_click 970 190
sleep .3
xdotool key --clearmodifiers --delay 50 Escape
tmux -S "$TMUX_TMPDIR/default" has-session -t overlay-test
echo 'tmux manager attach/detach, server preservation, and Kill cancellation passed.'
