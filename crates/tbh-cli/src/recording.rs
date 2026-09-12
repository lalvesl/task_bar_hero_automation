//! A pointer that tells the watcher what it just did.
//!
//! The tasks take `&mut dyn Pointer` and know nothing about being observed.
//! This sits between them and the real pointer, forwarding the click and
//! filing the resolved position, so the watcher can tell the bot's own clicks
//! apart from a person's.

use tbh_capture::WindowRect;
use tbh_input::watch::X11Watch;
use tbh_input::x11::X11Input;
use tbh_input::{Button, InputError, NormalizedPoint, Pointer};

/// Wraps the real pointer and records every click into the watcher's ledger.
pub struct Recording<'a> {
    /// The pointer that actually talks to the display.
    pub pointer: &'a mut X11Input,
    /// The watcher whose ledger the clicks are filed in.
    pub watch: &'a mut X11Watch,
}

impl Pointer for Recording<'_> {
    fn click(
        &mut self,
        at: NormalizedPoint,
        within: WindowRect,
        button: Button,
    ) -> Result<(i16, i16), InputError> {
        // Filed before the click is sent. The observed event can arrive while
        // the injecting call is still returning, and an unrecorded click of
        // ours reads as a person taking over.
        let (x, y) = at.resolve(within)?;
        self.watch.record_injected(x, y);

        let landed = self.pointer.click(at, within, button)?;

        // Reconciled straight away rather than on the next worker tick: a task
        // that blocks for seconds would otherwise let its own clicks age out
        // of the ledger and come back looking like a person taking over.
        self.watch.poll()?;
        Ok(landed)
    }

    fn interrupted(&mut self) -> Result<bool, InputError> {
        self.watch.poll()?;
        Ok(self.watch.human_seen())
    }
}

impl tbh_input::Keyboard for Recording<'_> {
    fn press(&mut self, key: tbh_input::Key) -> Result<(), InputError> {
        // No ledger entry: the watcher observes button presses only, so a key
        // the bot sends can never be mistaken for a person's click.
        self.pointer.press(key)
    }
}
