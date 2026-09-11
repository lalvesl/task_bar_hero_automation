//! Recognising what is on screen, cheapest check first.
//!
//! The order matters more than any individual technique. A region of interest
//! keeps a search off the rest of the frame; a pixel signature answers a yes or
//! no question for the cost of a few byte comparisons; template matching only
//! runs for targets whose position cannot be known in advance. In the v1 scope
//! that last case is dropped chests, and nothing else.

use serde::{Deserialize, Serialize};

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

/// One pixel a signature check expects to find, and what colour it should be.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SamplePoint {
    /// Where to sample, normalized against the window.
    pub x: f32,
    /// Where to sample, normalized against the window.
    pub y: f32,
    /// Expected colour as red, green, blue.
    pub rgb: [u8; 3],
}

/// A handful of sampled pixels that together identify a UI state.
///
/// This is what decides whether the cube's synthesize button is enabled. A
/// disabled button is drawn greyed out, so a few points inside it separate the
/// two states without any matching at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PixelSignature {
    /// The points to sample.
    pub points: Vec<SamplePoint>,
    /// How far each channel may drift before the sample counts as a miss.
    pub tolerance: u8,
}

/// Where a template was found, and how well it matched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    /// Centre of the match, normalized against the window.
    pub x: f32,
    /// Centre of the match, normalized against the window.
    pub y: f32,
    /// Normalized cross-correlation score, where 1.0 is a perfect match.
    pub score: f32,
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
