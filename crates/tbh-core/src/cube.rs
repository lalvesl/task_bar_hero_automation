//! The cube task.
//!
//! Auto-fill picks the items, so this never has to know what an item is or what
//! rarity it carries. The whole loop is three buttons and one colour check.

use std::thread::sleep;
use std::time::Duration;

use tbh_capture::Capture;
use tbh_input::{Button, Pointer};

use crate::TaskError;
use crate::config::CubeConfig;

/// How long to wait for a panel to open or a synthesis to play out.
///
/// The game animates, and a frame grabbed mid-animation shows a button in a
/// state it is about to leave. Generous rather than tight: the task runs on a
/// minutes-long interval, so a second here costs nothing.
const SETTLE: Duration = Duration::from_millis(1200);

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
    /// The task is switched off in the configuration.
    Disabled,
}

/// Run the cube until auto-fill stops filling the grid.
///
/// The panel is closed and reopened between syntheses, because the result of a
/// synthesis stays in the grid and the panel has to be cycled to clear it.
///
/// # Errors
/// Fails if the window cannot be captured or a click cannot be delivered.
pub fn run(
    config: &CubeConfig,
    stash_store_all: Option<tbh_input::NormalizedPoint>,
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
) -> Result<CubeRun, TaskError> {
    if !config.enabled {
        return Ok(CubeRun {
            synthesized: 0,
            outcome: CubeOutcome::Disabled,
        });
    }

    if let Some(store_all) = stash_store_all {
        click(capture, pointer, store_all)?;
        sleep(SETTLE);
    }

    let mut synthesized = 0;
    while synthesized < config.max_per_run {
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

        // The synthesised item is left sitting in the grid. Toggling the panel
        // is what clears it, and it is cheaper and more reliable than hunting
        // for the grid's own clear control.
        click(capture, pointer, config.menu_icon)?;
        sleep(SETTLE);
        click(capture, pointer, config.menu_icon)?;
        sleep(SETTLE);
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
    capture: &dyn Capture,
    pointer: &mut dyn Pointer,
    at: tbh_input::NormalizedPoint,
) -> Result<(), TaskError> {
    let rect = capture.window_rect()?;
    pointer.click(at, rect, Button::Left)?;
    Ok(())
}
