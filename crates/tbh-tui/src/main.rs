//! The `tbh` binary.
//!
//! Ratatui comes in at M8. Until the tasks run headless against a log there is
//! nothing worth drawing, and a UI written first would be a UI written against
//! guesses. What exists now is calibration, which is what the templates and
//! click points have to be cut from.

mod calibrate;

use clap::{Parser, Subcommand};

/// Background farming automation for TBH: Task Bar Hero.
#[derive(Debug, Parser)]
#[command(name = "tbh", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Capture the game window and report its geometry.
    ///
    /// This is where templates come from: run it, then cut the chest sprites
    /// and the cube's buttons out of the PNG it writes.
    Calibrate(calibrate::Args),
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Calibrate(args) => calibrate::run(&args),
    }
}
