#!/usr/bin/env bash
# Sourced by the Xvfb smoke fixtures; coordinates are window-relative.
# Do not batch a pointer warp and an instantaneous click: let the UI
# dispatch motion and press separately before sending the matching up.
# Each action is sent exactly once; callers assert the resulting state.
smoke_click() {
  xdotool mousemove --window "$window" "$1" "$2"
  sleep .15
  xdotool mousedown 1
  sleep .15
  xdotool mouseup 1
  sleep .15
}
