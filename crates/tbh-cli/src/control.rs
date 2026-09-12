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
    /// Whether the operator has asked for a run that is not due yet.
    ///
    /// Set by `run` and by `force`, cleared by the worker when the run ends.
    /// It stands on its own: `run` checks `enabled` before setting it, and
    /// `force` sets it in spite of `enabled`, so by the time the worker sees
    /// it the decision has already been made.
    pub queued: bool,
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
            queued: false,
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

    /// Record a finished run, and clear any request that asked for it.
    pub fn finished(&mut self, result: Option<R>) {
        self.result = result;
        self.last = Some(Instant::now());
        self.queued = false;
    }

    /// Whether this task should run on this tick.
    pub fn wanted(&self, interval: Duration) -> bool {
        self.queued || (self.enabled && self.due(interval))
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
    /// Collecting from the chest slots.
    pub chests: TaskState<ChestPass>,
    /// Whether the bot has its hands off the game, and on whose terms.
    pub halt: Halt,

    /// Whether the UI has to be driven back to a known state before the tasks
    /// resume, because a person has been clicking around in it.
    pub needs_restore: bool,

    /// Set by `quit`, so the worker can finish its current step and stop.
    pub shutdown: bool,
}

/// Whether the bot may touch the game.
///
/// One value rather than a pause and a switch side by side, because the two
/// have to be ordered against each other and a pair of fields cannot say which
/// wins. A click stops the bot for a few seconds and starts it again on its
/// own; a typed `stop` is the operator taking the game, and nothing but a
/// typed `start` gives it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halt {
    /// The bot may act.
    Running,
    /// A person clicked, so the bot stands down until this instant and then
    /// starts itself.
    Clicked(Instant),
    /// The operator typed `stop`. There is no timer on this one.
    Manual,
}

impl State {
    /// Whether the bot is currently standing down.
    #[must_use]
    pub fn halted(&self) -> bool {
        match self.halt {
            Halt::Running => false,
            Halt::Clicked(until) => Instant::now() < until,
            Halt::Manual => true,
        }
    }

    /// Stand down for `window` because a person clicked.
    ///
    /// A manual stop is left alone: it was typed, this was inferred, and the
    /// typed one is not something a click may put a timer on.
    pub fn clicked(&mut self, window: Duration) {
        if self.halt == Halt::Manual {
            return;
        }
        self.halt = Halt::Clicked(Instant::now() + window);
        self.needs_restore = true;
    }

    /// Take the game away from the bot until a typed `start`.
    pub const fn stop(&mut self) {
        self.halt = Halt::Manual;
    }

    /// Give it back, with the menus put in order on the way in.
    pub const fn start(&mut self) {
        self.halt = Halt::Running;
        self.needs_restore = true;
    }

    /// Clear an expired click stand-down, reporting whether it did.
    ///
    /// Only the click kind expires. Returning whether anything changed is what
    /// lets the worker say so once, rather than every tick.
    pub fn expire(&mut self) -> bool {
        if matches!(self.halt, Halt::Clicked(until) if Instant::now() >= until) {
            self.halt = Halt::Running;
            return true;
        }
        false
    }

    /// Start from the configuration's own switches.
    #[must_use]
    pub const fn new(synthesis_enabled: bool, chests_enabled: bool) -> Self {
        Self {
            synthesis: TaskState::new(synthesis_enabled),
            chests: TaskState::new(chests_enabled),
            halt: Halt::Running,
            needs_restore: false,
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

#[cfg(test)]
mod tests {
    use super::{Halt, State};
    use std::time::Duration;

    fn state() -> State {
        State::new(true, true)
    }

    #[test]
    fn a_click_stands_the_bot_down_and_lifts_itself() {
        let mut state = state();
        state.clicked(Duration::from_millis(20));
        assert!(state.halted());
        assert!(state.needs_restore, "the person may have moved the menus");

        std::thread::sleep(Duration::from_millis(30));
        assert!(state.expire(), "the window has run out");
        assert_eq!(state.halt, Halt::Running);
        assert!(!state.expire(), "and it is only reported once");
    }

    #[test]
    fn a_click_does_not_put_a_timer_on_a_typed_stop() {
        let mut state = state();
        state.stop();
        state.clicked(Duration::from_millis(1));

        assert_eq!(state.halt, Halt::Manual);
        std::thread::sleep(Duration::from_millis(10));
        assert!(!state.expire(), "nothing but a typed start lifts this");
        assert!(state.halted());
    }

    #[test]
    fn start_lifts_either_kind_and_asks_for_the_menus_back() {
        for mut state in [state(), state()] {
            state.stop();
            state.start();
            assert!(!state.halted());
            assert!(state.needs_restore);
        }
    }
}
