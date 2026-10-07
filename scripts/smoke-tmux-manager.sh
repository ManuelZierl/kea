#!/usr/bin/env bash
# Exercise the tmux manager through the real window and an isolated tmux server.
set -Eeuo pipefail
source scripts/smoke-input.sh
unset TMUX
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
export TMUX_TMPDIR="$XDG_RUNTIME_DIR/tmux"
mkdir -p "$TMUX_TMPDIR"
chmod 700 "$XDG_RUNTIME_DIR" "$TMUX_TMPDIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export KEA_MEMORY_DIR="$XDG_RUNTIME_DIR/memories"
export HOME="$XDG_RUNTIME_DIR/home" HISTFILE="$XDG_RUNTIME_DIR/bash-history"
mkdir -p "$HOME"
printf 'theme = dark\ncheck_for_updates = false\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
window= pid=
cleanup() {
  local status=$?
  if [[ "$status" -ne 0 && -n "$window" ]]; then
    import -silent -window "$window" smoke-artifacts/tmux-manager-failure.png || true
    tail -n 60 smoke-artifacts/tmux-overlay.log >&2 || true
  fi
  [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  [[ -n "$pid" ]] && wait "$pid" 2>/dev/null || true
  tmux kill-server 2>/dev/null || true
  rm -rf "$XDG_RUNTIME_DIR"
  exit "$status"
}
trap cleanup EXIT
trap 'echo "Tmux smoke failed at line $LINENO: $BASH_COMMAND" >&2' ERR

tmux -f /dev/null new-session -d -s overlay-test 'exec bash --noprofile --norc'
server_pid=$(tmux display-message -p '#{pid}')
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
key() {
  xdotool key --clearmodifiers --delay 50 "$1"
  sleep .2
}
clients() {
  tmux display-message -p -t overlay-test '#{session_attached}'
}
assert_clients() {
  for _ in $(seq 1 50); do
    [[ "$(clients)" == "$1" ]] && return
    sleep .1
  done
  echo "Expected $1 attached tmux clients; got $(clients)" >&2
  return 1
}

# Standard 1050x780 smoke geometry: toolbar tmux button, then first row Open.
smoke_click 995 51
sleep .5
import -silent -window "$window" smoke-artifacts/tmux-manager.png
smoke_click 935 199
assert_clients 1
[[ ! -s smoke-artifacts/tmux-overlay.bin ]] || { echo 'Manager click leaked to the underlying terminal'; exit 1; }
[[ "$(tmux display-message -p '#{pid}')" == "$server_pid" ]]

# Closing the Kea tab detaches its client while preserving the server/session.
smoke_click 130 85
key ctrl+shift+w
sleep .3
key Escape
assert_clients 1
# Cancellation restores the tab's remembered focus. Re-enter the composer before
# using a Kea accelerator; a live tmux client must retain native key ownership.
smoke_click 130 85
key ctrl+shift+w
sleep .3
key Return
assert_clients 0
tmux has-session -t overlay-test
[[ "$(tmux display-message -p '#{pid}')" == "$server_pid" ]]

# Reopen the manager and cancel Kill; the persistent session must remain.
smoke_click 995 51
sleep .5
import -silent -window "$window" smoke-artifacts/tmux-manager-kill.png
smoke_click 997 199
sleep .3
key Escape
tmux has-session -t overlay-test
smoke_click 997 199
sleep .3
key Return
for _ in $(seq 1 50); do
  if ! tmux has-session -t overlay-test 2>/dev/null; then
    echo 'tmux manager attach/detach, server preservation, Kill cancellation/confirmation and pointer isolation passed.'
    exit 0
  fi
  sleep .1
done
echo 'Confirmed Kill did not terminate the disposable session' >&2
exit 1
