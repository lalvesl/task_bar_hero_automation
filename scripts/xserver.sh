#!/usr/bin/env bash
#
# Manage the private X server the game and the bot share.
#
# This runs on the host, never inside Steam's sandbox. See the comment in
# launch_isolated.sh for why the split exists.
#
#   ./scripts/xserver.sh start [Xvfb|Xephyr]
#   ./scripts/xserver.sh status
#   ./scripts/xserver.sh stop
#
# Environment overrides:
#   TBH_DISPLAY   X display number to use     (default :9)
#   TBH_SCREEN    virtual screen geometry     (default 1024x960x24)

set -euo pipefail

display="${TBH_DISPLAY:-:9}"
screen="${TBH_SCREEN:-1024x960x24}"
display_num="${display#:}"
port=$((6000 + display_num))

state_dir="${XDG_RUNTIME_DIR:-/tmp}/tbh-automation"
mkdir -p "$state_dir"
log="$state_dir/xserver.log"
auth="$state_dir/Xauthority"
pidfile="$state_dir/xserver.pid"
socket="/tmp/.X11-unix/X${display_num}"

# The TCP port is the authoritative liveness check: it is the one endpoint both
# the host and Steam's sandbox can see.
port_open() { (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; }

case "${1:-status}" in
  start)
    xserver="${2:-${TBH_XSERVER:-Xvfb}}"
    if port_open; then
      echo "already running on $display (tcp $port)"
      exit 0
    fi

    # One cookie per server lifetime, registered for the local socket (used by
    # the bot, which runs on the host) and the TCP endpoint (used by the game,
    # which runs inside the sandbox and cannot see the socket).
    cookie=$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')
    rm -f "$auth"
    : >"$auth"
    xauth -q -f "$auth" add "unix:${display_num}" MIT-MAGIC-COOKIE-1 "$cookie"
    xauth -q -f "$auth" add "127.0.0.1:${display_num}" MIT-MAGIC-COOKIE-1 "$cookie"
    chmod 600 "$auth"

    echo "starting $xserver on $display ($screen), log at $log"
    case "$xserver" in
      # Xephyr draws into a window on the host desktop, which is how you watch
      # what the bot sees. It needs the host DISPLAY to open that window.
      Xephyr) "$xserver" "$display" -screen "${screen%x*}" -listen tcp -auth "$auth" -resizeable >"$log" 2>&1 & ;;
      *)      "$xserver" "$display" -screen 0 "$screen" -listen tcp -auth "$auth" >"$log" 2>&1 & ;;
    esac
    echo $! >"$pidfile"

    for _ in $(seq 50); do
      port_open && break
      sleep 0.1
    done
    if ! port_open; then
      echo "FAIL: $xserver did not come up on $display; see $log" >&2
      exit 1
    fi
    echo "up: display $display, tcp $port, auth $auth"
    ;;

  stop)
    # The pid file can be stale, or absent when the server outlived the shell
    # that started it. The process holding the TCP port is the ground truth.
    pid=""
    [[ -f "$pidfile" ]] && pid=$(cat "$pidfile")
    if [[ -z "$pid" ]] || ! kill -0 "$pid" 2>/dev/null; then
      pid=$(ss -ltnpH "sport = :$port" 2>/dev/null | grep -oP "pid=\\K[0-9]+" | head -1 || true)
    fi

    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 30); do
        port_open || break
        sleep 0.1
      done
    fi
    rm -f "$pidfile"

    if port_open; then
      echo "FAIL: something is still listening on tcp $port" >&2
      exit 1
    fi
    echo "stopped"
    ;;

  status)
    if port_open; then
      echo "running: display $display, tcp $port, socket $socket, auth $auth"
    else
      echo "not running"
      exit 1
    fi
    ;;

  *)
    echo "usage: $0 {start [Xvfb|Xephyr]|stop|status}" >&2
    exit 2
    ;;
esac
