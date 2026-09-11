# TBH Automation — Implementation Plan

Farming automation for **TBH: Task Bar Hero** (TesseractStudio, Unity 6, Steam
app id `3678970`), running on Linux through Proton.

## 1. Scope

In scope for v1:

- **Chest task** — detect dropped chests (blue / brown / red boss) and click them.
- **Cube task** — open the cube, click auto-fill, click synthesize for as long as
  auto-fill keeps succeeding, then close.

Out of scope for v1: route selection, chapter/difficulty switching, boss
detection, stash deposit, market pricing, memory reading, code injection.

### 1.1 Approach and what it rules out

Two approaches exist in the wild. Memory reading (`tbh-meter`) and runtime
patching (`TaskBarHero-Modded`) both break on every game patch because IL2CPP
offsets move, and under Proton they would mean reading a process inside a Wine
prefix through a second address-translation layer. We take the other route:
**screen capture plus synthetic input**, like `task-hero-auto` and
`MAA-Task-Bar-Hero`. It survives game updates with a template swap instead of a
reverse-engineering session.

### 1.2 What counts as "game information"

We never read gold, XP, item counts, or rarity. Three signals are enough, and
each extra one would be one more thing to break at the next patch:

| Signal | Source | Cost |
| --- | --- | --- |
| Window rectangle | X11 metadata | Negligible |
| Is a chest on screen, and where | Template match on the frame | High, so bounded by region and interval |
| Is the synthesize button enabled | Color of a few pixels inside the button | Negligible |

## 2. The isolated display

**Requirement: the bot runs in the background while the machine is used
normally.**

Capture was never the obstacle. Synthetic input was. `XTEST` moves the real
pointer and delivers to whatever holds focus; `uinput` is worse, being a virtual
hardware device that is global by construction. Either one steals the user's
mouse.

The answer is not better injection. It is **running the game on a display of its
own**. The game gets its own X server, the bot connects to that server, and the
host desktop is never touched because it is a different server entirely.

### 2.1 Xvfb, with Xephyr for debugging

`Xvfb` is a real X server that renders into memory and shows nothing. `Xephyr`
is the same thing in a window you can open when you want to watch what the bot
sees. Same code path against either; only the launcher differs.

Both are already in the system closure on the target machine.

Consequences, all of them simplifications:

- **`XTEST` risk collapses.** These are reference X server implementations where
  `XTEST` has worked forever. The original worry was a modern nested compositor
  filtering synthetic pointer events.
- **The `uinput` backend is dropped.** It injects into the host seat, so its
  events would land on the user's desktop, not on the isolated display. It was
  the right answer to the previous design and is useless in this one.
- **`XComposite` is dropped.** With no compositor and no other windows on that
  display, the game window is never occluded, so a plain `XGetImage` suffices.
- **The safety rules shrink.** "Act only while focused" and "pause on mouse
  movement" existed so the bot would not fight the user. On a separate display
  there is nothing to fight.

### 2.2 GPU

The game's own log shows it rendering through Direct3D 11, translated by DXVK to
Vulkan, on the physical GPU:

```
Initialize engine version: 6000.0.72f1
Direct3D: Version:  Direct3D 11.0 [level 11.1]
          Renderer: NVIDIA GeForce GTX 1050
```

Vulkan does not bind device selection to the X server the way GLX did. It
renders on the real GPU and only needs an X surface to present to. So the
physical GPU should remain in play under `Xvfb`.

That is reasoning, not measurement. M1 measures it. Either outcome is
acceptable, which is why M1 is not a gate on this question.

### 2.3 Power and heat

The bot is meant to run for hours, on a machine whose CPU clock is deliberately
held down to limit heat and draw. That budget shapes two defaults.

**Hardware acceleration is the low-power option, not the high-power one.**
Software rasterizers (`llvmpipe`, `lavapipe`) do not save energy; they move the
rendering work from the GPU onto the CPU. Combined with a forced-low CPU clock
that is the worst of both: the throttled component takes on all the graphics
work and the frame rate collapses. A GTX 1050 drawing a tiny 2D game sits at
idle clocks instead.

The real levers, all of which live in the launch wrapper or the bot config:

