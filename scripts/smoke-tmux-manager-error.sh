#!/usr/bin/env bash
# Prove a tmux discovery error stays in the manager without resizing its PTY.
set -Eeuo pipefail
source scripts/smoke-input.sh
mkdir -p smoke-artifacts
export XDG_RUNTIME_DIR="$(mktemp -d)"
export KEA_SETTINGS="$XDG_RUNTIME_DIR/settings.conf"
export KEA_KEYBINDINGS="$XDG_RUNTIME_DIR/keybindings.conf"
export PATH="$XDG_RUNTIME_DIR/fake-bin:$PATH"
mkdir -p "$XDG_RUNTIME_DIR/fake-bin"
chmod 700 "$XDG_RUNTIME_DIR"
printf 'theme = dark\n' > "$KEA_SETTINGS"
: > "$KEA_KEYBINDINGS"
cat > "$XDG_RUNTIME_DIR/fake-bin/tmux" <<'SH'
#!/usr/bin/env bash
printf 'KEA_FAKE_TMUX_DISCOVERY_ERROR\n' >&2
exit 1
SH
chmod 700 "$XDG_RUNTIME_DIR/fake-bin/tmux"

fixture="$XDG_RUNTIME_DIR/raw-pty-fixture.py"
cat > "$fixture" <<'PY'
import os
import signal
import sys
import tty

sizes, inputs = map(open, sys.argv[1:3])

def record_size(*_):
    try:
        size = os.get_terminal_size(0)
    except OSError:
        return
    sizes.write(f"{size.columns} {size.lines}\n")
    sizes.flush()

signal.signal(signal.SIGWINCH, record_size)
tty.setraw(0)
os.write(1, b"\x1b[?1003h\x1b[?1006hPTY fixture ready\r\n")
record_size()
while True:
    data = os.read(0, 4096)
    if not data:
        break
    inputs.write(data.hex() + "\n")
    inputs.flush()
    os.write(1, data.hex().encode() + b"\r\n")
PY

window= pid=
cleanup() {
  local status=$?
  [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  [[ -n "$pid" ]] && wait "$pid" 2>/dev/null || true
  rm -rf "$XDG_RUNTIME_DIR"
  exit "$status"
}
trap cleanup EXIT

./target/debug/kea --direct -- python3 "$fixture" \
  "$XDG_RUNTIME_DIR/pty-sizes.log" "$XDG_RUNTIME_DIR/pty-input.log" \
  >smoke-artifacts/tmux-error.log 2>&1 &
pid=$!
for _ in $(seq 1 300); do
  kill -0 "$pid" 2>/dev/null || { cat smoke-artifacts/tmux-error.log; exit 1; }
  window=$(xdotool search --onlyvisible --name '^Kea$' 2>/dev/null | tail -1 || true)
  [[ -n "$window" && -s "$XDG_RUNTIME_DIR/pty-sizes.log" ]] && break
  sleep .1
done
[[ -n "$window" && -s "$XDG_RUNTIME_DIR/pty-sizes.log" ]] || exit 1
xdotool windowfocus --sync "$window"
sleep 1

# Wait for the initial PTY layout to settle, then retain the last stable size.
stable_size=''
for _ in $(seq 1 50); do
  current_size=$(tail -n 1 "$XDG_RUNTIME_DIR/pty-sizes.log")
  [[ "$current_size" == "$stable_size" ]] && break
  stable_size="$current_size"
  sleep .1
done
[[ "$stable_size" =~ ^[0-9]+\ [0-9]+$ ]]

# The tmux manager button is at the right edge of the standard smoke toolbar.
before_input=$(cat "$XDG_RUNTIME_DIR/pty-input.log" 2>/dev/null || :)
before_frame=$(import -silent -window "$window" png:- | sha256sum)
smoke_click 995 51
error_frame=''
for _ in $(seq 1 50); do
  sleep .1
  frame=$(import -silent -window "$window" png:- | sha256sum)
  if [[ "$frame" != "$before_frame" ]]; then
    error_frame="$frame"
    import -silent -window "$window" smoke-artifacts/tmux-manager-error.png
    break
  fi
done
[[ -n "$error_frame" ]] || { import -silent -window "$window" smoke-artifacts/tmux-manager-error-failure.png; exit 1; }

# Error content occupies the panel near y=170. SGR mouse reporting makes any
# leaked hover/wheel events observable in the child input log.
smoke_click 700 170
xdotool click 4
sleep .3
after_input=$(cat "$XDG_RUNTIME_DIR/pty-input.log" 2>/dev/null || :)
[[ "$before_input" == "$after_input" ]] || {
  import -silent -window "$window" smoke-artifacts/tmux-manager-error-input-leak.png
  printf 'covered manager gesture leaked into child input\n' >&2
  exit 1
}

after_size=$(tail -n 1 "$XDG_RUNTIME_DIR/pty-sizes.log")
[[ "$after_size" == "$stable_size" ]] || {
  import -silent -window "$window" smoke-artifacts/tmux-manager-error-resized.png
  printf 'PTY changed from %s to %s\n' "$stable_size" "$after_size" >&2
  exit 1
}
echo "tmux discovery error preserved PTY $stable_size and blocked covered pointer/wheel input."
