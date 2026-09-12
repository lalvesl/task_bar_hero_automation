//! The thread that actually drives the game.
//!
//! It polls the shared state on a short timer rather than waiting on a signal,
//! because it has to wake on a wall-clock interval regardless. The X
//! connections are opened here, in the thread that uses them, so nothing has to
//! be shared across threads but the switches.

use std::thread::sleep;
use std::time::Duration;

use tbh_capture::x11::X11Capture;
use tbh_core::config::Config;
use tbh_core::{chest, cube, restore};
use tbh_input::watch::X11Watch;
use tbh_input::x11::X11Input;

use crate::control::{Control, Halt, lock};
use crate::recording::Recording;

/// How often the worker wakes to check the switches and the clock.
///
/// Short enough that `run synthesis` feels immediate and that a person's click
/// stops the bot within a moment, long enough that an idle bot costs nothing on
/// a machine whose CPU clock is held down on purpose.
const TICK: Duration = Duration::from_millis(250);

/// The X connections and the observer, held together for the run's lifetime.
struct Session {
    capture: X11Capture,
    pointer: X11Input,
    watch: X11Watch,
}

/// Drive the enabled tasks until `quit`.
///
/// Errors from a single run are reported and swallowed: a task that fails
/// because a panel was in an unexpected state should not take the process down,
/// since the next interval will find the game in a different state.
pub fn run(config: &Config, control: &Control) {
    let Some(mut session) = open(config) else {
        return;
    };

    let cube_interval = Duration::from_secs(u64::from(config.cube.interval_secs));
    let chest_interval = Duration::from_secs_f32(config.chest.interval_secs.max(0.1));
    let pause = Duration::from_secs_f32(config.pause_after_click_secs.max(0.0));

    // The window appears while the game is still playing its opening sequence,
    // so acting on it immediately means clicking on the intro. Waiting is
    // cruder than watching for the intro to end, and it does not depend on
    // recognising a screen that only shows up once per launch.
    let settle = Duration::from_secs_f32(config.startup_delay_secs.max(0.0));
    if !settle.is_zero() {
        println!(
            "waiting {}s for the game to finish loading",
            settle.as_secs()
        );
        let deadline = std::time::Instant::now() + settle;
        while std::time::Instant::now() < deadline {
            if lock(control).shutdown {
                return;
            }
            sleep(TICK);
        }
    }

    // The UI is driven to a known state before the first task runs, which also
    // clears the dialog the game opens on launch.
    lock(control).needs_restore = true;

    loop {
        sleep(TICK);

        if lock(control).shutdown {
            return;
        }

        // Polled before anything else, so a click that lands mid-tick still
        // stops the next action rather than the one after it.
        if let Err(error) = session.watch.poll() {
            eprintln!("watch: {error}");
        }

        // The queue is drained whether or not the bot may act on it. Events
        // left to pile up would mean a long stop ends in a backlog of clicks
        // to work through, held by the X server in the meantime.
        let human = session.watch.take_human();
        match gate(control, human, pause) {
            Gate::Go => {}
            Gate::Wait => continue,
            Gate::JustStopped => {
                println!("stopped: you are using the display");
                continue;
            }
            Gate::JustStarted => println!("started: the display has gone quiet"),
        }

        if lock(control).needs_restore {
            restore_ui(config, &mut session, control);
            continue;
        }

        // A queued request stands on its own, without the task's switch: it is
        // set either by `run`, which already checked the switch, or by
        // `force`, which is the operator deliberately overriding it.
        let (run_cube, run_chests) = {
            let state = lock(control);
            (
                state.synthesis.wanted(cube_interval),
                state.chests.wanted(chest_interval),
            )
        };

        // Synthesis first, and it runs to completion before returning. Order
        // matters here: a cube run empties the stash into the grid over many
        // seconds, and chests collected partway through would land in an
        // inventory the run has already read past. Finishing the synthesis and
        // then sweeping keeps each pass working on a settled inventory.
        if run_cube {
            synthesize(config, control, &mut session);
        }
        if run_chests {
            sweep_chests(config, control, &mut session);
        }
    }
}

/// What a tick may do about the stand-down, and what it should say.
enum Gate {
    /// Act.
    Go,
    /// Stand down, silently, because it has already been said.
    Wait,
    /// Stand down, and say so for the first time.
    JustStopped,
    /// Act, and say the stand-down a click caused has run out.
    JustStarted,
}

