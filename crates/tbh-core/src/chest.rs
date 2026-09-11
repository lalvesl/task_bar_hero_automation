//! The chest task.
//!
//! One pass is: grab a frame, find every chest in the band they drop into,
//! click each one. There is nothing to remember between passes, because a
//! clicked chest is gone by the next one.

use tbh_capture::Capture;
use tbh_input::{Button, NormalizedPoint, Pointer};

use crate::TaskError;
use crate::config::ChestConfig;

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChestPass {
    /// How many chests were seen.
    pub found: usize,
    /// How many were clicked. Lower than `found` when the per-pass cap bit.
    pub clicked: usize,
}

/// Find and click every chest currently on screen.
///
/// # Errors
/// Fails if the window cannot be captured or a click cannot be delivered.
pub fn run(
    config: &ChestConfig,
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
) -> Result<ChestPass, TaskError> {
    let frame = capture.grab()?;
    let chests = config.scan.find(&frame)?;

    // The cap is not an optimisation. If the scan ever misreads the band as
    // full of content, this is what stops a pass turning into a click storm.
    let limit = usize::from(config.max_clicks_per_pass);
    let clicked = chests.len().min(limit);

    let rect = capture.window_rect()?;
    for chest in chests.iter().take(limit) {
        pointer.click(
            NormalizedPoint {
                x: chest.x,
                y: chest.y,
            },
            rect,
            Button::Left,
        )?;
    }

    Ok(ChestPass {
        found: chests.len(),
        clicked,
    })
}