| Lever | Mechanism | Effect |
| --- | --- | --- |
| Frame rate cap | `DXVK_FRAME_RATE` | Cuts most GPU and CPU work; an idle game does not need a high frame rate |
| Small resolution | `Xvfb` screen size plus Unity `-screen-width` / `-screen-height` | Less work for the game, and template matching cost scales with area |
| Chest poll interval | Bot config | Each capture-and-match cycle is CPU; the interval sets how often it is paid |

### 2.4 Blast radius

Nothing here is a system-level change:

- `Xvfb` is an ordinary user process on an unused display number, started on
  demand and killed with the game.
- The only persistent change is a launch-option line on app id `3678970`. Steam
  launch options are per game.
- Wine prefixes are already isolated per app under `compatdata`.
- No kernel module, no udev rule. The udev rule belonged to the dropped `uinput`
  path.
- Cost when not running is zero. While running, `Xvfb` is an idle in-memory
  framebuffer; the real cost is the game, which is paid either way.

Every other tool lives in the project flake's dev shell, so it exists only
inside `nix develop` in this directory.

### 2.5 Launching into the isolated display

`steam` cannot simply be run with another `DISPLAY`: a second invocation
delegates to the already-running instance, which launches on the user's desktop.
The game must therefore be started through a **wrapper script set as the launch
option** for app id `3678970`. The wrapper starts the isolated server if it is
not up, then execs the game command with `DISPLAY` pointed at it. It is also
where the frame rate cap and the screen size from 2.3 are applied.

## 3. Architecture

A Cargo workspace of five crates. Each boundary is a trait, so a backend can be
swapped without touching its callers.

| Crate | Responsibility |
| --- | --- |
| `tbh-capture` | Connect to the isolated display, locate the game window, capture frames. |
| `tbh-input` | `Pointer` trait: move, click. `XTEST` backend. |
| `tbh-vision` | Template matching, pixel-signature checks, region-of-interest cropping. |
| `tbh-core` | Hardcoded state machines for the chest and cube tasks, plus the TOML config for tunables. |
| `tbh-cli` | The `tbh` binary: the control process, plus the calibration subcommands. |

### 3.1 Vision

Cheap checks first, expensive checks last:

1. **Region of interest** — never scan the whole frame when the target lives in a
   known sub-rectangle.
2. **Pixel-signature check** — sample a handful of pixels at known offsets and
   compare against expected colors, within a per-channel tolerance. This is what
   decides whether the synthesize button is enabled.
3. **Template matching** — `imageproc` normalized cross-correlation on a
   grayscale downscale. Only for targets whose position is unpredictable, which
   in v1 means dropped chests.

### 3.2 Coordinate system

No absolute screen coordinates anywhere. Every point is stored normalized
against a reference window size and resolved against the live window rectangle
at click time.

## 4. Milestones

Ordered so the approach-killing risk is tested first, before any code exists
that would have to be rewritten.

### M1 — Isolation spike (the go / no-go gate)

Pure shell, no Rust. Answers whether the whole design is viable.

- Start `Xvfb` on an unused display.
- Launch app id `3678970` into it through the wrapper script.
- Confirm the game renders at a small screen size with the frame rate capped.
- Record from its log whether the renderer is the physical card or a software
  rasterizer. Both are acceptable, so this is measurement, not a pass condition.
- Send one synthetic click with `XTEST` and confirm the game reacts.
- **Done when** the game runs on the isolated display and responds to a
  synthetic click, with the host desktop unaffected throughout.
- **If it does not render at all**, fall back to `gamescope` and re-run this
  milestone against it before writing any Rust.

### M2 — Nix flake and workspace skeleton

The current `flake.nix` is inherited from an unrelated project. It still calls
itself WaveDB, imports a `./benches/nix` directory that does not exist, and
carries wasm, SQLite, and a MongoDB unfree predicate. It does not evaluate.

- Rewrite `flake.nix` for this project; keep `nix/pkgs.nix` and `nix/rust.nix`.
- Replace `nix/gui-libs.nix` with the X11 set: `libX11`, `libXtst`, `libXext`,
  `libxcb`, plus `pkg-config`, and the spike tooling (`xdotool`, `xwd`).
- Create the five-crate workspace with workspace lints matching `clippy.toml`.
- **Done when** `nix develop` enters, `cargo check --workspace` is clean, and
  `cargo deny check` passes.

### M3 — Capture and the `calibrate` subcommand

