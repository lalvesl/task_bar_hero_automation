//! The command loop on stdin.
//!
//! Plain lines rather than a full-screen interface. The operator types a verb
//! and a task name, and the process keeps running in between; there is no
//! screen to redraw and nothing to lay out.

use std::io::{BufRead as _, Write as _};

use crate::control::{Control, Task, lock};

/// What the operator can type.
const HELP: &str = "\
commands:
  enable <task>     let the task run on its interval
  disable <task>    stop the task running
  run <task>        run the task now, without waiting for the interval
  status            what is on, and what the last run did
  help              this text
  quit              stop the bot

tasks:
  synthesis         equipment synthesis in the cube
                    also accepted: \"synthesis equipment\", \"cube\"";

/// Read commands until end of input or `quit`.
///
/// # Errors
/// Fails only if stdin itself breaks; an unrecognised command is reported to
/// the operator and the loop continues.
pub fn run(control: &Control) -> anyhow::Result<()> {
    println!("tbh: type \"help\" for commands");
    prompt();

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if !dispatch(control, line.trim()) {
            break;
        }
        prompt();
    }

    lock(control).shutdown = true;
    Ok(())
}

/// Act on one line. Returns false when the loop should end.
fn dispatch(control: &Control, line: &str) -> bool {
    let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));

    match verb {
        "" => {}
        "help" | "?" => println!("{HELP}"),
        "status" => status(control),
        "quit" | "exit" => {
            lock(control).shutdown = true;
            println!("stopping");
            return false;
        }
        "enable" | "disable" => set_enabled(control, rest, verb == "enable"),
        "run" => run_now(control, rest),
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
        Task::Synthesis => lock(control).synthesis_enabled = enabled,
        Task::Chests => lock(control).chests_enabled = enabled,
    }
    println!(
        "{} is now {}",
        task.name(),
        if enabled { "enabled" } else { "disabled" }
    );
}

/// Ask for a task to run without waiting for its interval.
fn run_now(control: &Control, rest: &str) {
    let Some(task) = Task::parse(rest) else {
        println!("unknown task {rest:?}; type \"help\"");
        return;
    };

    match task {
        Task::Synthesis => {
            // The guard is dropped before printing: holding a lock across I/O
            // would let a slow terminal stall the worker.
            let queued = {
                let mut state = lock(control);
                let enabled = state.synthesis_enabled;
                state.synthesis_now |= enabled;
                enabled
            };
            if queued {
                println!("synthesis: queued");
            } else {
                println!("synthesis is disabled; enable it first");
            }
        }
        // The chest task runs every couple of seconds on its own, so there is
        // nothing a manual trigger would bring forward.
        Task::Chests => println!("chests runs continuously; enable it instead"),
    }
}

/// Report the switches and the last result.
fn status(control: &Control) {
    let state = lock(control);
    println!(
        "synthesis: {}",
        if state.synthesis_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    match (state.synthesis_last, state.synthesis_result) {
        (Some(last), Some(result)) => println!(
            "  last run {}s ago: {} synthesised, stopped because {:?}",
            last.elapsed().as_secs(),
            result.synthesized,
            result.outcome
        ),
        (Some(last), None) => println!("  last run {}s ago: failed", last.elapsed().as_secs()),
        _ => println!("  has not run yet"),
    }

    println!(
        "chests: {}",
        if state.chests_enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    match (state.chests_last, state.chests_result) {
        (Some(last), Some(pass)) => println!(
            "  last pass {}s ago: {} found, {} clicked",
            last.elapsed().as_secs(),
            pass.found,
            pass.clicked
        ),
        (Some(last), None) => println!("  last pass {}s ago: failed", last.elapsed().as_secs()),
        _ => println!("  has not run yet"),
    }
}

/// Print the prompt and push it out, since stdout is line buffered and the
/// prompt carries no newline.
fn prompt() {
    print!("> ");
    let _ = std::io::stdout().flush();
}
