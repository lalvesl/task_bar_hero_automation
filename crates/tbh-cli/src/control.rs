//! The state the command loop and the worker share.
//!
//! One mutex around a small struct, rather than channels: every field here is
//! read far more often than it is written, the worker polls on a timer anyway,
//! and a reader that blocks for the length of a boolean read is not a problem
//! this program has.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tbh_core::chest::ChestPass;
use tbh_core::cube::CubeRun;

/// A task the operator can switch on and off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// Equipment synthesis in the cube.
    Synthesis,
    /// Clicking dropped chests.
    Chests,
}

impl Task {
    /// Parse a task name, accepting the phrasings an operator would actually
    /// type rather than one exact spelling.
    #[must_use]
    pub fn parse(words: &str) -> Option<Self> {
        match words.trim() {
            "synthesis" | "synthesis equipment" | "equipment synthesis" | "cube" => {
                Some(Self::Synthesis)
            }
            "chests" | "chest" | "open chests" => Some(Self::Chests),
            _ => None,
        }
    }

    /// The canonical name, for printing back.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Synthesis => "synthesis",
            Self::Chests => "chests",
        }
    }
}

/// What the worker is allowed to do, and what it last did.
#[derive(Debug)]
pub struct State {
    /// Whether the synthesis task may run.
    pub synthesis_enabled: bool,
    /// Set by `run synthesis` to skip the wait for the next interval.
    pub synthesis_now: bool,
    /// When the synthesis task last finished, if it has.
    pub synthesis_last: Option<Instant>,
    /// What that run did.
    pub synthesis_result: Option<CubeRun>,
    /// Whether the chest task may run.
    pub chests_enabled: bool,
    /// When the chest task last ran, if it has.
    pub chests_last: Option<Instant>,
    /// What that pass did.
    pub chests_result: Option<ChestPass>,

    /// Set by `quit`, so the worker can finish its current step and stop.
    pub shutdown: bool,
}

impl State {
    /// Start from the configuration's own switches.
    #[must_use]
    pub const fn new(synthesis_enabled: bool, chests_enabled: bool) -> Self {
        Self {
            synthesis_enabled,
            synthesis_now: false,
            synthesis_last: None,
            synthesis_result: None,
            chests_enabled,
            chests_last: None,
            chests_result: None,
            shutdown: false,
        }
    }
}

/// A handle both threads hold.
pub type Control = Arc<Mutex<State>>;

/// Take the lock, recovering from a panic in the other thread.
///
/// A poisoned mutex here means the worker died mid-update. The state it guards
/// is a few switches, none of which can be left half-written, so carrying on is
/// better than bringing the whole process down with it.
pub fn lock(control: &Control) -> std::sync::MutexGuard<'_, State> {
    control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