No templates exist yet, and the ones bundled by Windows tools cannot be trusted
under Proton where font rendering and scaling may differ. So the first runnable
artifact is a calibration tool, not a bot.

- Connect to the isolated display, match the game window by `WM_CLASS` /
  `_NET_WM_NAME`, capture one frame, write a PNG and print the geometry.
- **Done when** `tbh calibrate --out frame.png` yields a correct screenshot of
  the game running on the isolated display.
- **This screenshot also answers an open design question**: whether the cube has
  a rarity selector that must be clicked before auto-fill.

### M4 — Input

- `Pointer` trait with the `XTEST` backend, targeting the isolated display.
- **Done when** a click at a normalized point produces the expected in-game UI
  response, driven from Rust rather than the M1 shell spike.

### M5 — Vision primitives

- Region cropping, pixel-signature checks, template matching with a score
  threshold.
- **Done when** each primitive has unit tests against fixture PNGs captured in
  M3, including deliberate negatives.

### M6 — Chest task

Poll the band chests drop into, find every one, click them all.

No template matching. The band sits on pure black, so a chest is a run of
columns that are not black, and a scan of a thin band replaces a correlation
over the frame. It also handles however many chests appear, rather than however
many sprites were cut into templates.

### M7 — Cube task

Fixed UI, so no template matching for the buttons: fixed normalized points plus
one pixel-signature check.

1. Open the cube.
2. Select the target rarity, if M3 shows a selector exists.
3. Click auto-fill.
4. Read the synthesize button. Enabled means it filled; disabled means the
   inventory ran out of that rarity.
5. Enabled: click synthesize, return to step 3.
6. Disabled: close the cube, sleep until the next interval.

- **Done when** a run drains the available items of the target rarity and exits
  cleanly instead of spinning.

### M8 — the control process

Not a full-screen interface. A process that stays up, drives the enabled tasks
on their intervals, and takes plain commands on stdin:

```
> enable synthesis
> status
> run synthesis
> disable synthesis
> quit
```

A ratatui interface was the original plan and was dropped. There is no screen
worth redrawing here: the operator turns a task on, checks on it occasionally,
and turns it off. Lines of text do that with none of the layout, no event loop
of its own, and it pipes and scripts like anything else.

## 5. Configuration

The state machines are hardcoded in Rust, as decided. The numbers are not,
because they get retuned constantly during calibration and recompiling per
attempt would kill the feedback loop.

`config.toml` holds: display number, match thresholds, poll intervals, the cube
interval, the target rarity, normalized regions of interest, and normalized
click points. The chest poll interval doubles as the bot's own power knob, per
section 2.3.

## 6. Safety

Display isolation removes most of what a foreground bot needs to guard against.
What remains:

- A stop signal that halts all tasks and leaves the game untouched.
- A hard cap on clicks per interval, so a mismatch cannot turn into a click
  storm.
- Never act before the game window has been located on the isolated display.

## 7. Open questions

- Does the game render on the physical GPU under `Xvfb`, or fall back to a
  software rasterizer? Measured in M1; both outcomes are workable.
- Does the cube have a rarity selector preceding auto-fill? Resolved by M3.
- Is `imageproc` fast enough for chest polling, or is an `opencv` binding
  needed? Measured in M6; pure Rust stays unless the numbers say otherwise.

## 8. Milestone results

### M1 — passed, 2026-09-11

The game runs on an isolated `Xvfb` display, on the physical GPU, and responds
to synthetic clicks, with the host desktop untouched. The offline-rewards dialog
was dismissed by an `XTEST` click at screen coordinates, which is unambiguous
visual proof.

Two findings changed the design along the way.

**Steam's sandbox hides the display socket.** Launch options run inside Steam's
bubblewrap FHS sandbox, whose command line ends with `--tmpfs /tmp/.X11-unix
--ro-bind-try /tmp/.X11-unix/X0 /tmp/.X11-unix/X0`. Only `X0` exists in there,
so the game cannot reach any other display over a unix socket. The server
therefore listens on TCP, which the sandbox does not isolate, authenticated by
an MIT magic cookie in an auth file under `/run`, which the sandbox does bind.

