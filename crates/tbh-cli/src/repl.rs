//! The command loop on stdin.
//!
//! Plain lines rather than a full-screen interface. The operator types a verb
//! and a task name, and the process keeps running in between; there is no
//! screen to redraw and nothing to lay out.

use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;

use crate::control::{Control, Halt, Task, lock};
use crate::viewer::Viewer;

/// What `show` needs to know to put a mirror on screen.
pub struct Mirror {
    /// The display to mirror.
    pub display: String,
    /// Loopback port to serve on.
    pub port: u16,
    /// The isolated display cookie.
    pub auth: PathBuf,
}

/// What the operator can type.
const HELP: &str = "\
commands:
  enable <task>     let the task run on its interval
  disable <task>    stop the task running
  run <task>        run the task now, without waiting for the interval
  force <task>      the same, but also when the task is disabled, and
                    without waiting out a pause
  stop              hands off the game, without forgetting anything
  start             hands back on, putting the menus back first
  status            what is on, and what the last run did
  show              open a window onto the game
  hide              close it again
  help              this text
  quit              stop the bot

tasks:
  synthesis         equipment synthesis in the cube
                    also accepted: \"synthesis equipment\", \"cube\"
  chests            collecting the chests
                    also accepted: \"chest\", \"open chests\"";

/// Read commands until end of input or `quit`.
///
/// # Errors
/// Fails only if stdin itself breaks; an unrecognised command is reported to
/// the operator and the loop continues.
pub fn run(control: &Control, mirror: &Mirror) -> anyhow::Result<()> {
    let mut viewer = Viewer::default();

    println!("tbh: type \"help\" for commands");
    prompt();

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if !dispatch(control, &mut viewer, mirror, line.trim()) {
            break;
        }
        prompt();
    }

    lock(control).shutdown = true;
    Ok(())
}

/// Act on one line. Returns false when the loop should end.
fn dispatch(control: &Control, viewer: &mut Viewer, mirror: &Mirror, line: &str) -> bool {
    let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));

    match verb {
        "" => {}
        "help" | "?" => println!("{HELP}"),
        "status" => status(control),
        "stop" => {
            lock(control).stop();
            println!("stopped; type \"start\" to hand the game back");
        }
        "start" => {
            lock(control).start();
            println!("started; putting the menus back first");
        }
        "show" => match viewer.show(&mirror.display, mirror.port, &mirror.auth) {
            Ok(()) => println!("showing {}", mirror.display),
            Err(error) => println!("show failed: {error:#}"),
        },
        "hide" | "hidden" => {
            viewer.hide();
            println!("hidden");
        }
        "quit" | "exit" => {
            viewer.hide();
            lock(control).shutdown = true;
            println!("stopping");
            return false;
        }
        "enable" | "disable" => set_enabled(control, rest, verb == "enable"),
        "run" => run_now(control, rest, false),
        "force" => run_now(control, rest, true),
        _ => println!("unknown command {verb:?}; type \"help\""),
    }
    true
}

/// Turn a task on or off.
fn set_enabled(control: &Control, rest: &str, enabled: bool) {
    let Some(task) = Task::parse(rest) else {
        println!("unknown task {rest:?}; type \"help\"");
        return;
    };

    match task {
        Task::Synthesis => lock(control).synthesis.enabled = enabled,
        Task::Chests => lock(control).chests.enabled = enabled,
    }
    println!(
        "{} is now {}",
        task.name(),
        if enabled { "enabled" } else { "disabled" }
    );
}

/// Ask for a task to run without waiting for its interval.
///
/// `force` is the same request with the two things that would hold it back
/// taken out of the way: the task's own switch, and the stand-down window a
/// person's clicks put in place. Neither is changed permanently. A forced task
/// stays disabled afterwards, and the next click pauses the bot again.
fn run_now(control: &Control, rest: &str, force: bool) {
    let Some(task) = Task::parse(rest) else {
        println!("unknown task {rest:?}; type \"help\"");
        return;
    };

    // The guard is dropped before printing: holding a lock across I/O would
    // let a slow terminal stall the worker.
    let queued = {
        let mut state = lock(control);
        let enabled = match task {
            Task::Synthesis => state.synthesis.enabled,
            Task::Chests => state.chests.enabled,
        };
        let queued = enabled || force;
        if queued {
            match task {
                Task::Synthesis => state.synthesis.queued = true,
                Task::Chests => state.chests.queued = true,
            }
        }
        // A click stand-down is a guess that a person is working in the game,
        // and an explicit `force` outranks a guess. A typed `stop` is not a
        // guess, so that one stands and the request waits behind it.
        if force && matches!(state.halt, Halt::Clicked(_)) {
            state.halt = Halt::Running;
        }
        queued
    };

    if queued {
        println!("{}: queued", task.name());
        // Said here because a request that sits there silently looks like one
        // that was dropped.
        if lock(control).halt == Halt::Manual {
            println!("  the bot is stopped; it will run after \"start\"");
        }
    } else {
        println!(
            "{} is disabled; enable it first, or type \"force {}\"",
            task.name(),
            task.name()
        );
    }
}

/// Report the switches and the last result.
fn status(control: &Control) {
    let state = lock(control);
    match state.halt {
        Halt::Manual => println!("stopped: nothing runs until \"start\""),
        _ if state.halted() => println!("stopped: you are using the display"),
        _ => {}
    }
    report(
        "synthesis",
        state.synthesis.enabled,
        state.synthesis.since(),
        || {
            state.synthesis.result.map(|run| {
                format!(
                    "{} synthesised, stopped because {:?}",
                    run.synthesized, run.outcome
                )
            })
        },
    );
    report("chests", state.chests.enabled, state.chests.since(), || {
        state
            .chests
            .result
            .map(|pass| format!("{} found, {} clicked", pass.found, pass.clicked))
    });
}

/// Print one task's line, and the line about its last run.
fn report(
    name: &str,
    enabled: bool,
    since: Option<std::time::Duration>,
    describe: impl FnOnce() -> Option<String>,
) {
    println!("{name}: {}", if enabled { "enabled" } else { "disabled" });
    match (since, describe()) {
        (Some(since), Some(what)) => println!("  last run {}s ago: {what}", since.as_secs()),
        (Some(since), None) => println!("  last run {}s ago: failed", since.as_secs()),
        _ => println!("  has not run yet"),
    }
}

/// Print the prompt and push it out, since stdout is line buffered and the
/// prompt carries no newline.
fn prompt() {
    print!("> ");
    let _ = std::io::stdout().flush();
}
