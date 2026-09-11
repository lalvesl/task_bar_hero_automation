//! The `tbh` binary.
//!
//! `tbh run` is the whole thing: it brings up the isolated display, asks Steam
//! to launch the game onto it, drives the enabled tasks, and takes plain
//! commands on stdin. The other subcommands exist for calibration, which is
//! done by hand.

mod act;
mod calibrate;
mod control;
mod recording;
mod repl;
mod supervisor;
mod viewer;
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

    /// Omitted, this is `run`: the normal thing to do with this binary is start
    /// it and leave it up.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Bring up the display and the game, then take commands on stdin.
    Run(RunArgs),

    /// Capture the game window and report its geometry.
    ///
    /// This is where the UI map comes from: run it, then read coordinates off
    /// the PNG it writes.
    Calibrate(calibrate::Args),

    /// Click one normalized point in the game window.
    Click(act::ClickArgs),

    /// Press one key.
    Key(act::KeyArgs),

    /// Print the launch option to paste into Steam.
    Setup(SetupArgs),
}

/// Arguments for `tbh setup`.
#[derive(Debug, clap::Args)]
struct SetupArgs {
    /// Where to read the Steam app id from.
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,
}

/// Arguments for `tbh run`.
#[derive(Debug, clap::Args)]
struct RunArgs {
    /// Where to read tunables from.
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,

    /// Use a display and game that are already running.
    #[arg(long)]
    no_launch: bool,
}

impl Default for RunArgs {
    fn default() -> Self {
        Self {
            config: PathBuf::from("config.toml"),
            no_launch: false,
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.as_ref() {
        None => run(&RunArgs::default()),
        Some(Command::Run(args)) => run(args),
        Some(Command::Calibrate(args)) => calibrate::run(&cli.target, args),
        Some(Command::Click(args)) => act::click(&cli.target, args),
        Some(Command::Key(args)) => act::key(&cli.target, args),
        Some(Command::Setup(args)) => setup(args),
    }
}

/// Bring up the display and the game, start the worker, and hand the terminal
/// to the command loop.
///
/// One command rather than three: the operator should not have to remember a
/// ritual before typing `enable chests`.
fn run(args: &RunArgs) -> anyhow::Result<()> {
    let config =
        Config::load(&args.config).with_context(|| format!("loading {}", args.config.display()))?;

    if !args.no_launch {
        supervisor::start_display(&supervisor::scripts_dir())?;
        supervisor::start_game(config.steam_app_id, &config.display, &config.window)?;
    }

    let mirror = repl::Mirror {
        display: config.display.clone(),
        port: config.vnc_port,
        auth: auth_path(),
    };

    let control = Arc::new(Mutex::new(control::State::new(
        config.cube.enabled,
        config.chest.enabled,
    )));

    // The worker owns the X connections, so the configuration moves into it
    // rather than being borrowed across the thread boundary.
    let worker_control = Arc::clone(&control);
    let worker = std::thread::spawn(move || worker::run(&config, &worker_control));

    repl::run(&control, &mirror)?;

    // The repl has already set shutdown, so this is a join, not a wait on
    // something that might never finish.
    let _ = worker.join();
    Ok(())
}

/// Where `scripts/xserver.sh` leaves the display cookie.
///
/// Derived rather than configured: the script and the binary have to agree, and
/// a path in two places is a path that will disagree with itself.
fn auth_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_owned());
    PathBuf::from(runtime).join("tbh-automation/Xauthority")
}

/// Print the launch option to paste into Steam.
fn setup(args: &SetupArgs) -> anyhow::Result<()> {
    let config =
        Config::load(&args.config).with_context(|| format!("loading {}", args.config.display()))?;
    supervisor::print_setup(&supervisor::scripts_dir(), config.steam_app_id)
}
