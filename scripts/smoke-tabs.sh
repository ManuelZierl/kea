#!/usr/bin/env bash
# Exercise the real workspace and child processes, not just a tab collection.
set -Eeuo pipefail
export PYTHONDONTWRITEBYTECODE=1
mkdir -p smoke-artifacts
exec > >(tee smoke-artifacts/tabs-acceptance.log) 2>&1
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export KEA_MEMORY_DIR="$XDG_RUNTIME_DIR/memories"
export SHELL=/bin/bash
export HISTFILE="$XDG_RUNTIME_DIR/bash-history"
printf 'theme = dark\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
window=''; pid=''
cleanup() {
  local status=$?
  if [[ $status -ne 0 && -n "$window" ]]; then
    import -window "$window" smoke-artifacts/tabs-failure.png || true
  fi
  if [[ -n "$pid" ]]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
trap 'echo "Tab smoke failed at line $LINENO: $BASH_COMMAND" >&2' ERR
./target/debug/kea --direct -- python3 scripts/terminal-fixture.py smoke-artifacts/tab-a.bin >smoke-artifacts/tabs.log 2>&1 & pid=$!
for _ in $(seq 1 100); do
  kill -0 "$pid"
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
  [[ -z "$window" ]] || break
  sleep .1
done
[[ -n "$window" ]]
xdotool windowfocus --sync "$window"
sleep 1
key() {
  local chord
  for chord in "$@"; do
    xdotool key --clearmodifiers --delay 50 "$chord"
    sleep .2
  done
}
assert_draft() {
  local actual=''
  key ctrl+a ctrl+c
  for _ in $(seq 1 30); do
    actual=$(timeout .5s xclip -selection clipboard -t UTF8_STRING -o 2>/dev/null || true)
    [[ "$actual" != "$1" ]] || return 0
    sleep .1
  done
  printf 'Expected draft <%s>, got <%s>\n' "$1" "$actual" >&2
  return 1
}
key ctrl+l
xdotool type --clearmodifiers 'draft-A'
key ctrl+shift+t
sleep .5
xdotool type --clearmodifiers 'draft-B'
assert_draft draft-B
key ctrl+Tab
assert_draft draft-A
# Only A's child receives this character; the other shell stays independent.
key ctrl+l
xdotool type --clearmodifiers 'a'
key ctrl+l ctrl+Tab
assert_draft draft-B
key ctrl+shift+Prior
assert_draft draft-B
key ctrl+Tab
assert_draft draft-A
key ctrl+Tab
assert_draft draft-B
# Closing a live session must prompt; cancellation preserves the context.
key ctrl+shift+w
sleep .5
import -window "$window" smoke-artifacts/tabs-close-confirm.png
key Escape
sleep .3
assert_draft draft-B
# Reorder back and confirm mouse activation retains the original draft.
key ctrl+shift+Next
xdotool mousemove --window "$window" 75 51 click 1
sleep .3
assert_draft draft-A
python3 - <<'PY'
from pathlib import Path
actual = Path('smoke-artifacts/tab-a.bin').read_bytes()
assert actual == b'a', actual
PY
import -window "$window" smoke-artifacts/terminal-tabs.png
echo 'Independent drafts, terminal input, tab cycling/reordering, mouse activation and close cancellation passed.'
