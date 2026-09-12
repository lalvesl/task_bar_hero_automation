//! Bringing up everything the bot needs, so `tbh run` is the only command.
//!
//! The display and the game are started here rather than by hand: an operator
//! should not have to remember a three-step ritual before typing `enable
//! chests`.
//!
//! ## Why this shells out to xserver.sh
//!
//! The server, its auth cookie and the loopback forwarder are already
//! implemented there, and that implementation is the one that has been tested
//! against Steam's sandbox. Reimplementing it in Rust would mean two copies of
//! a fiddly sequence, and the second copy is the one that drifts. The script
//! also has to stay usable on its own, because the display has to exist before
//! there is any configuration to run the binary against.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use tbh_capture::x11::X11Capture;

/// How long to wait for the game's window to appear after asking Steam for it.
///
/// Proton has a container and a Wine prefix to bring up first, so this is
/// generous. A wrong answer here reads as "the game never started", which would
/// send someone looking in the wrong place.
const WINDOW_TIMEOUT: Duration = Duration::from_secs(90);

/// Start the X server if it is not already up.
///
/// # Errors
/// Fails if the script is missing or reports failure.
pub fn start_display(scripts: &Path) -> anyhow::Result<()> {
    let script = scripts.join("xserver.sh");
    let status = Command::new(&script)
        .arg("start")
        .status()
        .with_context(|| format!("running {}", script.display()))?;

    anyhow::ensure!(status.success(), "{} start failed", script.display());
    Ok(())
}

/// Ask Steam to launch the game, then wait for its window to appear.
///
/// Steam is asked rather than the executable being run directly, because the
/// launch options that point the game at the isolated display live in Steam's
/// own configuration.
///
/// # Errors
/// Fails if Steam cannot be run, or if no window appears in time.
pub fn start_game(app_id: u32, display: &str, window: &str) -> anyhow::Result<()> {
    if find_window(display, window).is_ok() {
        println!("game: already running");
        return Ok(());
    }

    println!("game: asking Steam to launch {app_id}");
    println!("game: if it opens on your desktop instead, run `tbh setup`");

    // Printed every launch, not only on failure. If the launch option is
    // missing the game comes up on the desktop instead, and the only symptom
    // is a wait that ends in a timeout ninety seconds later. Showing the line
    // up front costs one line and saves that.
    if let Ok(option) = launch_option(&scripts_dir()) {
        println!("game: its Steam Launch Options should read:");
        println!("      {option}");
    }

    Command::new("steam")
        .arg("-applaunch")
        .arg(app_id.to_string())
        .spawn()
        .context("running steam; is it on PATH?")?;

    let deadline = Instant::now() + WINDOW_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(mut capture) = find_window(display, window) {
            capture
                .ensure_onscreen()
                .context("moving the window fully on-screen")?;
            println!("game: window up on {display}");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    // The overwhelmingly likely cause is that Steam has no launch option for
    // this game, so it started on the desktop instead of the isolated display.
    // Saying so beats making someone guess.
    let hint = launch_option(&scripts_dir())
        .unwrap_or_else(|_| "scripts/launch_isolated.sh %command% -screen-fullscreen 0".to_owned());

    anyhow::bail!(
        "no window matching {window:?} appeared on {display} within {}s.\n\
         Check the game's Launch Options in Steam. They should read:\n\n  {hint}",
        WINDOW_TIMEOUT.as_secs()
    )
}

/// Where the helper scripts live, relative to the working directory.
#[must_use]
pub fn scripts_dir() -> PathBuf {
    PathBuf::from("scripts")
}

/// Bind to the game window, if it is there.
fn find_window(display: &str, window: &str) -> anyhow::Result<X11Capture> {
    X11Capture::connect(display, window).map_err(Into::into)
}

/// The line to paste into Steam's launch options for the game.
///
/// Built from the working directory at run time rather than written down
/// anywhere. A path in a document is a path that goes stale the first time the
/// checkout moves, and this one has to be exact for Steam to find the script.
///
/// # Errors
/// Fails if the working directory cannot be read or the script is missing.
pub fn launch_option(scripts: &Path) -> anyhow::Result<String> {
    let script = scripts.join("launch_isolated.sh");
    let absolute = script
        .canonicalize()
        .with_context(|| format!("resolving {}", script.display()))?;

    anyhow::ensure!(absolute.is_file(), "{} is not a file", absolute.display());

    Ok(format!(
        "{} %command% -screen-fullscreen 0",
        absolute.display()
    ))
}

/// Print the launch option along with where to put it.
///
/// # Errors
/// Fails if the script cannot be resolved.
pub fn print_setup(scripts: &Path, app_id: u32) -> anyhow::Result<()> {
    println!("In Steam, right-click TBH: Task Bar Hero (app id {app_id}),");
    println!("choose Properties, and paste this into Launch Options:");
    println!();
    println!("  {}", launch_option(scripts)?);
    println!();
    println!("This is per game. No other title sees it.");
    Ok(())
}
