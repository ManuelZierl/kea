# Sourced by smoke-linux.sh; uses its isolated display, app/clipboard helpers,
# settings paths and cleanup trap. Also run via --terminal-scrollback.
printf 'theme = dark\n' > "$KEA_SETTINGS"
select_scrollback_probe_row() {
  # A Shift press with an existing range intentionally extends its old anchor.
  # Clear that range before using Shift to create a fresh row probe in a TUI.
  key Escape
  if [[ "$reporting" == on ]]; then xdotool keydown Shift; fi
  xdotool mousemove --window "$window" 0 160
  sleep .15
  xdotool mousedown 1
  sleep .15
  xdotool mousemove --window "$window" 80 160
  xdotool mouseup 1
  if [[ "$reporting" == on ]]; then xdotool keyup Shift; fi
  key ctrl+c
}
for reporting in off on; do
  fixture_args=(--scrollback)
  if [[ "$reporting" == on ]]; then fixture_args+=(--mouse); fi
  capture="smoke-artifacts/scrollback-$reporting.bin"
  log="smoke-artifacts/scrollback-$reporting.log"
  ./target/debug/kea --direct -- python3 scripts/terminal-selection-fixture.py "$capture" "${fixture_args[@]}" >"$log" 2>&1 & kea_pid=$!
  wait_window "$log"
  if [[ "$reporting" == on ]]; then xdotool keydown Shift; fi
  xdotool mousemove --window "$window" 80 230
  sleep .15
  xdotool mousedown 1
  sleep .15
  # Outside the terminal hitbox, over its header. The timer must keep extending
  # across old output with no further pointer movement.
  xdotool mousemove --window "$window" 0 100
  if [[ "$reporting" == on ]]; then xdotool keyup Shift; fi
  # Observe selection growth, not an exact number of timer ticks under load.
  # Do not use key() here: xdotool --clearmodifiers also releases held buttons.
  for _ in $(seq 1 40); do
    sleep .1
    xdotool key --delay 50 ctrl+c
    SCROLL_SELECTION="$(clipboard)"
    if [[ "$(printf '%s\n' "$SCROLL_SELECTION" | wc -l)" -gt 60 ]]; then break; fi
  done
  xdotool mouseup 1
  key ctrl+c
  export SCROLL_SELECTION="$(clipboard)"
  export SCROLL_CAPTURE="$capture"
  python3 - <<'PY'
import os
from pathlib import Path
text = os.environ['SCROLL_SELECTION']
lines = text.splitlines()
assert len(lines) > 60, (len(lines), text)
numbers = [int(line.removeprefix('L')) for line in lines]
assert numbers == list(range(numbers[0], numbers[-1] + 1)), numbers
assert Path(os.environ['SCROLL_CAPTURE']).read_bytes() == b''
PY
  # Release stops the timer; returning the pointer cannot resume a drag.
  sleep .4
  xdotool mousemove --window "$window" 80 230
  key ctrl+c
  assert_clipboard "$SCROLL_SELECTION"
  # Wheel + a stationary held local gesture must also extend its endpoint.
  key Escape
  if [[ "$reporting" == on ]]; then xdotool keydown Shift; fi
  xdotool mousemove --window "$window" 80 230
  sleep .15
  xdotool mousedown 1
  sleep .15
  xdotool mousemove --window "$window" 0 190
  sleep .15
  xdotool click --repeat 5 --delay 40 4
  xdotool mouseup 1
  if [[ "$reporting" == on ]]; then xdotool keyup Shift; fi
  key ctrl+c
  export WHEEL_SELECTION="$(clipboard)"
  python3 - <<'PY'
import os
lines = os.environ['WHEEL_SELECTION'].splitlines()
assert len(lines) > 10, lines
numbers = [int(line.removeprefix('L')) for line in lines]
assert numbers == list(range(numbers[0], numbers[-1] + 1)), numbers
PY
  [[ "$(wc -c < "$capture")" -eq 0 ]] || { echo 'Local scroll/selection reached child'; exit 1; }
  # The header's Oldest / pages / latest controls move only the local viewport.
  # Jumping must not replace an existing selected range with visible text.
  smoke_click "$((WIDTH-150))" 120
  key ctrl+c
  assert_clipboard "$WHEEL_SELECTION"
  select_scrollback_probe_row
  assert_clipboard L0001
  smoke_click "$((WIDTH-52))" 120
  key ctrl+c
  assert_clipboard L0001
  select_scrollback_probe_row
  page_line="$(clipboard)"
  [[ "$page_line" =~ ^L[0-9]{4}$ ]] && ((10#${page_line#L} > 10 && 10#${page_line#L} < 100)) || { echo "Unexpected page-down line: $page_line"; exit 1; }
  smoke_click "$((WIDTH-84))" 120
  select_scrollback_probe_row
  assert_clipboard L0001
  smoke_click "$((WIDTH-20))" 120
  select_scrollback_probe_row
  latest_line="$(clipboard)"
  [[ "$latest_line" =~ ^L[0-9]{4}$ ]] && ((10#${latest_line#L} > 250)) || { echo "Unexpected latest line: $latest_line"; exit 1; }
  [[ "$(wc -c < "$capture")" -eq 0 ]] || { echo 'Local navigation reached child'; exit 1; }
  cleanup_app
done
echo 'Terminal edge autoscroll, release, held-wheel selection and history navigation passed.'
