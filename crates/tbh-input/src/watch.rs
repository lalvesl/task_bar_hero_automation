//! Noticing when a person takes over.
//!
//! The operator can open a viewer onto the isolated display and click in it. A
//! click of theirs while the bot is mid-task would fight for the same UI, so the
//! bot has to see it and stand down.
//!
//! ## Why this cannot just read the device id
//!
//! `x11vnc` injects the viewer's clicks through `XTEST`, the same path the bot
//! uses. Both arrive from the virtual XTEST pointer, so the source device
//! separates nothing.
//!
//! What does separate them is that the bot knows what it sent. Every injected
//! click is recorded, and an observed click that matches a recent one of ours in
//! both place and time is ours. Anything else came from a person.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

use crate::InputError;

/// How long an injected click stays unmatched before it is given up on.
///
/// Entries are normally removed by being matched against the event they
/// produced, which arrives within milliseconds because the queue is drained
/// after every click. This is only a safety valve so a click whose event never
/// arrives cannot hold a slot forever.
///
/// It was 600ms once, which was a bug: the restore sequence blocks for nearly
/// three seconds, so by the time the queue was drained its own clicks had
/// expired and read as a person taking over. That paused the bot, which
/// triggered a restore, which paused it again.
const LEDGER_LIFETIME: Duration = Duration::from_secs(5);

/// How far an observed click may sit from an injected one and still be counted
/// as the same click, in pixels.
const LEDGER_RADIUS: i32 = 3;

/// A click the bot injected, kept so the same click can be recognised coming
/// back the other way.
#[derive(Debug, Clone, Copy)]
struct Injected {
    x: i16,
    y: i16,
    at: Instant,
}

/// Watches the display for clicks the bot did not make.
pub struct X11Watch {
    connection: RustConnection,
    root: Window,
    ledger: VecDeque<Injected>,
    human: bool,
}

impl X11Watch {
    /// Connect to `display` and start observing raw button presses.
    ///
    /// # Errors
    /// Fails if the display is unreachable or the server has no `XInput2`.
    pub fn connect(display: &str) -> Result<Self, InputError> {
        let (connection, screen_index) =
            RustConnection::connect(Some(display)).map_err(|source| InputError::Connect {
                display: display.to_owned(),
                source,
            })?;

        connection
            .xinput_xi_query_version(2, 2)
            .map_err(to_reply_error)?
            .reply()
            .map_err(|_| InputError::XInputUnavailable)?;

        let root = connection.setup().roots[screen_index].root;

        // Raw events are the only ones delivered regardless of which window the
        // pointer is over, which is what makes a passive observer possible
        // without grabbing the pointer away from the game.
        let mask = xinput::EventMask {
            deviceid: xinput::Device::ALL_MASTER.into(),
            mask: vec![xinput::XIEventMask::RAW_BUTTON_PRESS],
        };
        connection
            .xinput_xi_select_events(root, &[mask])
            .map_err(to_reply_error)?
            .check()
            .map_err(InputError::Protocol)?;

        Ok(Self {
            connection,
            root,
            ledger: VecDeque::new(),
            human: false,
        })
    }

    /// Record a click the bot is about to make.
    pub fn record_injected(&mut self, x: i16, y: i16) {
        self.expire();
        self.ledger.push_back(Injected {
            x,
            y,
            at: Instant::now(),
        });
    }

    /// Drain pending events, matching each against the ledger.
    ///
    /// Called after every injected click as well as on the worker tick, so an
    /// event is reconciled while the click that caused it is still in the
    /// ledger. A long task that blocks the tick would otherwise let its own
    /// clicks age out and come back looking like a person.
    ///
    /// # Errors
    /// Fails if the connection breaks.
    pub fn poll(&mut self) -> Result<(), InputError> {
        self.expire();

        while let Some(event) = self
            .connection
            .poll_for_event()
            .map_err(|error| InputError::Protocol(to_reply_error(error)))?
        {
            if !matches!(event, Event::XinputRawButtonPress(_)) {
                continue;
            }

            // Raw events carry no position, so the pointer is asked where it is.
            // The query happens within microseconds of the press, and the
            // pointer does not move on its own.
            let pointer = self
                .connection
                .query_pointer(self.root)
                .map_err(to_reply_error)?
                .reply()
                .map_err(InputError::Protocol)?;

            if self.take_matching(pointer.root_x, pointer.root_y).is_none() {
                self.human = true;
            }
        }

        Ok(())
    }

    /// Whether a person has clicked, without clearing the flag.
    ///
    /// Long sequences read this between steps so they can stand down partway
    /// through. They peek rather than consume so the worker still sees the
    /// same click and pushes its pause out.
    #[must_use]
    pub const fn human_seen(&self) -> bool {
        self.human
    }

    /// Whether a person has clicked since this was last asked, clearing the
    /// flag.
    pub const fn take_human(&mut self) -> bool {
        std::mem::replace(&mut self.human, false)
    }

    /// Remove and return the ledger entry a click at this position matches.
    ///
    /// Removed rather than merely found, so two rapid clicks on one spot are
    /// not both explained by a single injected one.
    fn take_matching(&mut self, x: i16, y: i16) -> Option<Injected> {
        let index = self.ledger.iter().position(|injected| {
            (i32::from(injected.x) - i32::from(x)).abs() <= LEDGER_RADIUS
                && (i32::from(injected.y) - i32::from(y)).abs() <= LEDGER_RADIUS
        })?;
        self.ledger.remove(index)
    }

    /// Drop ledger entries too old to explain anything.
    fn expire(&mut self) {
        while self
            .ledger
            .front()
            .is_some_and(|injected| injected.at.elapsed() > LEDGER_LIFETIME)
        {
            self.ledger.pop_front();
        }
    }
}

/// Collapse a connection-level failure into the same error a rejected request
/// produces. Callers cannot act differently on the two.
const fn to_reply_error(error: x11rb::errors::ConnectionError) -> x11rb::errors::ReplyError {
    x11rb::errors::ReplyError::ConnectionError(error)
}
