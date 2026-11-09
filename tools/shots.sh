#!/bin/bash
# Screenshots of the running app on a private Xvfb display (adapted from
# Plaza's tools/shots.sh). The app runs --muted: the engine and clock run, so
# the playhead moves, but every sample sent to the speakers is silence.
cd "$(dirname "$0")/.."
EXE=${REDLIGHT_EXE:-target/release/redlight}
OUT=${1:-docs/screenshots}; mkdir -p "$OUT"
N=79
export REDLIGHT_DATA=$(mktemp -d)  # keep test clicks out of the real song folder
start() {
  Xvfb ":$N" -screen 0 "${1}x24" -nolisten tcp >/dev/null 2>&1 & XPID=$!
  sleep 0.7
  env -u WAYLAND_DISPLAY DISPLAY=":$N" XDG_SESSION_TYPE=x11 SLINT_BACKEND=winit-software \
    "$EXE" --muted "${@:2}" >"/tmp/redlight-shot.log" 2>&1 & APID=$!
  sleep 4
}
stop() { kill $APID $XPID 2>/dev/null; wait $APID $XPID 2>/dev/null; }
shot() { sleep "${2:-1}"; DISPLAY=:$N import -window root "$OUT/$1.png"; }
x() { tools/xclick.py :$N "$@"; }

start 1180x830 --song demo-low-sun-slow-roll --play; shot playing 1.3; stop
start 1180x830 --song demo-night-signal-electro; x click 823 98; shot open-menu 0.8; stop
start 1180x830 --song demo-dusty-boom-bap; x click 1071 98; shot about 0.8; stop
# Interaction check: Space starts it, clicks add snare hits and an accent, B then A.
start 1180x830 --song demo-night-signal-electro; x key space; x click 183 260; x click 663 260; x click 263 169; x click 785 39; x click 745 39; shot edited 1.0; stop
