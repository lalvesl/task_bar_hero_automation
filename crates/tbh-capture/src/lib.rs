//! Frame capture from the isolated X display the game runs on.
//!
//! The game is launched onto a private `Xvfb` display by `scripts/xserver.sh`,
//! so there is no compositor and no other client on that display. The game
//! window is therefore never occluded, and a plain `GetImage` is enough; the
//! `XComposite` machinery an on-desktop capture would need does not apply.
//!
//! The X protocol is spoken through `x11rb`, which is pure Rust.

pub mod x11;

/// A window rectangle on the isolated display, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    /// Distance from the left edge of the screen.
    pub x: i32,
    /// Distance from the top edge of the screen.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// One captured frame, as tightly packed BGRA rows.
///
/// BGRA rather than RGBA because that is the byte order the X server hands
/// back on the little-endian, 24-bit-depth displays this targets. Converting
/// once at the point of use beats converting every frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, four per pixel.
    pub pixels: Vec<u8>,
}

impl Frame {
    /// The red, green and blue of one pixel, or `None` if the coordinates fall
    /// outside the frame.
    ///
    /// Bounds are returned rather than panicking because the caller is usually
    /// resolving a normalized point against a window whose size it read a
    /// moment ago, and the window can change underneath it.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let bgra = self.pixels.get(index..index + 4)?;
        Some([bgra[2], bgra[1], bgra[0]])
    }

    /// Convert to an RGBA image.
    ///
    /// The X server hands back BGRA on the little-endian, 24-bit displays this
    /// targets, so the two colour channels are swapped and the alpha byte,
    /// which the server leaves unset, is forced opaque.
    ///
    /// # Panics
    /// Panics if `pixels` is shorter than `width * height * 4`, which would
    /// mean the server returned a truncated image.
    #[must_use]
    pub fn to_rgba(&self) -> image::RgbaImage {
        let mut rgba = self.pixels.clone();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 0xff;
        }
        image::RgbaImage::from_raw(self.width, self.height, rgba)
            .expect("the server returned fewer bytes than the geometry promised")
    }
}

/// Everything that can go wrong between connecting to the display and holding
/// a frame.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// The display could not be reached. Usually the X server is not running.
    #[error("cannot connect to display {display}: {source}")]
    Connect {
        /// The display that was tried, e.g. `:9`.
        display: String,
        /// The underlying connection failure.
        #[source]
        source: x11rb::errors::ConnectError,
    },

    /// No window on the display matched the search.
    #[error("no window matching {pattern:?} on the display")]
    WindowNotFound {
        /// The name pattern that was searched for.
        pattern: String,
    },

    /// The window does not fit on the virtual screen, so part of it can never
    /// be captured. Raise `TBH_SCREEN` and restart the server.
    #[error("window is {}x{} but the screen is only {}x{}", window.0, window.1, screen.0, screen.1)]
    WindowLargerThanScreen {
        /// The window size, width then height.
        window: (u32, u32),
        /// The screen size, width then height.
        screen: (u32, u32),
    },

    /// The X server rejected a request or the connection broke mid-exchange.
    #[error("X protocol error: {0}")]
    Protocol(#[from] x11rb::errors::ReplyError),
}

/// A source of frames for one window.
///
/// A trait rather than a concrete type so the capture backend can be swapped,
/// and so tests can feed the rest of the pipeline fixture frames instead of a
/// live display.
pub trait Capture {
    /// Where the window currently sits. Click points are resolved against this,
    /// so it is read per action rather than cached.
    ///
    /// # Errors
    /// Fails if the window has gone away or the connection broke.
    fn window_rect(&self) -> Result<WindowRect, CaptureError>;

    /// Grab the window's current contents.
    ///
    /// # Errors
    /// Fails if the window has gone away or the connection broke.
    fn grab(&mut self) -> Result<Frame, CaptureError>;
}
