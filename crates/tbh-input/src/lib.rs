//! Synthetic pointer input on the isolated X display.
//!
//! `XTEST` is safe here in a way it would not be on the user's desktop: the
//! events land on a display that holds nothing but the game, so they can never
//! steal the real pointer or reach a focused application. That isolation is
//! also why the `uinput` backend an on-desktop design would need was dropped;
//! `uinput` injects into the host seat and would miss this display entirely.

/// A point inside the game window, expressed as a fraction of the window's
/// size.
///
/// Normalized rather than absolute so a configured click point survives the
/// window moving or the virtual screen being resized. It is resolved against
/// the live window rectangle at click time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedPoint {
    /// Horizontal position, 0.0 at the left edge and 1.0 at the right.
    pub x: f32,
    /// Vertical position, 0.0 at the top edge and 1.0 at the bottom.
    pub y: f32,
}

/// Which mouse button an action uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// The primary button, which is every click the v1 tasks need.
    Left,
    /// The secondary button.
    Right,
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

    /// The X server rejected a request or the connection broke mid-exchange.
    #[error("X protocol error: {0}")]
    Protocol(#[from] x11rb::errors::ReplyError),
}

/// A pointer that can be moved and clicked.
///
/// A trait rather than a concrete type so tasks can be tested against a
/// recording fake instead of a live display.
pub trait Pointer {
    /// Move the pointer to a point in the window and press and release a
    /// button there.
    ///
    /// # Errors
    /// Fails if the display is unreachable or the request is rejected.
    fn click(&mut self, at: NormalizedPoint, button: Button) -> Result<(), InputError>;
}
