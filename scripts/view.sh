#!/usr/bin/env bash
#
# Mirror the isolated display onto the host desktop, in a window you can click
# in.
#
#   ./scripts/view.sh
#
# The point of the isolated display is that the game stays out of the way, so
# this is deliberately a thing you start and stop rather than something that
# runs alongside the bot. Use it to pick out buttons and cut templates, then
# close it and let the game go back to being invisible.
#
# It attaches to the running server over VNC rather than switching the server
# to Xephyr, because switching would mean restarting the display and therefore
# relaunching the game. The VNC server is bound to localhost.
#
# Environment overrides:
#   TBH_DISPLAY    the display to mirror   (default :9)
#   TBH_VNC_PORT   local port to serve on  (default 5909)

set -euo pipefail

display="${TBH_DISPLAY:-:9}"
port="${TBH_VNC_PORT:-5909}"
auth="${XDG_RUNTIME_DIR:-/tmp}/tbh-automation/Xauthority"

if [[ ! -r "$auth" ]]; then
  echo "view: no auth file at $auth. Run scripts/xserver.sh start first." >&2
  exit 1
fi

if [[ -z "${TBH_VIEW_READY:-}" ]]; then
  export TBH_VIEW_READY=1
  exec nix shell nixpkgs#x11vnc nixpkgs#tigervnc -c "$0" "$@"
fi

# -localhost keeps the mirror off the network; -shared lets a second viewer
# attach without kicking the first.
# WAYLAND_DISPLAY and XDG_SESSION_TYPE are cleared because x11vnc refuses to
# start when it detects a Wayland session, even though the display it is asked
# to mirror is a plain X server with no compositor anywhere near it.
env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE \
  x11vnc -display "$display" -auth "$auth" \
  -rfbport "$port" -localhost -nopw -forever -shared -quiet \
  >"${XDG_RUNTIME_DIR:-/tmp}/tbh-automation/x11vnc.log" 2>&1 &
vnc_pid=$!
trap 'kill "$vnc_pid" 2>/dev/null || true' EXIT

for _ in $(seq 50); do
  (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null && break
  sleep 0.1
done

echo "view: mirroring $display, close the viewer window to stop"
vncviewer "localhost:$port"
