//! Finding dropped chests without a template.
//!
//! The chest row sits on pure black, so "is there a chest here" reduces to "is
//! this column not black". Columns with content are grouped into runs, and each
//! run is one chest. That handles one chest, two, or however many the game
//! decides to drop, whether they sit centred or side by side, and it costs a
//! scan of a thin band rather than a correlation over the frame.
//!
//! Template matching was the plan for this. It is not needed, and a template
//! would have to be recut whenever the art changes.

use serde::{Deserialize, Serialize};
use tbh_capture::Frame;

use crate::{NormalizedRect, SamplePoint, VisionError};

/// How to find chests in the band they drop into.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChestScan {
    /// The band to scan, as fractions of the window.
    pub region: NormalizedRect,

    /// How bright a pixel's brightest channel must be to count as content.
    ///
    /// Above the black background's noise floor and far below anything the art
    /// actually draws, so the exact value does not matter much.
    pub min_luma: u8,

    /// How many columns of background may sit inside one chest before it is
    /// read as two, as a fraction of the window's width.
    pub max_gap: f32,

    /// How wide a run must be to count as a chest rather than a stray pixel,
    /// as a fraction of the window's width.
    pub min_width: f32,
}

impl ChestScan {
    /// The centre of every chest found in `frame`, normalized against it.
    ///
    /// # Errors
    /// Fails if the region falls outside the unit square.
    pub fn find(&self, frame: &Frame) -> Result<Vec<SamplePoint>, VisionError> {
        let (left, top, width, height) = self.region.resolve(frame)?;
        if width == 0 || height == 0 {
            return Ok(Vec::new());
        }

        let occupied: Vec<bool> = (left..left + width)
            .map(|x| self.column_has_content(frame, x, top, height))
            .collect();

        let max_gap = fraction_to_pixels(self.max_gap, frame.width);
        let min_width = fraction_to_pixels(self.min_width, frame.width);

        let centre_y = f64::from(top) + f64::from(height) / 2.0;
        Ok(runs(&occupied, max_gap)
            .into_iter()
            .filter(|(start, end)| end - start + 1 >= min_width)
            .map(|(start, end)| {
                let centre_x = f64::from(left) + f64::from(start + end) / 2.0;
                SamplePoint {
                    x: (centre_x / f64::from(frame.width)) as f32,
                    y: (centre_y / f64::from(frame.height)) as f32,
                }
            })
            .collect())
    }

    /// Whether any pixel in one column of the band is brighter than the floor.
    fn column_has_content(&self, frame: &Frame, x: u32, top: u32, height: u32) -> bool {
        (top..top + height).any(|y| {
            frame
                .pixel(x, y)
                .is_some_and(|rgb| rgb.iter().copied().max().unwrap_or(0) >= self.min_luma)
        })
    }
}

/// Group occupied columns into runs, bridging gaps no wider than `max_gap`.
///
/// Indices are relative to the start of the slice; the caller adds the band's
/// own offset.
fn runs(occupied: &[bool], max_gap: u32) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut current: Option<(u32, u32)> = None;

    for (index, &is_content) in occupied.iter().enumerate() {
        let index = index as u32;
        if !is_content {
            continue;
        }

        match current {
            Some((start, end)) if index - end <= max_gap + 1 => current = Some((start, index)),
            Some(run) => {
                out.push(run);
                current = Some((index, index));
            }
            None => current = Some((index, index)),
        }
    }

    out.extend(current);
    out
}

/// Turn a fraction of the window's width into a pixel count.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn fraction_to_pixels(fraction: f32, width: u32) -> u32 {
    (f64::from(fraction) * f64::from(width)).max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_columns_form_one_run() {
        assert_eq!(runs(&[true, true, true], 0), vec![(0, 2)]);
    }

    #[test]
    fn a_wide_gap_splits_a_run() {
        let occupied = [true, false, false, false, true];
        assert_eq!(runs(&occupied, 1), vec![(0, 0), (4, 4)]);
    }

    #[test]
    fn a_narrow_gap_is_bridged() {
        let occupied = [true, false, true];
        assert_eq!(runs(&occupied, 1), vec![(0, 2)]);
    }

    /// The two chests measured off a real capture: 75 columns each, six
    /// columns of background between them.
    #[test]
    fn two_real_chests_are_found_separately() {
        assert_eq!(real_chests(2), vec![(20, 94), (101, 175)]);
    }

    /// The bug this cost an afternoon: a gap tolerance as wide as the gap
    /// merges both chests into one blob, and its centre is the empty space
    /// between them, so every click lands on nothing and the chests stay put.
    #[test]
    fn a_gap_tolerance_as_wide_as_the_gap_merges_the_chests() {
        assert_eq!(real_chests(6), vec![(20, 175)]);
    }

    /// Two 75-column chests separated by six columns of background.
    fn real_chests(max_gap: u32) -> Vec<(u32, u32)> {
        let mut occupied = vec![false; 200];
        occupied[20..95].fill(true);
        occupied[101..176].fill(true);
        runs(&occupied, max_gap)
    }
}