/// Decide whether this tick may touch the game.
///
/// A person's click and a typed `stop` are the same stand-down with a
/// different end to it. The click one carries a timer and lifts itself; the
/// typed one is checked first here and nothing in this function can lift it.
/// Every click pushes the timer out again, so someone working continuously
/// keeps the bot down without typing anything.
fn gate(control: &Control, human: bool, pause: Duration) -> Gate {
    let mut state = lock(control);

    if state.halt == Halt::Manual {
        return Gate::Wait;
    }
    if human {
        let already = state.halted();
        state.clicked(pause);
        return if already { Gate::Wait } else { Gate::JustStopped };
    }
    if state.expire() {
        return Gate::JustStarted;
    }
    if state.halted() { Gate::Wait } else { Gate::Go }
}

/// Open the display connections and bind to the game window.
fn open(config: &Config) -> Option<Session> {
    let mut capture = match X11Capture::connect(&config.display, &config.window) {
        Ok(capture) => capture,
        Err(error) => {
            eprintln!(
                "worker: connecting to {} for window {:?}: {error}",
                config.display, config.window
            );
            return None;
        }
    };
    if let Err(error) = capture.ensure_onscreen() {
        eprintln!("worker: moving the window fully on-screen: {error}");
        return None;
    }

    let pointer = match X11Input::connect(&config.display) {
        Ok(pointer) => pointer,
        Err(error) => {
            eprintln!("worker: opening XTEST on {}: {error}", config.display);
            return None;
        }
    };

    let watch = match X11Watch::connect(&config.display) {
        Ok(watch) => watch,
        Err(error) => {
            eprintln!("worker: observing {}: {error}", config.display);
            return None;
        }
    };

    Some(Session {
        capture,
        pointer,
        watch,
    })
}

/// Drive the UI back to a known state after a person has been in it.
///
/// A sequence that gave way partway through leaves `needs_restore` set, so it
/// is attempted again once the display is quiet. Clearing the flag on an
/// interrupted run would leave the panels half open with nothing to notice.
fn restore_ui(config: &Config, session: &mut Session, control: &Control) {
    println!("resuming: putting the menus back");

    // Split borrow: the capture is used on its own here, so it can be lent out
    // while the pointer and the watcher are borrowed together as the actor.
    let capture = &mut session.capture;
    let mut actor = Recording {
        pointer: &mut session.pointer,
        watch: &mut session.watch,
    };

    match restore::run(config, capture, &mut actor) {
        Ok(restore::Restored::Done) => lock(control).needs_restore = false,
        Ok(restore::Restored::Interrupted) => {
            println!("resuming: you are still using the display, standing down");
        }
        Err(error) => {
            eprintln!("restore: {error}");
            lock(control).needs_restore = false;
        }
    }
}

/// One cube run, with its result folded into the shared state.
fn synthesize(config: &Config, control: &Control, session: &mut Session) {
    let store_all = config
        .cube
        .store_all_first
        .then_some(config.stash.store_all);

    let capture = &mut session.capture;
    let mut actor = Recording {
        pointer: &mut session.pointer,
        watch: &mut session.watch,
    };

    match cube::run(&config.cube, store_all, capture, &mut actor) {
        Ok(result) => {
            println!(
                "synthesis: {} synthesised, stopped because {:?}",
                result.synthesized, result.outcome
            );
            if result.outcome == tbh_core::cube::CubeOutcome::WrongMode {
                eprintln!(
                    "synthesis: set the cube mode selector back to Síntese; \
                     nothing this task does means anything in the other modes"
                );
            }
            lock(control).synthesis.finished(Some(result));
        }
        Err(error) => {
            eprintln!("synthesis: {error}");
            lock(control).synthesis.finished(None);
        }
    }
}

/// One chest pass, with its result folded into the shared state.
///
/// Only a pass that actually clicked something is announced. The task runs
/// every couple of seconds and finds nothing most of the time, so printing
/// every empty pass would bury everything else the operator typed.
fn sweep_chests(config: &Config, control: &Control, session: &mut Session) {
    // A pass the operator asked for is always reported, even an empty one.
    // Silence is the right answer for a pass nobody asked about, and the wrong
    // one for a typed command.
    let asked_for = lock(control).chests.queued;

    let capture = &mut session.capture;
    let mut actor = Recording {
        pointer: &mut session.pointer,
        watch: &mut session.watch,
    };

    match chest::run(&config.chest, capture, &mut actor) {
        Ok(pass) => {
            if pass.clicked > 0 || asked_for {
                println!("chests: {} found, {} clicked", pass.found, pass.clicked);
            }

            let mut state = lock(control);
            state.chests.finished(Some(pass));

            // Collecting is what fills the inventory slots, so collecting is
            // what asks for them to be emptied. The synthesis has to be
            // enabled for this: it queues a run that is not due, it does not
            // switch a task on that the operator turned off.
            if pass.clicked > 0 && config.cube.after_chest && state.synthesis.enabled {
                state.synthesis.queued = true;
            }
        }
        Err(error) => {
            eprintln!("chests: {error}");
            lock(control).chests.finished(None);
        }
    }
}