**The launch wrapper cannot manage the server.** A first version started the X
server from inside the wrapper. Because the wrapper runs inside the sandbox, it
could not see the host's socket, so it spawned a second X server that died
fighting for the TCP port, and the game never launched. Server management moved
to `scripts/xserver.sh`, which runs on the host; the wrapper only points the
game at it.

The game window measures 970x892, so the virtual screen is 1024x960. An 800-tall
screen clipped the window's top rows.

### M2 — passed, 2026-09-11

The flake was rewritten for this project. The WaveDB description, the
`./benches/nix` import, the wasm target and its `wasm-bindgen-cli` build, the
SQLite and MongoDB inputs, and the four demo apps are all gone. `nix/pkgs.nix`
and `nix/rust.nix` survived; `nix/gui-libs.nix` became `nix/x11-tools.nix`.

`nix/x11-tools.nix` holds command-line tools, not build inputs. There are no X11
C libraries anywhere in the shell, because `x11rb` is pure Rust and speaks the
protocol to the display socket directly.

The workspace is the five crates from section 3, on edition 2024 with the
workspace lints pedantic and nursery set to warn. `scripts/check_file_length.sh`
now exists and enforces the 350-line budget that `clippy.toml` describes but
clippy itself cannot check.

Verified: `nix flake check` passes, `nix build .#default` produces a running
`tbh` binary, and `cargo check`, `cargo clippy`, `cargo fmt --check` and
`cargo deny check` are all clean.

One inherited item was dropped: `deny.toml` ignored `RUSTSEC-2023-0089` for
`atomic-polyfill`, which no crate in this dependency tree pulls in.

### M3 and M4 — passed, 2026-09-11

`tbh calibrate` connects to the isolated display, finds the window by title,
captures it and writes a PNG. `tbh click` and `tbh key` drive the pointer and
keyboard through `XTEST`. All of it is `x11rb`; no `xdotool` remains in the
Rust path.

Three findings.

**Windows must be nudged on-screen.** The isolated display runs no window
manager, so the game places itself and nothing corrects it; it landed at a
negative y offset. `GetImage` on a window rejects any rectangle not wholly
within the visible screen, so capture failed outright. `ensure_onscreen` moves
the window before capturing, and errors clearly when the window is larger than
the virtual screen rather than returning a silently clipped frame.

**Synthetic clicks need to dwell.** A motion, press and release sent back to
back did nothing. Unity samples input once per frame, and the game runs near 30
frames per second, so all three events landed inside one frame and the engine
never saw a transition. A 50ms dwell between events fixed it. This is why the
`xdotool` spike worked where the first Rust version did not: `xdotool` inserts
its own delays.

**The window size is the player's own setting.** Unity stores it in the Wine
registry under `Software\TesseractStudio\TaskBarHero`, and it does not track the
virtual screen size in any predictable way. The virtual screen is therefore
sized to fit the window, not the other way around.

The UI map that came out of calibration lives in `docs/ui_map.md`, including the
answer to the open question about the cube's rarity selector, and the two states
of the synthesize button.

### M5, M7 and M8 — passed, 2026-09-11

The cube task runs end to end, and the control process is up.

**The vision layer is smaller than planned.** Because the game's panels hold
fixed positions, the cube needs no template matching at all. What it needs is
one predicate: does the blue channel lead the red channel by a margin, at three
sampled pixels. `ChannelLead` in `tbh-vision` is that, with the real disabled
and enabled colours from the game as its unit tests. Template matching is still
coming, but only for M6's chests, whose position is genuinely unpredictable.

**Why a relative colour test and not a stored colour.** Disabled, the button is
pure grey and all three channels are equal. Enabled, it is blue and leads red by
around 80. Comparing two channels of the same pixel survives brightness drift,
hover highlighting and a repaint in a future patch; comparing against a stored
RGB triple would have to be recalibrated after any of those.

**Two stops, not one.** The task stops when auto-fill can no longer fill the
grid, read off the button. It also stops at `max_per_run`, which does not depend
on reading the screen correctly at all. If the colour check ever misreads, the
second stop is what keeps the loop finite.

**The panel is cycled between syntheses.** The synthesised item is left sitting
in the grid, and toggling the cube panel clears it. That is cheaper and more
reliable than finding and clicking the grid's own clear control.

**The task ships switched off.** A bot that clicks on its own should not start
clicking merely because the process came up.

