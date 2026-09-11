//! The `x11rb` backend: connect, find the window, read pixels.
//!
//! Every request here is plain core X11. There is no `XComposite` and no
//! `MIT-SHM`: the isolated display has no compositor and holds nothing but the
//! game, so the window is never occluded and `GetImage` on it returns the whole
//! thing. An on-desktop capture would need both extensions; this one needs
//! neither.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConfigureWindowAux, ConnectionExt, ImageFormat, InputFocus, Window,
};
use x11rb::rust_connection::RustConnection;

use crate::{Capture, CaptureError, Frame, WindowRect};

/// A live connection to the isolated display, bound to one window.
pub struct X11Capture {
    connection: RustConnection,
    window: Window,
    screen_index: usize,
    pattern: String,
}

impl X11Capture {
    /// Connect to `display` and bind to the first window whose name contains
    /// `pattern`.
    ///
    /// The match is a substring rather than an exact name because Unity titles
    /// its window from the product name, which carries no guarantee of matching
    /// the Steam listing.
    ///
    /// # Errors
    /// Fails if the display is unreachable or no window matches.
    pub fn connect(display: &str, pattern: &str) -> Result<Self, CaptureError> {
        let (connection, screen_index) =
            RustConnection::connect(Some(display)).map_err(|source| CaptureError::Connect {
                display: display.to_owned(),
                source,
            })?;

        let root = connection.setup().roots[screen_index].root;
        let window = find_window(&connection, root, pattern)?.ok_or_else(|| {
            CaptureError::WindowNotFound {
                pattern: pattern.to_owned(),
            }
        })?;

        Ok(Self {
            connection,
            window,
            screen_index,
            pattern: pattern.to_owned(),
        })
    }

    /// The window this capture is bound to, for callers that need to name it.
    #[must_use]
    pub const fn window_id(&self) -> u32 {
        self.window
    }

    /// The window's title, as the X server reports it.
    ///
    /// # Errors
    /// Fails if the window has gone away or the connection broke.
    pub fn window_name(&self) -> Result<String, CaptureError> {
        Ok(window_name(&self.connection, self.window)?.unwrap_or_default())
    }

    /// Search the tree again and bind to whatever matches now.
    ///
    /// The game destroys its first window during loading and creates another,
    /// so an id captured at launch stops being valid partway through startup.
    /// Rather than trying to guess when that happens, every request that fails
    /// with a bad window rebinds and tries once more.
    ///
    /// # Errors
    /// Fails if nothing matches any more, which means the game has gone.
    pub fn rebind(&mut self) -> Result<(), CaptureError> {
        let root = self.connection.setup().roots[self.screen_index].root;
        self.window = find_window(&self.connection, root, &self.pattern)?.ok_or_else(|| {
            CaptureError::WindowNotFound {
                pattern: self.pattern.clone(),
            }
        })?;
        Ok(())
    }
}

impl X11Capture {
    /// Move the window fully on-screen if any of it hangs off an edge.
    ///
    /// The isolated display runs no window manager, so the game positions
    /// itself and nothing corrects it. It also restores a saved position from
    /// the Wine registry a moment after launch, and that position has a
    /// negative y. `GetImage` on a window rejects any rectangle not wholly
    /// within the visible screen, and normalized coordinates are meaningless
    /// when part of the window has no pixels to address.
    ///
    /// Moving it is safe here in a way it would not be on a desktop: this
    /// display holds nothing but the game.
    ///
    /// # Errors
    /// Fails if the window has gone away or is larger than the screen.
    pub fn ensure_onscreen(&mut self) -> Result<(), CaptureError> {
        let rect = self.window_rect()?;
        let screen = &self.connection.setup().roots[self.screen_index];
        let (screen_width, screen_height) = (
            i32::from(screen.width_in_pixels),
            i32::from(screen.height_in_pixels),
        );

        let width = i32::try_from(rect.width).unwrap_or(i32::MAX);
        let height = i32::try_from(rect.height).unwrap_or(i32::MAX);
        if width > screen_width || height > screen_height {
            return Err(CaptureError::WindowLargerThanScreen {
                window: (rect.width, rect.height),
                screen: (
                    screen.width_in_pixels.into(),
                    screen.height_in_pixels.into(),
                ),
            });
        }

        let x = rect.x.clamp(0, screen_width - width);
        let y = rect.y.clamp(0, screen_height - height);
        if (x, y) == (rect.x, rect.y) {
            return Ok(());
        }

        self.connection
            .configure_window(self.window, &ConfigureWindowAux::new().x(x).y(y))
            .map_err(to_reply_error)?
            .check()
            .map_err(CaptureError::Protocol)?;
        Ok(())
    }

    /// Set the focus, without the rebind retry.
    fn set_focus(&self) -> Result<(), CaptureError> {
        self.connection
            .set_input_focus(InputFocus::PARENT, self.window, x11rb::CURRENT_TIME)
            .map_err(to_reply_error)?
            .check()
            .map_err(CaptureError::Protocol)?;
        Ok(())
    }

