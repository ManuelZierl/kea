#!/usr/bin/env bash
# Scroll capture through the real window: block-select a full-screen app's
# message column, capture while the wheel scrolls the app, review and copy.
set -Eeuo pipefail
source scripts/smoke-input.sh
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export KEA_MEMORY_DIR="$XDG_RUNTIME_DIR/input-memory"
printf 'theme = dark\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
unset WAYLAND_DISPLAY
received=smoke-artifacts/scroll-capture-input.bin
rm -f "$received"
pid=''; window=''
cleanup() {
  local code=$?
  if [[ "$code" -ne 0 && -n "$window" ]]; then
    import -window "$window" smoke-artifacts/scroll-capture-failure.png 2>/dev/null || true
  fi
  [[ -z "$pid" ]] || { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; }
  rm -rf "$XDG_RUNTIME_DIR"
}
trap cleanup EXIT
trap 'echo "Scroll capture smoke failed at line $LINENO: $BASH_COMMAND" >&2' ERR

./target/debug/kea --direct -- python3 scripts/scroll-capture-fixture.py "$received" \
  >smoke-artifacts/scroll-capture.log 2>&1 &
pid=$!
for _ in $(seq 1 300); do
  kill -0 "$pid" 2>/dev/null || { cat smoke-artifacts/scroll-capture.log; exit 1; }
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
  [[ -n "$window" && -f "$received" ]] && break
  sleep .1
done
[[ -n "$window" ]] || { echo 'No Kea window'; exit 1; }
xdotool windowfocus --sync "$window"
sleep 1.5

# Standard 1050x780 geometry: Shift+Alt-drag a block over the message column
# only, excluding the side panel, header and input box.
xdotool mousemove --window "$window" 101 157; sleep .2
xdotool keydown shift alt; sleep .1
xdotool mousedown 1; sleep .2
xdotool mousemove --window "$window" 300 300; sleep .2
xdotool mousemove --window "$window" 480 505; sleep .2
xdotool mouseup 1; sleep .1
xdotool keyup alt shift; sleep .4
smoke_click 430 119 # Capture scroll
sleep .5
xdotool mousemove --window "$window" 300 300; sleep .2
for _ in $(seq 1 15); do
  xdotool click 4
  sleep .25
done
sleep .5
import -silent -window "$window" smoke-artifacts/scroll-capture-active.png
smoke_click 523 119 # Done: review before copying
sleep 1.5
import -silent -window "$window" smoke-artifacts/scroll-capture-review.png
printf 'kea-capture-sentinel' | xclip -selection clipboard
smoke_click 900 679 # Copy all
copied=''
for _ in $(seq 1 30); do
  copied=$(timeout .5s xclip -selection clipboard -o 2>/dev/null || true)
  [[ "$copied" == *'message 150'* ]] && break
  sleep .1
done
python3 - "$copied" <<'PY'
import sys
got = sys.argv[1].split("\n")
expected = [f"message {i:03}: transcript text ä {'·' * (i % 7)}".rstrip() for i in range(1, 151)]
assert got == expected, f"reconstruction differs: {len(got)} lines; first {got[:2]!r}, last {got[-2:]!r}"
PY
# The app saw only wheel reports: capture never injects keystrokes.
python3 - "$received" <<'PY'
import re, sys
data = open(sys.argv[1], "rb").read()
rest = re.sub(rb"\x1b\[<6[45];\d+;\d+[Mm]", b"", data)
assert data and not rest, f"unexpected input reached the app: {rest[:40]!r}"
PY
echo 'Scroll capture block region, wheel passthrough, exact reconstruction and review-before-copy passed.'
