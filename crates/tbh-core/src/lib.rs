//! The farming tasks, and the tunables they read.
//!
//! The state machines are written in Rust rather than driven by a declarative
//! pipeline: the v1 scope is two tasks, and enums keep them type-checked. The
//! numbers are not hardcoded, because they are retuned constantly during
//! calibration and recompiling per attempt would kill the feedback loop.

pub mod config;

/// Which farming task a state belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// Detect dropped chests and click them.
    Chest,
    /// Open the cube, auto-fill, synthesize while it keeps filling.
    Cube,
}

/// Where the cube task currently is.
///
/// Auto-fill does the item selection, so no state here needs to know what an
/// item is or what rarity it carries. Whether the fill succeeded is read off
/// the synthesize button, which is greyed out when the inventory has run dry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeState {
    /// Waiting for the next cube interval to elapse.
    Idle,
    /// Opening the cube panel.
    Opening,
    /// Clicking auto-fill.
    Filling,
    /// Reading the synthesize button to see whether the fill succeeded.
    CheckingFill,
    /// Clicking synthesize.
    Synthesizing,
    /// Closing the cube panel, the inventory having run out.
    Closing,
}

/// Everything that can stop a task mid-run.
#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// The window could not be captured.
    #[error(transparent)]
    Capture(#[from] tbh_capture::CaptureError),

    /// A click could not be delivered.
    #[error(transparent)]
    Input(#[from] tbh_input::InputError),

    /// A frame could not be interpreted.
    #[error(transparent)]
    Vision(#[from] tbh_vision::VisionError),

    /// A state waited longer than its budget allows, which means the game is
    /// not where the machine believes it is.
    #[error("{task:?} timed out in state {state}")]
    Timeout {
        /// The task that stalled.
        task: Task,
        /// The state it stalled in.
        state: &'static str,
    },
}
