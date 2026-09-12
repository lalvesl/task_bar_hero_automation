//! Reading the average colour of a small region.
//!
//! The single-pixel checks elsewhere work on flat, solid areas: a title banner,
//! a button face. They do not work on a twelve-pixel icon full of internal
//! gaps, where a normalized coordinate truncating one pixel lower lands on the
//! black between two strokes and the whole test fails.
//!
//! Averaging a region sidesteps that. It costs a few hundred pixel reads, still
//! nothing next to a template match, and it does not care where inside the icon
//! the sample lands.

use serde::{Deserialize, Serialize};
use tbh_capture::Frame;

use crate::{Channel, NormalizedRect, VisionError};

/// A test that one colour channel leads another, averaged over a region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionLead {
    /// The region to average, as fractions of the window.
    pub region: NormalizedRect,
    /// The channel expected to be higher.
    pub lead: Channel,
    /// The channel expected to be lower.
    pub over: Channel,
    /// How far ahead `lead` must average for the test to hold.
    pub min_delta: u8,
}

impl RegionLead {
    /// Whether the test holds for `frame`.
    ///
    /// # Errors
    /// Fails if the region falls outside the unit square.
    pub fn holds(&self, frame: &Frame) -> Result<bool, VisionError> {
        Ok(self.delta(frame)? >= f64::from(self.min_delta))
    }

    /// How far the leading channel averages ahead of the other. Negative when
    /// it is behind, which is what separates the two states in practice.
    ///
    /// # Errors
    /// Fails if the region falls outside the unit square.
    pub fn delta(&self, frame: &Frame) -> Result<f64, VisionError> {
        let (left, top, width, height) = self.region.resolve(frame)?;
        if width == 0 || height == 0 {
            return Ok(0.0);
        }

        let mut lead = 0u64;
        let mut over = 0u64;
        let mut counted = 0u64;

        for y in top..top + height {
            for x in left..left + width {
                if let Some(rgb) = frame.pixel(x, y) {
                    lead += u64::from(self.lead.of(rgb));
                    over += u64::from(self.over.of(rgb));
                    counted += 1;
                }
            }
        }

        if counted == 0 {
            return Ok(0.0);
        }

        // A region is a few hundred pixels of one byte each, so these sums are
        // nowhere near the range where f64 loses whole numbers.
        #[allow(clippy::cast_precision_loss)]
        {
            let counted = counted as f64;
            Ok(lead as f64 / counted - over as f64 / counted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of one colour, in the BGRA order the X server uses.
    fn flat(width: u32, height: u32, rgb: [u8; 3]) -> Frame {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            pixels.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 0xff]);
        }
        Frame {
            width,
            height,
            pixels,
        }
    }

    fn blue_over_red() -> RegionLead {
        RegionLead {
            region: NormalizedRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            lead: Channel::Blue,
            over: Channel::Red,
            min_delta: 8,
        }
    }

    /// The synthesis mode icon, averaged off a real capture: blue ahead of red
    /// by about 20.
    #[test]
    fn the_synthesis_icon_leads_blue() {
        let frame = flat(8, 8, [35, 43, 55]);
        assert!(blue_over_red().holds(&frame).unwrap());
    }

    /// The creation mode icon: blue behind red by about 26.
    #[test]
    fn the_creation_icon_does_not() {
        let frame = flat(8, 8, [82, 67, 56]);
        assert!(!blue_over_red().holds(&frame).unwrap());
    }

    /// An icon is mostly background. Averaging is what makes the test survive
    /// that, where a single pixel landing in a gap would not.
    #[test]
    fn a_mostly_black_icon_still_reads() {
        let mut frame = flat(8, 8, [0, 0, 0]);
        for index in 0..16 {
            let at = index * 4;
            frame.pixels[at] = 247; // blue, in BGRA order
            frame.pixels[at + 1] = 170;
            frame.pixels[at + 2] = 33;
        }
        assert!(blue_over_red().holds(&frame).unwrap());
    }
}
