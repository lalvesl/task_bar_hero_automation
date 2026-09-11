//! The `calibrate` subcommand.
//!
//! No templates ship with this project, and the ones bundled by the Windows
//! tools cannot be trusted under Proton, where font rendering and scaling may
//! differ. So the first runnable artifact is the thing that produces a
//! screenshot of this machine's own game window.

use std::path::PathBuf;

use anyhow::Context as _;
use tbh_capture::{Capture as _, x11::X11Capture};

/// Arguments for `tbh calibrate`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The X display the game was launched onto.
    #[arg(long, default_value = ":9")]
    display: String,

    /// Substring of the game window's title.
    #[arg(long, default_value = "TaskBarHero")]
    window: String,

    /// Where to write the PNG.
    #[arg(long, default_value = "artifacts/calibrate.png")]
    out: PathBuf,
}

/// Capture one frame and write it out, reporting what was found.
///
/// # Errors
/// Fails if the display is unreachable, no window matches, or the PNG cannot be
/// written.
pub fn run(args: &Args) -> anyhow::Result<()> {
    let mut capture = X11Capture::connect(&args.display, &args.window)
        .with_context(|| format!("connecting to {} for window {:?}", args.display, args.window))?;

    capture
        .ensure_onscreen()
        .context("moving the window fully on-screen")?;

    let name = capture.window_name().unwrap_or_default();
    let rect = capture.window_rect().context("reading window geometry")?;
    let frame = capture.grab().context("capturing the window")?;

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    frame
        .to_rgba()
        .save(&args.out)
        .with_context(|| format!("writing {}", args.out.display()))?;

    println!("window  0x{:x} {name:?}", capture.window_id());
    println!(
        "geometry {}x{} at ({}, {})",
        rect.width, rect.height, rect.x, rect.y
    );
    println!("wrote   {}", args.out.display());
    Ok(())
}