**`XAUTHORITY` is handled by a wrapper, not by the binary.** Setting an
environment variable from Rust now requires `unsafe`, and the workspace forbids
it. `scripts/tbh` points the variable at the cookie `xserver.sh` wrote. A policy
worth having is worth not bending for a one-line convenience.

The crate formerly called `tbh-tui` is now `tbh-cli`, since the old name
describes an interface that is no longer being built.

### M6 — passed, 2026-09-11

The chest task finds and clicks every chest in the band, and the control process
drives it alongside the cube.

**The chests are not drops.** They are permanent slots holding a queue, and the
dots underneath are the count. Clicking collects one. So the slots do not
disappear while the queue has anything in it, and a pass that reports the same
two chests every time is correct rather than stuck.

**A gap tolerance as wide as the gap merged both chests into one.** The two
slots sit six columns of background apart, and `max_gap` was set to 0.005, which
is six pixels at this window size. The scan bridged them into a single run whose
centre is the empty space between them, so every click landed on nothing and
the chests stayed put. The symptom looked exactly like a task that was not
working; the loop was right and the number was wrong. Both the merged and the
separated case are now unit tests, built from the real measurements.

**The region is wider than the two slots seen during calibration**, because the
row can hold more, and the per-pass cap is generous for the same reason.

Template matching was planned for this milestone and is not needed anywhere in
v1. The vision crate carries a channel comparison and a column scan, and nothing
else.

### The display is on loopback only

The first working version had the X server itself listen on TCP, because that is
the one transport Steam's sandbox can reach. That put the display on every
network interface: `-listen tcp` binds 0.0.0.0 and Xorg has no option to choose
an address. A magic cookie was the only thing in front of a live view of the
screen and an input channel into it.

The server now opens its unix socket only, and `scripts/xserver.sh` runs a socat
forwarder from 127.0.0.1 into that socket. The sandbox reaches the loopback
port; nothing else reaches anything. Both xauth entries carry the same cookie,
which works because MIT-MAGIC-COOKIE-1 authenticates the bytes rather than the
transport.

### One command, and the four bugs that took

`tbh run` now brings up the display, asks Steam for the game, waits out the
opening sequence, drives the UI to a known state, and takes commands on stdin.
`show` and `hide` open and close a mirror of the display. A person clicking in
that mirror pauses the bot, and it resumes once they stop.

Getting there turned up four failures worth writing down, because each looked
like something other than what it was.

**The window id goes stale.** The game destroys its first window during loading
and creates another, so an id captured at launch becomes invalid partway
through startup. Every request that fails with a bad window now rebinds by
title and retries once.

**The window moves after launch.** Unity restores a saved position from the Wine
registry a moment after the window appears, and that position has a negative y.
`GetImage` rejects any rectangle not wholly on screen, so a position check done
once at startup turns into a `BadMatch` on every capture from then on. It is now
checked before every capture.

**Keys went nowhere.** The isolated display has no window manager, so nothing
ever sets an input focus and the server stays on `PointerRoot`: key events go to
whatever window the pointer happens to be over. Pressing Tab worked right after
a click and silently did nothing otherwise, which made it look intermittent
rather than unfocused. The restore sequence now sets the focus explicitly.

**Tab twice is only right half the time.** Closing and reopening the menu does
normalise its state, but only when it was open to begin with. At startup it is
closed, so the first Tab opened it and the second closed it again, and every
click afterwards landed on nothing. Whether the menu is open is now read off its
red title banner, the same shape of check the synthesize button and the launch
dialog use.

That check has now paid for itself three times over, which is the argument for
it: a relative comparison between two channels of one pixel is cheap enough to
use wherever a yes-or-no question about the screen comes up.

### The observer cannot read the device id

`x11vnc` injects the viewer's clicks through `XTEST`, the same path the bot
uses, so the source device separates nothing. What separates them is that the
bot knows what it sent: every injected click is recorded, and an observed press
that matches a recent one in place and time is ours.

The ledger is reconciled immediately after each injected click rather than on
the worker tick. An earlier version reconciled only on the tick, with a 600ms
entry lifetime, and the restore sequence blocks for nearly three seconds. Its
own clicks aged out before the queue was drained, read as a person taking over,
and triggered another restore, forever.

The limitation that remains: a person clicking the exact pixel the bot just
clicked, inside the reconciliation window, is attributed to the bot.
