//! The cube task.
//!
//! Auto-fill picks the items, so this never has to know what an item is or what
//! rarity it carries. The whole loop is three buttons and one colour check.

use std::thread::sleep;
use std::time::Duration;

use tbh_capture::Capture;
use tbh_input::{Button, Pointer};

use crate::config::CubeConfig;
use crate::{Task, TaskError};

/// How long to wait for a panel to open or a synthesis to play out.
///
/// The game animates, and a frame grabbed mid-animation shows a button in a
/// state it is about to leave. Generous rather than tight: the task runs on a
/// minutes-long interval, so a second here costs nothing.
const SETTLE: Duration = Duration::from_millis(1200);

/// How many times to click the cube icon before giving up on a panel state.
///
/// More than one because a click is sometimes swallowed, and small because if
/// several in a row do nothing the problem is not timing.
const PANEL_ATTEMPTS: u8 = 4;

/// What one run of the task did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CubeRun {
    /// How many syntheses were performed.
    pub synthesized: u16,
    /// Why the run ended.
    pub outcome: CubeOutcome,
}

/// Why a run stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeOutcome {
    /// Auto-fill could not fill the grid, so there is nothing left to combine.
    Exhausted,
    /// The per-run cap was reached, which means something is wrong: the cap
    /// exists so a misread colour cannot become an unbounded loop.
    CapReached,
    /// The cube is set to some mode other than synthesis, so none of the
    /// buttons this task presses mean what it expects.
    WrongMode,
    /// Someone is using the display, so the run stood down partway through.
    Interrupted,
}

/// Run the cube until auto-fill stops filling the grid.
///
/// Whether the task may run at all is the caller's decision, not this
/// function's. The configuration's `enabled` flag only seeds the process's
/// initial state, and an earlier version re-read it here: the operator would
/// type `enable synthesis`, the worker would decide a run was due, and this
/// would return on its first line saying it was switched off. Nothing happened
/// and nothing said why.
///
/// The panel is closed and reopened before every synthesis, because whatever
/// the grid already holds stays there and cycling the panel is what clears it.
/// Before rather than after: cycling only after a synthesis left the first item
/// of every run working on a grid nobody had cleared.
///
/// # Errors
/// Fails if the window cannot be captured or a click cannot be delivered.
pub fn run(
    config: &CubeConfig,
    stash_store_all: Option<tbh_input::NormalizedPoint>,
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
) -> Result<CubeRun, TaskError> {
    if let Some(store_all) = stash_store_all {
        click(capture, pointer, store_all)?;
        sleep(SETTLE);
    }

    let mut synthesized = 0;
    while synthesized < config.max_per_run {
        // A run can last minutes. Checked between items so someone who starts
        // using the display does not have to wait it out.
        if pointer.interrupted()? {
            return Ok(CubeRun {
                synthesized,
                outcome: CubeOutcome::Interrupted,
            });
        }

        // Cycled every pass, including the first. A synthesised item is left
        // sitting in the grid, and so is anything left there between runs;
        // closing and reopening the panel is what clears it.
        set_panel(config, capture, pointer, false)?;
        set_panel(config, capture, pointer, true)?;

        // Checked every pass, not once: the mode selector is one click away
        // from the buttons this task uses, and a person poking at the cube can
        // leave it somewhere else between runs.
        let frame = capture.grab()?;
        if !config.mode_is_synthesis.holds(&frame)? {
            return Ok(CubeRun {
                synthesized,
                outcome: CubeOutcome::WrongMode,
            });
        }

        click(capture, pointer, config.auto_fill)?;
        sleep(SETTLE);

        let frame = capture.grab()?;
        if !config.synthesize_enabled.holds(&frame)? {
            return Ok(CubeRun {
                synthesized,
                outcome: CubeOutcome::Exhausted,
            });
        }

        click(capture, pointer, config.synthesize)?;
        sleep(SETTLE);
        synthesized += 1;
    }

    Ok(CubeRun {
        synthesized,
        outcome: CubeOutcome::CapReached,
    })
}

/// Resolve a point against the window as it is right now, and click it.
///
/// The rectangle is read per click rather than cached, so the task keeps
/// working if the window is moved mid-run.
fn click(
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
    at: tbh_input::NormalizedPoint,
) -> Result<(), TaskError> {
    let rect = capture.window_rect()?;
    pointer.click(at, rect, Button::Left)?;
    Ok(())
}

/// Click the cube icon until the panel is in the state asked for.
///
/// The state is read rather than assumed. An earlier version clicked the icon
/// twice with a fixed wait between, on the reasoning that two toggles of a
/// toggle land you back where you started with the grid cleared. The second
/// click did not always take, so the panel stayed shut, every auto-fill after
/// that went nowhere, and the run ended reporting an exhausted inventory. The
/// symptom was indistinguishable from having genuinely run out.
fn set_panel(
    config: &CubeConfig,
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
    want_open: bool,
) -> Result<(), TaskError> {
    for _ in 0..PANEL_ATTEMPTS {
        let frame = capture.grab()?;
        if config.panel_visible.holds(&frame)? == want_open {
            return Ok(());
        }
        click(capture, pointer, config.menu_icon)?;
        sleep(SETTLE);
    }

    Err(TaskError::Timeout {
        task: Task::Cube,
        state: if want_open {
            "opening the cube panel"
        } else {
            "closing the cube panel"
        },
    })
}
