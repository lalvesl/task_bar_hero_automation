//! The `x11rb` backend: connect, find the window, read pixels.
//!
//! Every request here is plain core X11. There is no `XComposite` and no
//! `MIT-SHM`: the isolated display has no compositor and holds nothing but the
//! game, so the window is never occluded and `GetImage` on it returns the whole
//! thing. An on-desktop capture would need both extensions; this one needs
//! neither.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt, ImageFormat, Window};
use x11rb::rust_connection::RustConnection;

use crate::{Capture, CaptureError, Frame, WindowRect};

/// A live connection to the isolated display, bound to one window.
pub struct X11Capture {
    connection: RustConnection,
    window: Window,
    screen_index: usize,
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
        })
    }

    /// The window this capture is bound to, for callers that need to name it.
    #[must_use]
    pub const fn window_id(&self) -> u32 {
        self.window
    }

    /// Move the window fully on-screen if any of it hangs off an edge.
    ///
    /// The isolated display runs no window manager, so the game positions
    /// itself and nothing corrects it; it lands at a negative y offset. That
    /// matters because `GetImage` on a window rejects any rectangle not
    /// wholly within the visible screen, and because normalized coordinates
    /// are meaningless when part of the window has no pixels to address.
    ///
    /// Moving it is safe here in a way it would not be on a desktop: this
    /// display holds nothing but the game.
    ///
    /// # Errors
    /// Fails if the window has gone away or the connection broke.
    pub fn ensure_onscreen(&self) -> Result<(), CaptureError> {
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
                screen: (screen.width_in_pixels.into(), screen.height_in_pixels.into()),
            });
        }

        let x = rect.x.clamp(0, screen_width - width);
        let y = rect.y.clamp(0, screen_height - height);
        if (x, y) == (rect.x, rect.y) {
            return Ok(());
        }

        self.connection
            .configure_window(
                self.window,
                &ConfigureWindowAux::new().x(x).y(y),
            )
            .map_err(to_reply_error)?
            .check()
            .map_err(|error| match error {
                x11rb::errors::ReplyError::X11Error(inner) => {
                    CaptureError::Protocol(x11rb::errors::ReplyError::X11Error(inner))
                }
                other => CaptureError::Protocol(other),
            })?;
        Ok(())
    }

    /// The window's title, as the X server reports it.
    ///
    /// # Errors
    /// Fails if the window has gone away or the connection broke.
    pub fn window_name(&self) -> Result<String, CaptureError> {
        Ok(window_name(&self.connection, self.window)?.unwrap_or_default())
    }
}

impl Capture for X11Capture {
    fn window_rect(&self) -> Result<WindowRect, CaptureError> {
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

    fn grab(&mut self) -> Result<Frame, CaptureError> {
        let rect = self.window_rect()?;
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
fn to_reply_error(error: x11rb::errors::ConnectionError) -> x11rb::errors::ReplyError {
    x11rb::errors::ReplyError::ConnectionError(error)
}
