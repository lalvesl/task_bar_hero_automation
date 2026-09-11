//! The tunables, read from `config.toml`.
//!
//! Everything here is a number a human adjusts while calibrating against a real
//! game window. None of it changes the shape of a state machine.

use serde::{Deserialize, Serialize};
use tbh_vision::{NormalizedRect, PixelSignature};

/// The whole configuration file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Which X display the game was launched onto.
    pub display: String,
    /// Settings for the chest task.
    pub chest: ChestConfig,
    /// Settings for the cube task.
    pub cube: CubeConfig,
}

/// Settings for the chest task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChestConfig {
    /// Seconds between frame grabs.
    ///
    /// This is the bot's own power knob. Each cycle costs a capture and a
    /// template match, and the machine this runs on holds its CPU clock down
    /// deliberately.
    pub poll_interval_secs: f32,
    /// Minimum cross-correlation score for a chest sprite to count as found.
    pub match_threshold: f32,
    /// Where on screen chests can appear. Restricting this is the cheapest
    /// speed-up available, because matching cost scales with area.
    pub region: NormalizedRect,
    /// Upper bound on clicks per poll, so a bad match cannot become a click
    /// storm.
    pub max_clicks_per_poll: u8,
}

/// Settings for the cube task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CubeConfig {
    /// Seconds between cube runs.
    pub interval_secs: u32,
    /// Which rarity tier to synthesize up to.
    pub target_rarity: Rarity,
    /// The pixels that say whether the synthesize button is enabled.
    pub synthesize_enabled: PixelSignature,
}

/// A gear rarity tier.
///
/// The tasks never read an item's rarity off the screen; auto-fill picks the
/// items. This only names which tier the cube is set to work on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    /// The lowest tier.
    Grey,
    /// One above grey.
    Green,
    /// One above green.
    Blue,
    /// One above blue.
    Purple,
}
