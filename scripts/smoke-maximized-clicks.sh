#!/usr/bin/env bash
set -Eeuo pipefail

# Exercise right-side chrome and the real Settings dialog in a maximized Linux
# client-decorated window. Requires Xvfb, Openbox, xdotool, ImageMagick, and a
# built target/debug/kea. OPENBOX_BIN may point to a locally extracted package.
root=$(cd "$(dirname "$0")/.." && pwd)
openbox_bin=${OPENBOX_BIN:-$(command -v openbox || true)}
for tool in Xvfb xdotool import sha256sum; do
  command -v "$tool" >/dev/null || { echo "Missing required tool: $tool" >&2; exit 1; }
done
[[ -x "$openbox_bin" ]] || { echo 'Set OPENBOX_BIN to an Openbox executable.' >&2; exit 1; }
[[ -x "$root/target/debug/kea" ]] || { echo 'Build target/debug/kea first.' >&2; exit 1; }

tmp=$(mktemp -d "${TMPDIR:-/tmp}/kea-maximized-smoke.XXXXXX")
unset DISPLAY WAYLAND_DISPLAY
export XDG_RUNTIME_DIR="$tmp/run" XDG_CONFIG_HOME="$tmp/config"
export XDG_DATA_HOME="$tmp/data" XDG_STATE_HOME="$tmp/state"
export XDG_DATA_DIRS="$tmp/share:/usr/local/share:/usr/share"
export HOME="$tmp/home" SHELL=/bin/sh
export KEA_SETTINGS="$tmp/settings.conf" KEA_KEYBINDINGS="$tmp/keybindings.conf"
export KEA_MEMORY_DIR="$tmp/memory"
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_CONFIG_HOME/openbox" "$tmp/share/themes/KeaSmoke/openbox-3" "$HOME"
chmod 700 "$XDG_RUNTIME_DIR"

cleanup() {
  local code=$?
  [[ -z ${kea_pid:-} ]] || { kill "$kea_pid" 2>/dev/null || true; wait "$kea_pid" 2>/dev/null || true; }
  [[ -z ${wm_pid:-} ]] || { kill "$wm_pid" 2>/dev/null || true; wait "$wm_pid" 2>/dev/null || true; }
  [[ -z ${xvfb_pid:-} ]] || { kill "$xvfb_pid" 2>/dev/null || true; wait "$xvfb_pid" 2>/dev/null || true; }
  if [[ $code -eq 0 ]]; then
    rm -rf -- "$tmp"
  else
    echo "Smoke diagnostics retained at $tmp" >&2
  fi
  exit "$code"
}
trap cleanup EXIT

cat > "$tmp/share/themes/KeaSmoke/openbox-3/themerc" <<'EOF'
border.width: 1
border.color: #222222
padding.width: 4
padding.height: 3
window.active.title.bg: flat solid
window.active.title.bg.color: #444444
window.active.label.text.color: #ffffff
window.inactive.title.bg: flat solid
window.inactive.title.bg.color: #222222
window.inactive.label.text.color: #cccccc
menu.items.bg: flat solid
menu.items.bg.color: #333333
menu.items.text.color: #ffffff
EOF
rc_source=${OPENBOX_RC:-/etc/xdg/openbox/rc.xml}
[[ -f "$rc_source" ]] || { echo "Openbox config not found: $rc_source (set OPENBOX_RC)" >&2; exit 1; }
sed 's#<name>Clearlooks</name>#<name>KeaSmoke</name>#' "$rc_source" > "$XDG_CONFIG_HOME/openbox/rc.xml"

Xvfb -displayfd 3 -screen 0 1440x900x24 3>"$tmp/display" >"$tmp/xvfb.log" 2>&1 & xvfb_pid=$!
for _ in $(seq 1 50); do
  kill -0 "$xvfb_pid" || { cat "$tmp/xvfb.log" >&2; exit 1; }
  [[ -s "$tmp/display" ]] && break
  sleep .1
