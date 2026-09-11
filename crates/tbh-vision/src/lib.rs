//! Recognising what is on screen, cheapest check first.
//!
//! The order matters more than any individual technique. A region of interest
//! keeps a search off the rest of the frame; a channel-lead check answers a yes
//! or no question for the cost of a few byte comparisons; template matching
//! only runs for targets whose position cannot be known in advance. In the v1
//! scope that last case is dropped chests, and nothing else.

pub mod scan;

use serde::{Deserialize, Serialize};
use tbh_capture::Frame;

/// A sub-rectangle of the game window, as fractions of its size.
///
/// Normalized for the same reason click points are: the window rectangle is
/// read live, so a region configured once stays correct when the window moves
/// or the virtual screen is resized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRect {
    /// Left edge, 0.0 at the window's left.
    pub x: f32,
    /// Top edge, 0.0 at the window's top.
    pub y: f32,
    /// Width as a fraction of the window's width.
    pub width: f32,
    /// Height as a fraction of the window's height.
    pub height: f32,
}

impl NormalizedRect {
    /// Resolve to pixel left, top, width and height inside `frame`.
    ///
    /// # Errors
    /// Fails if any edge falls outside the unit square, which would mean the
    /// rectangle was configured against a different window.
    pub fn resolve(&self, frame: &Frame) -> Result<(u32, u32, u32, u32), VisionError> {
        for (value, what) in [
            (self.x, "x"),
            (self.y, "y"),
            (self.width, "width"),
            (self.height, "height"),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(VisionError::OutOfBounds { what, value });
            }
        }

        // Clamped so a rectangle that reaches the far edge stays inside the
        // frame instead of naming a pixel one past it.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scale = |fraction: f32, size: u32| -> u32 {
            (f64::from(fraction) * f64::from(size)).clamp(0.0, f64::from(size)) as u32
        };

        let left = scale(self.x, frame.width);
        let top = scale(self.y, frame.height);
        Ok((
            left,
            top,
            scale(self.width, frame.width).min(frame.width - left),
            scale(self.height, frame.height).min(frame.height - top),
        ))
    }
}

/// One pixel to sample, as fractions of the window's size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SamplePoint {
    /// Horizontal position, 0.0 at the window's left.
    pub x: f32,
    /// Vertical position, 0.0 at the window's top.
    pub y: f32,
}

/// A colour channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
}

impl Channel {
    /// Pull this channel out of an RGB triple.
    const fn of(self, rgb: [u8; 3]) -> u8 {
        match self {
            Self::Red => rgb[0],
            Self::Green => rgb[1],
            Self::Blue => rgb[2],
        }
    }
}

/// A test that one colour channel leads another by a margin, at every one of a
/// handful of sampled pixels.
///
/// This is how the cube's synthesize button is read. Disabled, it is drawn in
/// pure grey, so every channel is equal. Enabled, it is blue, and blue leads
/// red by around 80.
///
/// A relative test rather than a comparison against stored colours: the
/// relationship between two channels survives brightness drift, hover
/// highlighting, and a repaint in a future patch, where absolute values would
/// all have to be recaptured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelLead {
    /// The channel expected to be higher.
    pub lead: Channel,
    /// The channel expected to be lower.
    pub over: Channel,
    /// How far ahead `lead` must be for the test to hold.
    pub min_delta: u8,
    /// The pixels to sample. All of them must satisfy the test.
    pub points: Vec<SamplePoint>,
}

impl ChannelLead {
    /// Whether the test holds across every sampled pixel of `frame`.
    ///
    /// # Errors
    /// Fails if a sample point falls outside the frame, which means the point
    /// was configured against a different window size.
    pub fn holds(&self, frame: &Frame) -> Result<bool, VisionError> {
        for point in &self.points {
            let rgb = sample(frame, *point)?;
            let delta = i16::from(self.lead.of(rgb)) - i16::from(self.over.of(rgb));
            if delta < i16::from(self.min_delta) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Read the pixel a normalized point names.
fn sample(frame: &Frame, point: SamplePoint) -> Result<[u8; 3], VisionError> {
    for (value, what) in [(point.x, "x"), (point.y, "y")] {
        if !(0.0..=1.0).contains(&value) {
            return Err(VisionError::OutOfBounds { what, value });
        }
    }

    // Truncating towards zero after clamping to the last valid index, so a
    // point at exactly 1.0 lands on the final pixel instead of one past it.
    // The clamp is what makes both casts safe.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = |fraction: f32, size: u32| -> u32 {
        let scaled = f64::from(fraction) * f64::from(size);
        let last = f64::from(size.saturating_sub(1));
        scaled.clamp(0.0, last) as u32
    };

    let x = index(point.x, frame.width);
    let y = index(point.y, frame.height);
    frame.pixel(x, y).ok_or(VisionError::OutOfFrame { x, y })
}

/// Everything that can go wrong while inspecting a frame.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// A normalized rectangle or point fell outside the unit square.
    #[error("{what} is out of bounds: {value}")]
    OutOfBounds {
        /// Which value was rejected.
        what: &'static str,
        /// The offending value.
        value: f32,
    },

    /// A resolved pixel fell outside the captured frame.
    #[error("pixel ({x}, {y}) is outside the frame")]
    OutOfFrame {
        /// Resolved horizontal position.
        x: u32,
        /// Resolved vertical position.
        y: u32,
    },

    /// A template could not be read from disk.
    #[error("cannot load template {path}: {source}")]
    TemplateLoad {
        /// The path that failed.
        path: String,
        /// The underlying decoding failure.
        #[source]
        source: image::ImageError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a one-pixel frame of the given colour, in the BGRA order the X
    /// server uses.
    fn frame_of(rgb: [u8; 3]) -> Frame {
        Frame {
            width: 1,
            height: 1,
            pixels: vec![rgb[2], rgb[1], rgb[0], 0xff],
        }
    }

    fn blue_over_red(min_delta: u8) -> ChannelLead {
        ChannelLead {
            lead: Channel::Blue,
            over: Channel::Red,
            min_delta,
            points: vec![SamplePoint { x: 0.0, y: 0.0 }],
        }
    }

    #[test]
    fn grey_does_not_lead() {
        // The disabled synthesize button, sampled from the real game.
        let frame = frame_of([113, 113, 113]);
        assert!(!blue_over_red(40).holds(&frame).unwrap());
    }

    #[test]
    fn blue_leads() {
        // The enabled synthesize button, sampled from the real game.
        let frame = frame_of([33, 81, 115]);
        assert!(blue_over_red(40).holds(&frame).unwrap());
    }

    #[test]
    fn a_point_outside_the_unit_square_is_rejected() {
        let mut check = blue_over_red(40);
        check.points = vec![SamplePoint { x: 1.5, y: 0.0 }];
        assert!(matches!(
            check.holds(&frame_of([0, 0, 0])),
            Err(VisionError::OutOfBounds { what: "x", .. })
        ));
    }
}
