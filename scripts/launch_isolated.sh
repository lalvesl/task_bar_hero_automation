#!/usr/bin/env bash
#
# Point a Steam game at the private X server, so it can be automated in the
# background without touching the host desktop.
#
# Set this as the Steam launch option for TBH: Task Bar Hero (app id 3678970),
# and for that game only:
#
#   /path/to/scripts/launch_isolated.sh %command% -screen-fullscreen 0
#
# Start the server first, on the host:
#
#   ./scripts/xserver.sh start
#
# ── Why this script does not start the server itself ─────────────────────────
#
# Steam launch options run inside Steam's bubblewrap FHS sandbox, whose command
# line ends with:
#
#   --tmpfs /tmp/.X11-unix --ro-bind-try /tmp/.X11-unix/X0 /tmp/.X11-unix/X0
#
# Inside the sandbox /tmp/.X11-unix is a private tmpfs holding only X0, so this
# script can neither see the host's socket for the isolated display nor create
# one the bot could reach. An earlier version tried to start the server from
# here; it spawned a second X server that died fighting for the TCP port, and
# the game never launched.
#
# So the server is managed on the host by xserver.sh, and reached from in here
# over TCP, which the sandbox does not isolate. Authentication is an MIT magic
# cookie in an auth file under /run, which the sandbox does bind.
#
# Environment overrides:
#   TBH_DISPLAY      X display number to use            (default :9)
#   TBH_FRAME_RATE   DXVK frame cap, 0 disables the cap (default 10)

set -euo pipefail

display="${TBH_DISPLAY:-:9}"
frame_rate="${TBH_FRAME_RATE:-10}"
display_num="${display#:}"
port=$((6000 + display_num))

state_dir="${XDG_RUNTIME_DIR:-/tmp}/tbh-automation"
auth="$state_dir/Xauthority"

if ! (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
  echo "tbh: no X server on $display (tcp $port). Run scripts/xserver.sh start first." >&2
  exit 1
fi
if [[ ! -r "$auth" ]]; then
  echo "tbh: auth file $auth is missing or unreadable." >&2
  exit 1
fi

# WAYLAND_DISPLAY is unset so Wine picks its X11 driver instead of talking to
# the host compositor, which would defeat the isolation.
unset WAYLAND_DISPLAY

exec env \
  DISPLAY="127.0.0.1:${display_num}" \
  XAUTHORITY="$auth" \
  SDL_VIDEODRIVER=x11 \
  DXVK_FRAME_RATE="$frame_rate" \
  "$@"