    /// Read the window's position and size, without the rebind retry.
    fn geometry(&self) -> Result<WindowRect, CaptureError> {
        let geometry = self
            .connection
            .get_geometry(self.window)
            .map_err(to_reply_error)?
            .reply()?;

        // The geometry is relative to the parent, so translate to root
        // coordinates. Without this a reparented window reports an offset that
        // is not where clicks have to land.
        let translated = self
            .connection
            .translate_coordinates(self.window, geometry.root, 0, 0)
            .map_err(to_reply_error)?
            .reply()?;

        Ok(WindowRect {
            x: i32::from(translated.dst_x),
            y: i32::from(translated.dst_y),
            width: u32::from(geometry.width),
            height: u32::from(geometry.height),
        })
    }

    /// Read the window's contents, without the rebind retry.
    fn image(&self, rect: WindowRect) -> Result<Frame, CaptureError> {
        let width = u16::try_from(rect.width).unwrap_or(u16::MAX);
        let height = u16::try_from(rect.height).unwrap_or(u16::MAX);

        let image = self
            .connection
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                0,
                0,
                width,
                height,
                !0, // every plane
            )
            .map_err(to_reply_error)?
            .reply()?;

        Ok(Frame {
            width: u32::from(width),
            height: u32::from(height),
            pixels: image.data,
        })
    }
}

impl Capture for X11Capture {
    /// Give the game window the keyboard focus.
    ///
    /// The isolated display runs no window manager, so nothing ever sets a
    /// focus and the server stays on `PointerRoot`: key events go to whatever
    /// window the pointer happens to be over. That makes every key press depend
    /// on where the last click left the cursor, which is why pressing Tab
    /// worked immediately after a click and silently did nothing otherwise.
    ///
    /// # Errors
    /// Fails if the window has gone away or the connection broke.
    fn focus(&mut self) -> Result<(), CaptureError> {
        match self.set_focus() {
            Err(error) if is_stale_window(&error) => {
                self.rebind()?;
                self.set_focus()
            }
            other => other,
        }
    }

    fn window_rect(&mut self) -> Result<WindowRect, CaptureError> {
        match self.geometry() {
            Err(error) if is_stale_window(&error) => {
                self.rebind()?;
                self.geometry()
            }
            other => other,
        }
    }

    fn grab(&mut self) -> Result<Frame, CaptureError> {
        // Checked every time, not once at startup. The game restores its saved
        // window position from the Wine registry a moment after it comes up,
        // and that position has a negative y, which puts part of the window off
        // the screen. `GetImage` rejects any rectangle not wholly visible, so a
        // stale check turns into a BadMatch on every capture from then on.
        self.ensure_onscreen()?;

        let rect = self.window_rect()?;
        match self.image(rect) {
            Err(error) if is_stale_window(&error) => {
                self.rebind()?;
                let rect = self.window_rect()?;
                self.image(rect)
            }
            other => other,
        }
    }
}

/// Whether an error means the window id no longer names anything.
///
/// The game destroys its first window during loading, so this is an expected
/// condition during startup rather than a failure.
const fn is_stale_window(error: &CaptureError) -> bool {
    let CaptureError::Protocol(x11rb::errors::ReplyError::X11Error(inner)) = error else {
        return false;
    };
    matches!(
        inner.error_kind,
        x11rb::protocol::ErrorKind::Window | x11rb::protocol::ErrorKind::Drawable
    )
}

/// Walk the window tree breadth-first, returning the first window whose name
/// contains `pattern`.
///
/// Breadth-first because the game's top-level window sits near the root, while
/// a depth-first walk would descend into whichever subtree it met first.
fn find_window(
    connection: &RustConnection,
    root: Window,
    pattern: &str,
) -> Result<Option<Window>, CaptureError> {
    let mut queue = vec![root];

    while let Some(window) = queue.pop() {
        if let Some(name) = window_name(connection, window)?
            && name.contains(pattern)
        {
            return Ok(Some(window));
        }

        let tree = connection
            .query_tree(window)
            .map_err(to_reply_error)?
            .reply()?;
        queue.extend(tree.children);
    }

    Ok(None)
}

/// Read a window's title, preferring the UTF-8 `_NET_WM_NAME` over the legacy
/// `WM_NAME`.
fn window_name(
    connection: &RustConnection,
    window: Window,
) -> Result<Option<String>, x11rb::errors::ReplyError> {
    let net_wm_name = connection
        .intern_atom(false, b"_NET_WM_NAME")
        .map_err(to_reply_error)?
        .reply()?
        .atom;

    for atom in [net_wm_name, AtomEnum::WM_NAME.into()] {
        let property = connection
            .get_property(false, window, atom, AtomEnum::ANY, 0, u32::MAX)
            .map_err(to_reply_error)?
            .reply()?;

        if !property.value.is_empty() {
            return Ok(Some(String::from_utf8_lossy(&property.value).into_owned()));
        }
    }

    Ok(None)
}

/// Collapse a connection-level failure into the same error type a rejected
/// request produces. Callers cannot act differently on the two, and carrying
/// both through every signature buys nothing.
const fn to_reply_error(error: x11rb::errors::ConnectionError) -> x11rb::errors::ReplyError {
    x11rb::errors::ReplyError::ConnectionError(error)
}
