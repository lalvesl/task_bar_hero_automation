//! The `tbh` binary.
//!
//! `tbh run` is the normal way to use this: a process that stays up, drives the
//! enabled tasks on their intervals, and takes plain commands on stdin. The
//! other subcommands exist for calibration, which is done by hand.

mod act;
mod calibrate;
mod control;
mod repl;
mod worker;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tbh_core::config::Config;

/// Background farming automation for TBH: Task Bar Hero.
#[derive(Debug, Parser)]
#[command(name = "tbh", version)]
struct Cli {
    #[command(flatten)]
    target: act::Target,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Keep running, driving the enabled tasks and taking commands on stdin.
    Run(RunArgs),

    /// Capture the game window and report its geometry.
    ///
    /// This is where templates come from: run it, then cut the chest sprites
    /// out of the PNG it writes.
    Calibrate(calibrate::Args),

    /// Click one normalized point in the game window.
    Click(act::ClickArgs),

    /// Press one key.
    Key(act::KeyArgs),
}

/// Arguments for `tbh run`.
#[derive(Debug, clap::Args)]
struct RunArgs {
    /// Where to read tunables from.
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Run(args) => run(args),
        Command::Calibrate(args) => calibrate::run(&cli.target, args),
        Command::Click(args) => act::click(&cli.target, args),
        Command::Key(args) => act::key(&cli.target, args),
    }
}

/// Start the worker and hand the terminal to the command loop.
fn run(args: &RunArgs) -> anyhow::Result<()> {
    let config =
        Config::load(&args.config).with_context(|| format!("loading {}", args.config.display()))?;

    let control = Arc::new(Mutex::new(control::State::new(
        config.cube.enabled,
        config.chest.enabled,
    )));

    // The worker owns the X connections, so the configuration moves into it
    // rather than being borrowed across the thread boundary.
    let worker_control = Arc::clone(&control);
    let worker = std::thread::spawn(move || worker::run(&config, &worker_control));

    repl::run(&control)?;

    // The repl has already set shutdown, so this is a join, not a wait on
    // something that might never finish.
    let _ = worker.join();
    Ok(())
}
