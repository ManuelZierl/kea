#!/usr/bin/env bash
# Run under Xvfb and DBus. Native Windows key shapes have separate unit coverage.
set -Eeuo pipefail
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
printf 'theme = dark\nshow_blocks = true\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
unset WAYLAND_DISPLAY
kea_pid=''; window=''
cleanup_app() { if [[ -n "$kea_pid" ]]; then kill "$kea_pid" 2>/dev/null || true; wait "$kea_pid" 2>/dev/null || true; fi; kea_pid=''; window=''; }
cleanup() { cleanup_app; rm -rf "$XDG_RUNTIME_DIR"; }
trap cleanup EXIT
trap '[[ -z "$window" ]] || import -window "$window" smoke-artifacts/failure.png 2>/dev/null || true' ERR
start() {
  local name="$1"; shift
  ./target/debug/kea "$@" >"smoke-artifacts/$name.log" 2>&1 & kea_pid=$!
  for _ in $(seq 1 80); do
    kill -0 "$kea_pid" || { cat "smoke-artifacts/$name.log"; exit 1; }
    window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
    [[ -z "$window" ]] || break
    sleep .25
  done
  [[ -n "$window" ]]
  xdotool windowfocus --sync "$window"
  sleep 1
}
key() { xdotool key --clearmodifiers "$@"; sleep .15; }
fkey() {
  local code
  case "$1" in
    F6) code=72 ;;
    F7) code=73 ;;
    F8) code=74 ;;
    F9) code=75 ;;
    F10) code=76 ;;
    *) echo "Unsupported function key: $1" >&2; return 1 ;;
  esac
  xdotool key --clearmodifiers "$code"
  sleep .15
}
clip() { timeout 3 xclip -selection clipboard -o; }
put() { printf '%s' "$1" | xclip -selection clipboard; sleep .1; }
assert_clip() { local actual; actual=$(clip); [[ "$actual" == "$1" ]] || { printf 'Expected <%s>, got <%s>\n' "$1" "$actual"; return 1; }; }
editor_click() {
  eval "$(xdotool getwindowgeometry --shell "$window")"
  xdotool mousemove --window "$window" 140 "$((HEIGHT-115))" click 1
  sleep .2
}

start demo --demo
sleep 4
fkey F6
fkey F6
fkey F6
fkey F10
clip > smoke-artifacts/history.txt
grep -q 'ERROR: connection failed' smoke-artifacts/history.txt
fkey F9
fkey F10
clip > smoke-artifacts/latest.txt
grep -q 'Ready. The error has disappeared' smoke-artifacts/latest.txt
cleanup_app

start document -- bash --noprofile --norc
key ctrl+l
xdotool type --clearmodifiers --delay 15 'abcXYZ'
sleep 1.2
key End shift+Left shift+Left shift+Left ctrl+c
assert_clip XYZ
key ctrl+x ctrl+a ctrl+c; assert_clip abc
key ctrl+z ctrl+a ctrl+c; assert_clip abcXYZ
key ctrl+shift+z ctrl+a ctrl+c; assert_clip abc
key ctrl+a BackSpace
put $'cd /tmp\nprintf "kea_doc_one ä\\n"\nprintf "kea_doc_two\\n"'
key ctrl+v
fkey F10
[[ -z "$(clip)" ]]
key ctrl+Return
found=''
for _ in $(seq 1 60); do
  fkey F10
  clip > smoke-artifacts/document.txt
  if grep -Fq 'kea_doc_one ä' smoke-artifacts/document.txt && grep -Fq 'exit 0' smoke-artifacts/document.txt; then found=1; break; fi
  sleep .1
done
[[ -n "$found" ]]
put KEEP; key ctrl+z ctrl+a ctrl+c; assert_clip KEEP
import -window "$window" smoke-artifacts/document.png
# Blocks are on the right; the terminal remains mounted on the left.
eval "$(xdotool getwindowgeometry --shell "$window")"
xdotool mousemove --window "$window" "$((WIDTH*3/4))" 235 click 1
sleep .2
key ctrl+a ctrl+c
clip > smoke-artifacts/selected-block.txt
grep -q 'kea_doc_one' smoke-artifacts/selected-block.txt
key BackSpace ctrl+x ctrl+v ctrl+a ctrl+c
clip > smoke-artifacts/selected-block-after.txt
cmp smoke-artifacts/selected-block.txt smoke-artifacts/selected-block-after.txt
key ctrl+l
xdotool type --clearmodifiers 'printf first'
key Return
xdotool type --clearmodifiers 'printf second'
key ctrl+a ctrl+c; assert_clip $'printf first\nprintf second'
cleanup_app

# Every Kea shortcut goes to the child while the actual terminal is focused.
printf 'theme = dark\nshow_blocks = false\n' > "$KEA_SETTINGS"
probe="$(pwd)/smoke-artifacts/raw-input.bin"
start raw --direct -- python3 -u scripts/terminal-input-probe.py "$probe"
for _ in $(seq 1 50); do [[ -f "${probe%.bin}.ready" ]] && break; sleep .1; done
[[ -f "${probe%.bin}.ready" ]]
xdotool type --clearmodifiers --delay 20 'a b'
key Tab shift+Tab ctrl+c ctrl+v ctrl+z ctrl+l
fkey F6
fkey F7
fkey F8
fkey F9
fkey F10
key ctrl+Return
sleep .3
python3 - "$probe" <<'PY'
import pathlib,sys
actual=pathlib.Path(sys.argv[1]).read_bytes()
expected=b'a b\t\x1b[Z\x03\x16\x1a\x0c\x1b[17~\x1b[18~\x1b[19~\x1b[20~\x1b[21~\x1b[13;5u'
assert actual==expected, (actual, expected)
PY
import -window "$window" smoke-artifacts/terminal.png
# The bottom editor coexists with this raw application; no view/mode switch.
editor_click
put 'draft with spaces'; key ctrl+v
before=$(wc -c < "$probe")
key Return
[[ "$(wc -c < "$probe")" == "$before" ]]
key BackSpace ctrl+Return
sleep .3
python3 - "$probe" <<'PY'
import pathlib,sys
assert pathlib.Path(sys.argv[1]).read_bytes().endswith(b'draft with spaces\r')
PY
# Completion transfers the exact one-line draft plus Tab, never Enter.
put 'look'; key ctrl+v ctrl+space
sleep .3
python3 - "$probe" <<'PY'
import pathlib,sys
assert pathlib.Path(sys.argv[1]).read_bytes().endswith(b'draft with spaces\rlook\t')
PY
import -window "$window" smoke-artifacts/shared-session.png
cleanup_app
printf 'Shared session acceptance passed.\n'
