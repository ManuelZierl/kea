#!/usr/bin/env bash
# End-to-end acceptance for composer transformations and protected terminal input.
set -Eeuo pipefail
source scripts/smoke-input.sh
mkdir -p smoke-artifacts
exec > >(tee -a smoke-artifacts/composer-workflow-acceptance.log) 2>&1
export PYTHONDONTWRITEBYTECODE=1
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export KEA_MEMORY_DIR="$XDG_RUNTIME_DIR/input-memory"
printf 'theme = dark\npost_submit_focus = editor\nconfirm_ctrl_c = true\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
unset WAYLAND_DISPLAY
kea_pid=''; window=''; output=''
cleanup_app() {
  if [[ -n "$kea_pid" ]]; then
    kill "$kea_pid" 2>/dev/null || true
    wait "$kea_pid" 2>/dev/null || true
    kea_pid=''
  fi
}
cleanup() {
  local code=$?
  if [[ "$code" -ne 0 && -n "$window" ]]; then
    import -window "$window" smoke-artifacts/composer-workflow-failure.png 2>/dev/null || true
  fi
  cleanup_app
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
trap 'printf "Composer workflow assertion failed at line %s: %s\n" "$LINENO" "$BASH_COMMAND" >&2' ERR
key() {
  local chord
  for chord in "$@"; do
    xdotool key --clearmodifiers --delay 50 "$chord"
    sleep .15
  done
}
hold_enter() {
  xdotool keydown Return
  sleep .8
  xdotool keyup Return
  sleep .2
}
put_clipboard() { printf '%s' "$1" | xclip -selection clipboard; sleep .15; }
assert_draft() {
  local expected="$1" actual='(clipboard unavailable)'
  put_clipboard kea-composer-sentinel
  key ctrl+a ctrl+c
  for _ in $(seq 1 30); do
    if actual=$(timeout .5s xclip -selection clipboard -t UTF8_STRING -o 2>/dev/null) && [[ "$actual" == "$expected" ]]; then return; fi
    sleep .1
  done
  printf 'Expected draft <%s>; got <%s>\n' "$expected" "$actual" >&2
  return 1
}
assert_bytes() {
  python3 - "$output" "$1" <<'PY'
import sys
import time
from pathlib import Path
path = Path(sys.argv[1])
expected = bytes.fromhex(sys.argv[2])
for _ in range(50):
    actual = path.read_bytes() if path.exists() else b""
    if actual == expected:
        # Catch delayed duplicate submissions and autorepeat after focus changes.
        time.sleep(0.2)
        assert path.read_bytes() == expected, "Input arrived after the expected one-shot delivery"
        break
    time.sleep(0.05)
else:
    raise AssertionError(f"Expected exact PTY bytes {expected!r}; got {actual!r}")
PY
}
start_fixture() {
  cleanup_app
  window=''
  output="smoke-artifacts/composer-$1.bin"
  local log="smoke-artifacts/composer-$1.log"
  ./target/debug/kea --direct -- python3 scripts/terminal-fixture.py "$output" >"$log" 2>&1 & kea_pid=$!
  for _ in $(seq 1 100); do
    kill -0 "$kea_pid" || { cat "$log"; return 1; }
    window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    if [[ -n "$window" && -f "$output" ]]; then
      xdotool windowfocus --sync "$window"
      sleep .5
      smoke_click 130 85
      sleep .2
      return
    fi
    sleep .1
  done
  echo 'No ready Kea fixture window' >&2
  return 1
}

# Recognition alone must not mutate pasted separators or split submission text.
start_fixture literal
put_clipboard $'first\n---\nsecond'
key ctrl+v
assert_draft $'first\n---\nsecond'
key End ctrl+Return
assert_bytes ''
key Return
assert_bytes '1b5b3230307e66697273740a2d2d2d0a7365636f6e641b5b3230317e0d'

# A preview can be cancelled. Only a second explicit acceptance applies a split.
# Home is a supported line-start action; Ctrl+Home is not a buffer-start binding.
start_fixture sections
put_clipboard $'first\n---\nsecond'
key ctrl+v End Home Up ctrl+period Return Escape
assert_draft $'first\n---\nsecond'
assert_bytes ''
key End Home Up ctrl+period Return Return
assert_draft first
assert_bytes ''
key End ctrl+Return
hold_enter
assert_bytes '1b5b3230307e66697273741b5b3230317e0d'
assert_draft second
key End ctrl+Return Return
assert_bytes '1b5b3230307e66697273741b5b3230317e0d1b5b3230307e7365636f6e641b5b3230317e0d'

# Ctrl-C stays local until confirmation; a held Enter delivers exactly one ETX.
start_fixture interrupt
key ctrl+l ctrl+c
assert_bytes ''
hold_enter
assert_bytes '03'
key ctrl+c Escape
assert_bytes '03'
key Return
assert_bytes '030d'
# Local editor Copy must not request or deliver an interrupt.
key ctrl+l
put_clipboard 'copy stays local'
key ctrl+v
assert_draft 'copy stays local'
assert_bytes '030d'

# Confirm before C has repeated, while C itself remains physically held. A
# repeated key-down after confirmation must not leak text or arm another gate.
start_fixture interrupt-trigger
key ctrl+l
xdotool keydown Control_L keydown c keyup Control_L
xdotool key --delay 50 Return
xdotool keydown c keyup c
sleep .2
assert_bytes '03'
key Return
assert_bytes '030d'
echo 'Composer literal input, explicit previews, independent sections, held confirmation, protected Ctrl-C, held trigger, cancellation and local Copy passed.'
