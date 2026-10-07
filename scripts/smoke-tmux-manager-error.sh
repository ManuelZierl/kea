#!/usr/bin/env bash
# Verify a tmux discovery error remains inside the occluded manager panel.
set -Eeuo pipefail
source scripts/smoke-input.sh
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
export TMUX_TMPDIR="$XDG_RUNTIME_DIR/missing-tmux"
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
  rm -rf "$XDG_RUNTIME_DIR"
  exit "$status"
}
trap cleanup EXIT

./target/debug/kea --direct -- python3 scripts/terminal-fixture.py \
  smoke-artifacts/tmux-error-terminal.bin >smoke-artifacts/tmux-error.log 2>&1 &
pid=$!
for _ in $(seq 1 300); do
  kill -0 "$pid" 2>/dev/null || { cat smoke-artifacts/tmux-error.log; exit 1; }
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
  [[ -n "$window" ]] && break
  sleep .1
done
[[ -n "$window" ]] || { echo 'No Kea window'; exit 1; }
xdotool windowfocus --sync "$window"
sleep 1
eval "$(xdotool getwindowgeometry --shell "$window")"
before_geometry="$WIDTH,$HEIGHT"

# Discovery fails because this owned socket directory has no server. The error
# must paint in the manager without adding a normal-flow row before the body.
smoke_click 800 51
sleep 1
import -silent -window "$window" smoke-artifacts/tmux-manager-error.png
eval "$(xdotool getwindowgeometry --shell "$window")"
[[ "$before_geometry" == "$WIDTH,$HEIGHT" ]]

# A covered click and wheel gesture must not reach the live fixture. This also
# leaves a screenshot artifact for manual bounds inspection on GUI runners.
before_bytes=$(wc -c < smoke-artifacts/tmux-error-terminal.bin)
smoke_click 700 300
xdotool click 4
sleep .3
after_bytes=$(wc -c < smoke-artifacts/tmux-error-terminal.bin)
[[ "$before_bytes" == "$after_bytes" ]]
echo 'tmux discovery error stayed in the stable manager overlay without terminal gesture leakage.'
