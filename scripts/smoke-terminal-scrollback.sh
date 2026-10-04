# Sourced by smoke-linux.sh; uses its isolated display, app/clipboard helpers,
# settings paths and cleanup trap. Also run via --terminal-scrollback.
printf 'theme = dark\n' > "$KEA_SETTINGS"
drag_indicator_hash() {
  # The fixed-font fixture's first terminal-context words: "Selecting..."
  # while a local drag is held, "Local selection..." after it is released.
  # Exclude the changing offset counter and hash pixels, not image metadata.
  import -window "$window" -crop 120x20+120+110 rgb:- | sha256sum | cut -d' ' -f1
}
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
  put_clipboard kea-scrollback-edge-sentinel
  if [[ "$reporting" == on ]]; then xdotool keydown Shift; fi
  xdotool mousemove --window "$window" 80 230
  sleep .15
  idle_indicator="$(drag_indicator_hash)"
  xdotool mousedown 1
  sleep .15
  # Outside the terminal hitbox, over the title bar. Use the documented maximum
  # speed zone so the same multi-screen assertion needs fewer dispatched timer
  # ticks under CPU load. No further pointer movement may be needed for growth.
  xdotool mousemove --window "$window" 0 15
  if [[ "$reporting" == on ]]; then xdotool keyup Shift; fi
  held_indicator="$idle_indicator"
  for _ in $(seq 1 60); do
    held_indicator="$(drag_indicator_hash)"
    [[ "$held_indicator" != "$idle_indicator" ]] && break
    sleep .1
  done
  [[ "$held_indicator" != "$idle_indicator" ]] || { echo 'Local drag ownership was not painted'; exit 1; }
  # Observe selection growth, not an exact number of timer ticks under load.
  # Do not use key() here: xdotool --clearmodifiers also releases held buttons.
  # Dispatch can be much slower than the nominal 50 ms timer while compiling
  # with a software renderer. Bound observation, not a presumed frame rate.
  for _ in $(seq 1 120); do
    sleep .1
    xdotool key --delay 50 ctrl+c
    SCROLL_SELECTION="$(clipboard)"
    if [[ "$(printf '%s\n' "$SCROLL_SELECTION" | wc -l)" -gt 60 ]]; then break; fi
  done
  xdotool mouseup 1
  # A painted change from Selecting to Local selection acknowledges that the
  # application handled release and all earlier held Copy keys. Waiting only
  # for a different clipboard value can accept an older queued Copy result.
  released_indicator="$held_indicator"
  for _ in $(seq 1 60); do
    released_indicator="$(drag_indicator_hash)"
    [[ "$released_indicator" != "$held_indicator" ]] && break
    sleep .1
  done
  [[ "$released_indicator" != "$held_indicator" ]] || { echo 'Local drag release was not painted'; exit 1; }
  # The last held Copy may still own the clipboard while the release and final
  # Copy are queued. Require a fresh copy before recording the release baseline.
  put_clipboard kea-scrollback-release-sentinel
  key ctrl+c
  for _ in $(seq 1 30); do
    SCROLL_SELECTION="$(clipboard)"
    [[ "$SCROLL_SELECTION" != kea-scrollback-release-sentinel ]] && break
    sleep .1
  done
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
  printf 'Scrollback %s release baseline: %s lines\n' "$reporting" "$(printf '%s\n' "$SCROLL_SELECTION" | wc -l)"
  # Release stops the timer; returning the pointer cannot resume a drag.
  sleep .4
  xdotool mousemove --window "$window" 80 230
  put_clipboard kea-scrollback-stability-sentinel
  key ctrl+c
  assert_clipboard "$SCROLL_SELECTION"
  echo "Scrollback $reporting release stability passed"
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
  put_clipboard kea-scrollback-wheel-sentinel
  key ctrl+c
  for _ in $(seq 1 30); do
    WHEEL_SELECTION="$(clipboard)"
    [[ "$WHEEL_SELECTION" != kea-scrollback-wheel-sentinel ]] && break
    sleep .1
  done
  export WHEEL_SELECTION="$(clipboard)"
  python3 - <<'PY'
import os
lines = os.environ['WHEEL_SELECTION'].splitlines()
assert len(lines) > 10, lines
numbers = [int(line.removeprefix('L')) for line in lines]
assert numbers == list(range(numbers[0], numbers[-1] + 1)), numbers
PY
  printf 'Scrollback %s wheel baseline: %s lines\n' "$reporting" "$(printf '%s\n' "$WHEEL_SELECTION" | wc -l)"
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
  if [[ "$reporting" == on ]]; then
    # A frozen grid intentionally retains its old size. Shrinking clips it;
    # the new visible bottom, not the old bottom behind the composer, must
    # remain the local drag's edge. The snapshot must not reflow to achieve it.
    smoke_click "$((WIDTH-150))" 120
    key Escape
    xdotool keydown Control_L keydown Shift_L
    xdotool mousemove --window "$window" 0 190
    xdotool mousedown 1
    xdotool mousemove --window "$window" 80 190
    xdotool mouseup 1
    xdotool keyup Shift_L keyup Control_L
    xdotool windowsize --sync "$window" 1050 600
    sleep .3
    xdotool mousemove --window "$window" 0 300
    xdotool mousedown 1
    xdotool mousemove --window "$window" 80 420
    printf '%s' kea-frozen-growth-sentinel | xclip -selection clipboard
    xdotool key --delay 50 ctrl+c
    for _ in $(seq 1 30); do
      frozen_before="$(clipboard)"
      [[ "$frozen_before" != kea-frozen-growth-sentinel ]] && break
      sleep .1
    done
    frozen_before="$(clipboard)"
    before_lines="$(printf '%s\n' "$frozen_before" | wc -l)"
    for _ in $(seq 1 30); do
      sleep .1
      xdotool key --delay 50 ctrl+c
      frozen_after="$(clipboard)"
      after_lines="$(printf '%s\n' "$frozen_after" | wc -l)"
      ((after_lines > before_lines + 3)) && break
    done
    xdotool mouseup 1
    ((after_lines > before_lines + 3)) || { echo 'Frozen drag did not scroll at the clipped visible bottom'; exit 1; }
    key ctrl+c
    frozen_released="$(clipboard)"
    sleep .3
    key ctrl+c
    assert_clipboard "$frozen_released"
    [[ "$(wc -c < "$capture")" -eq 0 ]] || { echo 'Frozen resize/drag reached child'; exit 1; }
  fi
  cleanup_app
done
echo 'Terminal edge autoscroll, release, held-wheel selection, history navigation and clipped frozen dragging passed.'
