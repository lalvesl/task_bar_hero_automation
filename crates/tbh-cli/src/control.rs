//! The state the command loop and the worker share.
//!
//! One mutex around a small struct, rather than channels: every field here is
//! read far more often than it is written, the worker polls on a timer anyway,
//! and a reader that blocks for the length of a boolean read is not a problem
//! this program has.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tbh_core::chest::ChestPass;
use tbh_core::cube::CubeRun;

/// A task the operator can switch on and off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// Equipment synthesis in the cube.
    Synthesis,
    /// Collecting from the chest slots.
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

/// One task's switch and its last result.
#[derive(Debug)]
pub struct TaskState<R> {
    /// Whether the task may run.
    pub enabled: bool,
    /// When it last finished, if it has.
    pub last: Option<Instant>,
    /// What that run did, absent if it failed.
    pub result: Option<R>,
}

impl<R> TaskState<R> {
    /// Start from the configuration's own switch.
    const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last: None,
            result: None,
        }
    }

    /// Whether enough time has passed to run again.
    ///
    /// A task that has never run is due immediately, so enabling one does not
    /// mean waiting out a full interval first.
    pub fn due(&self, interval: Duration) -> bool {
        self.last.is_none_or(|last| last.elapsed() >= interval)
    }

    /// Record a finished run.
    pub fn finished(&mut self, result: Option<R>) {
        self.result = result;
        self.last = Some(Instant::now());
    }

    /// How long ago it last ran.
    pub fn since(&self) -> Option<Duration> {
        self.last.map(|last| last.elapsed())
    }
}

/// What the worker is allowed to do, and what it last did.
#[derive(Debug)]
pub struct State {
    /// Equipment synthesis in the cube.
    pub synthesis: TaskState<CubeRun>,
    /// Set by `run synthesis` to skip the wait for the next interval.
    pub synthesis_now: bool,
    /// Collecting from the chest slots.
    pub chests: TaskState<ChestPass>,
    /// Set by `quit`, so the worker can finish its current step and stop.
    pub shutdown: bool,
}

impl State {
    /// Start from the configuration's own switches.
    #[must_use]
    pub const fn new(synthesis_enabled: bool, chests_enabled: bool) -> Self {
        Self {
            synthesis: TaskState::new(synthesis_enabled),
            synthesis_now: false,
            chests: TaskState::new(chests_enabled),
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
    control.lock().unwrap_or_else(PoisonError::into_inner)
}
