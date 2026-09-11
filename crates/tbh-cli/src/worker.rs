//! The thread that actually drives the game.
//!
//! It polls the shared state on a short timer rather than waiting on a signal,
//! because it has to wake on a wall-clock interval regardless. The X
//! connections are opened here, in the thread that uses them, so nothing has to
//! be shared across threads but the switches.

use std::thread::sleep;
use std::time::{Duration, Instant};

use tbh_capture::x11::X11Capture;
use tbh_core::config::Config;
use tbh_core::chest;
use tbh_core::cube;
use tbh_input::x11::X11Input;

use crate::control::{Control, lock};

/// How often the worker wakes to check the switches and the clock.
///
/// Short enough that `run synthesis` feels immediate, long enough that an idle
/// bot costs nothing on a machine whose CPU clock is held down on purpose.
const TICK: Duration = Duration::from_millis(250);

/// Drive the enabled tasks until `quit`.
///
/// Errors from a single run are reported and swallowed: a task that fails
/// because a panel was in an unexpected state should not take the process down,
/// since the next interval will find the game in a different state.
pub fn run(config: &Config, control: &Control) {
    let mut capture = match connect(config) {
        Ok(capture) => capture,
        Err(error) => {
            eprintln!("worker: {error}");
            return;
        }
    };

    let mut pointer = match X11Input::connect(&config.display) {
        Ok(pointer) => pointer,
        Err(error) => {
            eprintln!("worker: opening XTEST on {}: {error}", config.display);
            return;
        }
    };

    let cube_interval = Duration::from_secs(u64::from(config.cube.interval_secs));
    let chest_interval = Duration::from_secs_f32(config.chest.interval_secs.max(0.1));

    loop {
        sleep(TICK);

        let (shutdown, run_cube, run_chests) = {
            let state = lock(control);
            let cube_due = state.synthesis_now
                || state
                    .synthesis_last
                    .is_none_or(|last| last.elapsed() >= cube_interval);
            let chests_due = state
                .chests_last
                .is_none_or(|last| last.elapsed() >= chest_interval);
            (
                state.shutdown,
                state.synthesis_enabled && cube_due,
                state.chests_enabled && chests_due,
            )
        };

        if shutdown {
            return;
        }

        if run_chests {
            sweep_chests(config, control, &mut capture, &mut pointer);
        }

        if !run_cube {
            continue;
        }

        let store_all = config
            .cube
            .store_all_first
            .then_some(config.stash.store_all);

        match cube::run(&config.cube, store_all, &mut capture, &mut pointer) {
            Ok(result) => {
                println!(
                    "synthesis: {} synthesised, stopped because {:?}",
                    result.synthesized, result.outcome
                );
                let mut state = lock(control);
                state.synthesis_result = Some(result);
                state.synthesis_last = Some(Instant::now());
                state.synthesis_now = false;
            }
            Err(error) => {
                eprintln!("synthesis: {error}");
                let mut state = lock(control);
                state.synthesis_last = Some(Instant::now());
                state.synthesis_now = false;
            }
        }
    }
}

/// Connect to the display and bind to the game window.
fn connect(config: &Config) -> anyhow::Result<X11Capture> {
    let capture = X11Capture::connect(&config.display, &config.window).map_err(|error| {
        anyhow::anyhow!(
            "connecting to {} for window {:?}: {error}",
            config.display,
            config.window
        )
    })?;
    capture
        .ensure_onscreen()
        .map_err(|error| anyhow::anyhow!("moving the window fully on-screen: {error}"))?;
    Ok(capture)
}

/// One chest pass, with its result folded into the shared state.
///
/// Only a pass that actually clicked something is announced. The task runs
/// every couple of seconds and finds nothing most of the time, so printing
/// every empty pass would bury everything else the operator typed.
fn sweep_chests(
    config: &Config,
    control: &Control,
    capture: &mut X11Capture,
    pointer: &mut X11Input,
) {
    match chest::run(&config.chest, capture, pointer) {
        Ok(pass) => {
            if pass.clicked > 0 {
                println!("chests: {} found, {} clicked", pass.found, pass.clicked);
            }
            let mut state = lock(control);
            state.chests_result = Some(pass);
            state.chests_last = Some(Instant::now());
        }
        Err(error) => {
            eprintln!("chests: {error}");
            lock(control).chests_last = Some(Instant::now());
        }
    }
}
