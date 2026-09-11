//! Synthetic pointer and keyboard input on the isolated X display.
//!
//! `XTEST` is safe here in a way it would not be on the user's desktop: the
//! events land on a display that holds nothing but the game, so they can never
//! steal the real pointer or reach a focused application. That isolation is
//! also why the `uinput` backend an on-desktop design would need was dropped;
//! `uinput` injects into the host seat and would miss this display entirely.

pub mod watch;
pub mod x11;

use tbh_capture::WindowRect;

/// A point inside the game window, expressed as a fraction of the window's
/// size.
///
/// Normalized rather than absolute so a configured click point survives the
/// window moving or the virtual screen being resized. It is resolved against
/// the live window rectangle at click time.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NormalizedPoint {
    /// Horizontal position, 0.0 at the left edge and 1.0 at the right.
    pub x: f32,
    /// Vertical position, 0.0 at the top edge and 1.0 at the bottom.
    pub y: f32,
}

impl NormalizedPoint {
    /// Turn this into absolute screen coordinates inside `window`.
    ///
    /// # Errors
    /// Fails if either coordinate falls outside the unit square, which would
    /// mean a click aimed outside the window it was configured against.
    pub fn resolve(self, window: WindowRect) -> Result<(i16, i16), InputError> {
        for (value, axis) in [(self.x, "x"), (self.y, "y")] {
            if !(0.0..=1.0).contains(&value) {
                return Err(InputError::OutOfWindow { axis, value });
            }
        }

        // The cast is the intent: the value is clamped into range on the line
        // above it, so truncating towards zero is what turns a fractional
        // pixel into the pixel it falls in.
        #[allow(clippy::cast_possible_truncation)]
        let absolute = |offset: i32, size: u32, fraction: f32| -> i16 {
            let scaled = f64::from(fraction) * f64::from(size);
            let total = f64::from(offset) + scaled;
            total.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
        };

        Ok((
            absolute(window.x, window.width, self.x),
            absolute(window.y, window.height, self.y),
        ))
    }
}

/// Which mouse button an action uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// The primary button, which is every click the v1 tasks need.
    Left,
    /// The secondary button.
    Right,
}

/// A key the tasks need to send.
///
/// Deliberately a short list rather than a full keyboard: the game's menus are
/// reached with one key, and an open-ended API would invite the bot to type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Opens the game's main menu.
    Tab,
    /// Backs out of a menu.
    Escape,
}

impl Key {
    /// The X keysym for this key.
    #[must_use]
    pub const fn keysym(self) -> u32 {
        match self {
            Self::Tab => 0xff09,
            Self::Escape => 0xff1b,
        }
    }
}

/// Everything that can go wrong while injecting input.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// The display could not be reached.
    #[error("cannot connect to display {display}: {source}")]
    Connect {
        /// The display that was tried, e.g. `:9`.
        display: String,
        /// The underlying connection failure.
        #[source]
        source: x11rb::errors::ConnectError,
    },

    /// The server does not offer the `XTEST` extension.
    #[error("the X server does not support XTEST")]
    XTestUnavailable,

    /// The server does not offer `XInput2`, so human clicks cannot be observed.
    #[error("the X server does not support XInput2")]
    XInputUnavailable,

    /// A normalized coordinate fell outside the window.
    #[error("{axis} coordinate {value} is outside the window")]
    OutOfWindow {
        /// Which axis was rejected.
        axis: &'static str,
        /// The offending value.
        value: f32,
    },

    /// The current keyboard layout has no key for the requested keysym.
    #[error("no keycode maps to keysym {keysym:#x}")]
    NoKeycode {
        /// The keysym that was not found.
        keysym: u32,
    },

    /// The X server rejected a request or the connection broke mid-exchange.
    #[error("X protocol error: {0}")]
    Protocol(#[from] x11rb::errors::ReplyError),
}

/// A pointer that can be moved and clicked.
///
/// A trait rather than a concrete type so tasks can be tested against a
/// recording fake instead of a live display.
pub trait Pointer {
    /// Move the pointer to a point in `within` and press and release a button
    /// there, returning where on screen it landed.
    ///
    /// The resolved position is returned rather than discarded because a caller
    /// that observes the display needs to recognise its own click coming back.
    ///
    /// # Errors
    /// Fails if the display is unreachable, the point is outside the window, or
    /// the request is rejected.
    fn click(
        &mut self,
        at: NormalizedPoint,
        within: WindowRect,
        button: Button,
    ) -> Result<(i16, i16), InputError>;
}

/// A keyboard that can press and release a key.
pub trait Keyboard {
    /// Press and release `key`.
    ///
    /// # Errors
    /// Fails if the display is unreachable or the layout has no such key.
    fn press(&mut self, key: Key) -> Result<(), InputError>;
}
