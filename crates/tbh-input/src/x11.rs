//! The `XTEST` backend.
//!
//! `XTEST` is a reference X server extension, so this is the one part of the
//! design that carried no real risk once the game moved onto its own display.
//! The events go to a server holding nothing but the game, which is why there
//! is no guard here against stealing the user's pointer: there is no user
//! pointer on that display to steal.

use std::thread::sleep;
use std::time::Duration;

use tbh_capture::WindowRect;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, Keycode};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::{Button, InputError, Key, Keyboard, NormalizedPoint, Pointer};

/// Motion, press and release, as `XTEST` numbers them.
const MOTION_NOTIFY: u8 = 6;
const BUTTON_PRESS: u8 = 4;
const BUTTON_RELEASE: u8 = 5;
const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

/// How long to hold a press before releasing it, and to settle after moving.
///
/// Unity samples input once per frame. The game is capped near 30 frames per
/// second, so a frame is about 33ms; a press and release sent back to back
/// land inside one frame and the engine never sees a transition. This is set
/// above one frame so both edges are observed, whatever the cap.
const DWELL: Duration = Duration::from_millis(50);

/// A live `XTEST` connection to the isolated display.
pub struct X11Input {
    connection: RustConnection,
    root: u32,
}

impl X11Input {
    /// Connect to `display` and confirm the server offers `XTEST`.
    ///
    /// # Errors
    /// Fails if the display is unreachable or the extension is missing.
    pub fn connect(display: &str) -> Result<Self, InputError> {
        let (connection, screen_index) =
            RustConnection::connect(Some(display)).map_err(|source| InputError::Connect {
                display: display.to_owned(),
                source,
            })?;

        // Asking for the version is the handshake XTEST requires, and doubles
        // as the check that the extension is there at all.
        connection
            .xtest_get_version(2, 2)
            .map_err(to_reply_error)?
            .reply()
            .map_err(|_| InputError::XTestUnavailable)?;

        let root = connection.setup().roots[screen_index].root;
        Ok(Self { connection, root })
    }

    /// Send one fake event and flush, because nothing is delivered until the
    /// request actually leaves the socket.
    fn fake(&self, event_type: u8, detail: u8, x: i16, y: i16) -> Result<(), InputError> {
        self.connection
            .xtest_fake_input(event_type, detail, 0, self.root, x, y, 0)
            .map_err(to_reply_error)?
            .check()
            .map_err(InputError::Protocol)?;
        Ok(())
    }

    /// Find the keycode the current layout assigns to a keysym.
    ///
    /// Looked up rather than hardcoded: a keycode is a property of the
    /// keyboard map, and `Xvfb` builds its map from whatever xkb config the
    /// host happened to hand it.
    fn keycode_for(&self, keysym: u32) -> Result<Keycode, InputError> {
        let setup = self.connection.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let count = max - min + 1;

        let mapping = self
            .connection
            .get_keyboard_mapping(min, count)
            .map_err(to_reply_error)?
            .reply()
            .map_err(InputError::Protocol)?;

        let per_keycode = mapping.keysyms_per_keycode as usize;
        let index = mapping
            .keysyms
            .chunks(per_keycode)
            .position(|syms| syms.contains(&keysym))
            .ok_or(InputError::NoKeycode { keysym })?;

        let offset = u8::try_from(index).map_err(|_| InputError::NoKeycode { keysym })?;
        Ok(min + offset)
    }
}

impl Pointer for X11Input {
    fn click(
        &mut self,
        at: NormalizedPoint,
        within: WindowRect,
        button: Button,
    ) -> Result<(), InputError> {
        let (x, y) = at.resolve(within)?;
        let detail = match button {
            Button::Left => 1,
            Button::Right => 3,
        };

        self.fake(MOTION_NOTIFY, 0, x, y)?;
        sleep(DWELL);
        self.fake(BUTTON_PRESS, detail, x, y)?;
        sleep(DWELL);
        self.fake(BUTTON_RELEASE, detail, x, y)?;
        Ok(())
    }
}

impl Keyboard for X11Input {
    fn press(&mut self, key: Key) -> Result<(), InputError> {
        let keycode = self.keycode_for(key.keysym())?;
        self.fake(KEY_PRESS, keycode, 0, 0)?;
        sleep(DWELL);
        self.fake(KEY_RELEASE, keycode, 0, 0)?;
        Ok(())
    }
}

/// Collapse a connection-level failure into the same error a rejected request
/// produces. Callers cannot act differently on the two.
const fn to_reply_error(error: x11rb::errors::ConnectionError) -> x11rb::errors::ReplyError {
    x11rb::errors::ReplyError::ConnectionError(error)
}
