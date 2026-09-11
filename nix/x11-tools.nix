# X11 command-line tools used by the spike and calibration scripts.
#
# These are not build inputs. `scripts/inspect_isolated.sh` reaches for them to
# describe the isolated display, and having them in the dev shell means that
# script does not have to pull them from the registry on every run.
#
# There are deliberately no X11 C libraries here: the capture and input crates
# speak the X protocol through `x11rb`, which is pure Rust and talks to the
# display socket directly.
pkgs: with pkgs; [
  xdotool
  xwininfo
  xdpyinfo
  imagemagick
  xorg-server # provides Xvfb and Xephyr
]
