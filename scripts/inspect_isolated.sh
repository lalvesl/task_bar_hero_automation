#!/usr/bin/env bash
#
# Report on the isolated display: is the game up, what is its window geometry,
# what renderer did it pick, and does it accept a synthetic click.
#
# This is the M1 acceptance check from docs/implementation_plan.md. It is shell
# on purpose: the point is to answer the go / no-go question before any Rust
# exists that would have to be rewritten.
#
#   ./scripts/inspect_isolated.sh            # report only
#   ./scripts/inspect_isolated.sh --click    # also send one synthetic click

set -euo pipefail

display="${TBH_DISPLAY:-:9}"
app_id=3678970
out_dir="${1:-}"
[[ "$out_dir" == --* ]] && out_dir=""
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out_dir="${out_dir:-$repo_root/artifacts}"
mkdir -p "$out_dir"

click=false
for arg in "$@"; do [[ "$arg" == "--click" ]] && click=true; done

# The helper X tools are not in the system closure; pull them from the pinned
# registry nixpkgs for the duration of this script only.
if [[ -z "${TBH_TOOLS_READY:-}" ]]; then
  export TBH_TOOLS_READY=1
  exec nix shell nixpkgs#xorg.xwininfo nixpkgs#xorg.xdpyinfo nixpkgs#xdotool \
    nixpkgs#imagemagick -c "$0" "$@"
fi

if [[ ! -e "/tmp/.X11-unix/X${display#:}" ]]; then
  echo "FAIL: no X server on $display. Launch the game through scripts/launch_isolated.sh first." >&2
  exit 1
fi
export DISPLAY="$display"
export XAUTHORITY="${XAUTHORITY:-${XDG_RUNTIME_DIR:-/tmp}/tbh-automation/Xauthority}"

echo "== display =="
xdpyinfo | grep -E "^(name of display|number of screens)|dimensions:|depth of root"

echo
echo "== windows =="
mapfile -t wins < <(xdotool search --onlyvisible --name . 2>/dev/null || true)
if [[ ${#wins[@]} -eq 0 ]]; then
  echo "FAIL: no visible window on $display. The game did not render." >&2
  exit 1
fi
for w in "${wins[@]}"; do
  name=$(xdotool getwindowname "$w" 2>/dev/null || echo "?")
  geom=$(xdotool getwindowgeometry "$w" 2>/dev/null | tr '\n' ' ')
  echo "  $w  '$name'  $geom"
done

# Pick the game window by name; fall back to the largest one.
target=$(xdotool search --onlyvisible --name "TaskbarHero|Task ?Bar ?Hero" 2>/dev/null | head -1 || true)
[[ -z "$target" ]] && target="${wins[0]}"
echo
echo "== target window: $target ($(xdotool getwindowname "$target" 2>/dev/null || echo '?')) =="
xwininfo -id "$target" | grep -E "Absolute|Width:|Height:"

shot="$out_dir/isolated-$(date +%Y%m%d-%H%M%S).png"
import -window "$target" "$shot" 2>/dev/null || import -window root "$shot"
echo "screenshot: $shot"

echo
echo "== renderer (from the game's own log) =="
log="$HOME/.steam/steam/steamapps/compatdata/$app_id/pfx/drive_c/users/steamuser/AppData/LocalLow/TesseractStudio/TaskbarHero/Player.log"
if [[ -f "$log" ]]; then
  grep -E "Renderer:|Vendor:|Version: +Direct3D|VRAM:" "$log" | head -4
else
  echo "  (no Player.log yet at $log)"
fi

if $click; then
  echo
  echo "== synthetic click =="
  read -r _ _ w h < <(xwininfo -id "$target" | awk '/Width:/{w=$2} /Height:/{h=$2} END{print "x", "y", w, h}')
  # Click dead centre: harmless in most UIs and enough to prove XTEST lands.
  xdotool mousemove --window "$target" $((w / 2)) $((h / 2)) click 1
  echo "  clicked centre of window $target ($((w / 2)),$((h / 2)))"
  sleep 1
  after="$out_dir/isolated-afterclick-$(date +%Y%m%d-%H%M%S).png"
  import -window "$target" "$after" 2>/dev/null || import -window root "$after"
  echo "  screenshot after click: $after"
  echo "  compare the two PNGs: a visible UI change means XTEST works."
fi