done
[[ -s "$tmp/display" ]] || { echo 'Xvfb did not allocate a display.' >&2; exit 1; }
export DISPLAY=":$(cat "$tmp/display")"
xdotool getdisplaygeometry >/dev/null
"$openbox_bin" >"$tmp/openbox.log" 2>&1 & wm_pid=$!
sleep .5
printf 'theme = dark\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
"$root/target/debug/kea" --direct -- python3 -c 'import time; time.sleep(3600)' >"$tmp/kea.log" 2>&1 & kea_pid=$!
window=''
for _ in $(seq 1 100); do
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
  [[ -n $window ]] && break
  kill -0 "$kea_pid" || { cat "$tmp/kea.log" >&2; exit 1; }
  sleep .1
done
[[ -n $window ]] || { echo 'Kea window did not appear.' >&2; exit 1; }
xdotool windowfocus --sync "$window"

# GPUI's Linux title bar handles a double-click by requesting WM maximize.
xdotool mousemove --sync --window "$window" 720 15
sleep .15
xdotool mousedown 1
sleep .15
xdotool mouseup 1
sleep .2
xdotool mousedown 1
sleep .15
xdotool mouseup 1
sleep 1
eval "$(xdotool getwindowgeometry --shell "$window")"
if (( WIDTH < 1400 || HEIGHT < 800 )); then
  import -window "$window" "$tmp/not-maximized.png"
  echo "Window did not maximize: ${WIDTH}x${HEIGHT}" >&2
  exit 1
fi

click_at() {
  xdotool mousemove --window "$window" "$1" "$2"
  sleep .15
  xdotool mousedown 1
  sleep .15
  xdotool mouseup 1
  sleep .2
}
tab_strip_hash() {
  import -silent -window "$window" -crop "${WIDTH}x36+0+32" -depth 8 rgb:- | sha256sum | cut -d' ' -f1
}

# Open actual Settings, switch tabs, record a chord and save it. This covers the
# dialog/modal focus path omitted by isolated KeybindingEditor UI tests.
click_at "$((WIDTH - 213))" 86
sleep .7
click_at "$((WIDTH / 2 - 205))" 95
sleep .7
click_at "$((WIDTH / 2 + 300))" 352
sleep .3
xdotool key --clearmodifiers --delay 50 alt+l
sleep .4
click_at "$((WIDTH / 2))" 311
for _ in $(seq 1 50); do
  grep -Fqx 'focus_editor = alt-l' "$KEA_KEYBINDINGS" 2>/dev/null && break
  sleep .1
done
grep -Fqx 'focus_editor = alt-l' "$KEA_KEYBINDINGS" || {
  import -window "$window" "$tmp/failure.png"
  echo 'Record/Save in maximized Settings did not persist the captured chord.' >&2
  exit 1
}

# Close Settings explicitly, then test the far-right add-tab control. Keep this
# before adding a tab because toolbar contents adapt to the new active receiver.
click_at "$((WIDTH / 2 + 333))" 56
sleep .5
before=$(tab_strip_hash)
click_at "$((WIDTH - 13))" 51
xdotool mousemove --window "$window" "$((WIDTH / 2))" 160
for _ in $(seq 1 40); do
  after=$(tab_strip_hash)
  [[ $after != "$before" ]] && break
  sleep .1
done
[[ $after != "$before" ]] || { echo 'Add-tab click did not change the tab strip.' >&2; exit 1; }

# Save session is another far-right toolbar action. Keep its file under this
# test's isolated XDG state directory and require a real persisted recording.
click_at "$((WIDTH - 142))" 86
for _ in $(seq 1 50); do
  saved=$(find "$XDG_STATE_HOME/kea/sessions" -maxdepth 1 -type f -name 'session-*.kea' -print -quit 2>/dev/null || true)
  [[ -n $saved ]] && break
  sleep .1
done
[[ -n ${saved:-} ]] || { echo 'Save session click did not create a recording.' >&2; exit 1; }

echo 'Maximized Linux add-tab, Save session, Settings, and keybinding Record/Save clicks passed.'
