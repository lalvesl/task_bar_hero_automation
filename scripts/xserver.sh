#!/usr/bin/env bash
#
# Manage the private X server the game and the bot share, and the loopback
# forwarder that lets Steam's sandbox reach it.
#
# This runs on the host, never inside Steam's sandbox. See the comment in
# launch_isolated.sh for why the split exists.
#
#   ./scripts/xserver.sh start [Xvfb|Xephyr]
#   ./scripts/xserver.sh status
#   ./scripts/xserver.sh stop
#
# ── Why a forwarder and not the server's own TCP ─────────────────────────────
#
# Steam's sandbox replaces /tmp/.X11-unix with a tmpfs holding only X0, so the
# game cannot reach this display over a unix socket. It can reach a TCP port,
# because the sandbox does not isolate the network.
#
# But an X server told to `-listen tcp` binds 0.0.0.0, and Xorg has no option to
# choose the address. That would put the display on every network interface,
# behind nothing but a cookie. So the server listens on its unix socket only,
# and socat forwards 127.0.0.1:<6000+display> into it. The sandbox reaches the
# loopback port; no network interface sees anything.
#
# Environment overrides:
#   TBH_DISPLAY   X display number to use     (default :9)
#   TBH_SCREEN    virtual screen geometry     (default 1800x1200x24)

set -euo pipefail

display="${TBH_DISPLAY:-:9}"
screen="${TBH_SCREEN:-1800x1200x24}"
display_num="${display#:}"
port=$((6000 + display_num))

state_dir="${XDG_RUNTIME_DIR:-/tmp}/tbh-automation"
mkdir -p "$state_dir"
log="$state_dir/xserver.log"
auth="$state_dir/Xauthority"
pidfile="$state_dir/xserver.pid"
proxy_pidfile="$state_dir/proxy.pid"
socket="/tmp/.X11-unix/X${display_num}"

# The loopback port is the liveness check: it is the one endpoint both the host
# and Steam's sandbox can see.
port_open() { (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; }

# Kill whatever a pid file names, falling back to the process holding the port.
stop_pid() {
  local file="$1" pid=""
  [[ -f "$file" ]] && pid=$(cat "$file")
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
  fi
  rm -f "$file"
}

case "${1:-status}" in
  start)
    xserver="${2:-${TBH_XSERVER:-Xvfb}}"
    if port_open; then
      echo "already running on $display (127.0.0.1:$port)"
      exit 0
    fi

    if ! command -v socat >/dev/null; then
      echo "FAIL: socat is not on PATH; enter the dev shell or install it" >&2
      exit 1
    fi

    # One cookie per server lifetime, registered for both the unix socket (used
    # by the bot, which runs on the host) and the loopback endpoint (used by the
    # game, through the forwarder). The same cookie value satisfies either,
    # because MIT-MAGIC-COOKIE-1 authenticates the bytes, not the transport.
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
      Xephyr) "$xserver" "$display" -screen "${screen%x*}" -auth "$auth" -resizeable >"$log" 2>&1 & ;;
      *)      "$xserver" "$display" -screen 0 "$screen" -auth "$auth" >"$log" 2>&1 & ;;
    esac
    echo $! >"$pidfile"

    for _ in $(seq 50); do
      [[ -S "$socket" ]] && break
      sleep 0.1
    done
    if [[ ! -S "$socket" ]]; then
      echo "FAIL: $xserver did not come up on $display; see $log" >&2
      exit 1
    fi

    socat "TCP-LISTEN:${port},bind=127.0.0.1,reuseaddr,fork" \
      "UNIX-CONNECT:${socket}" >"$state_dir/proxy.log" 2>&1 &
    echo $! >"$proxy_pidfile"

    for _ in $(seq 50); do
      port_open && break
      sleep 0.1
    done
    if ! port_open; then
      echo "FAIL: the forwarder did not come up on 127.0.0.1:$port" >&2
      exit 1
    fi
    echo "up: display $display, socket $socket, loopback 127.0.0.1:$port, auth $auth"
    ;;

  stop)
    stop_pid "$proxy_pidfile"
    stop_pid "$pidfile"

    for _ in $(seq 30); do
      port_open || break
      sleep 0.1
    done
    if port_open; then
      echo "FAIL: something is still listening on 127.0.0.1:$port" >&2
      exit 1
    fi
    echo "stopped"
    ;;

  status)
    if port_open; then
      echo "running: display $display, socket $socket, loopback 127.0.0.1:$port"
      echo "listening:"
      ss -ltnH "sport = :$port" 2>/dev/null | awk '{print "  " $4}'
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
