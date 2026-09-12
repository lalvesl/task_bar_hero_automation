//! The tunables, read from `config.toml`.
//!
//! Everything here is a number or a switch a human adjusts while calibrating
//! against a real game window. None of it changes the shape of a state machine.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tbh_input::NormalizedPoint;
use tbh_vision::ChannelLead;
use tbh_vision::region::RegionLead;
use tbh_vision::scan::ChestScan;

/// The whole configuration file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Which X display the game was launched onto.
    pub display: String,
    /// Substring of the game window title.
    pub window: String,

    /// How long the bot stands down after a person clicks in the viewer.
    pub pause_after_click_secs: f32,

    /// How long to wait after the window appears before acting at all.
    ///
    /// The game plays a long opening sequence, and a click during it lands on
    /// whatever the intro happens to be drawing.
    pub startup_delay_secs: f32,

    /// The game Steam app id, used to launch it.
    pub steam_app_id: u32,

    /// Loopback port the `show` mirror serves on.
    pub vnc_port: u16,
    /// The stash panel.
    pub stash: StashConfig,
    /// The cube task.
    pub cube: CubeConfig,
    /// The chest task.
    pub chest: ChestConfig,
    /// The launch dialog.
    pub popup: PopupConfig,
    /// The main menu.
    pub menu: MenuConfig,
}

/// The main menu, and how to tell whether it is open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuConfig {
    /// How to tell whether the menu is on screen.
    pub visible: ChannelLead,
}

/// The dialog the game opens on launch, and how to be rid of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupConfig {
    /// The button that closes it.
    pub close: NormalizedPoint,

    /// How to tell whether it is up.
    pub visible: ChannelLead,
}

/// The chest task: whether it runs, how often, and where to look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChestConfig {
    /// Whether the task runs at all.
    pub enabled: bool,

    /// Seconds between passes.
    ///
    /// This is the bot own power knob. Every pass costs a capture and a scan,
    /// on a machine whose CPU clock is held down on purpose.
    pub interval_secs: f32,

    /// Upper bound on clicks in one pass, so a misread band cannot become a
    /// click storm.
    pub max_clicks_per_pass: u8,

    /// How to find the chests.
    pub scan: ChestScan,
}

impl Config {
    /// Read and parse a configuration file.
    ///
    /// # Errors
    /// Fails if the file cannot be read or does not parse.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.display().to_string(),
            source,
        })
    }
}

/// Where the stash panel's controls are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StashConfig {
    /// The stash icon on the main menu bottom row.
    pub menu_icon: NormalizedPoint,
    /// The button that deposits the whole inventory.
    pub store_all: NormalizedPoint,
    /// How to tell whether the stash panel is open.
    pub panel_visible: ChannelLead,
}

/// The cube task: whether it runs, how, and where its controls are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CubeConfig {
    /// Whether the task runs at all.
    ///
    /// The switch lives here rather than only in the UI so a run can be turned
    /// off without the UI existing, and so the setting survives a restart.
    pub enabled: bool,

    /// Deposit the inventory into the stash before synthesizing.
    ///
    /// Auto-fill draws from the stash as well as the inventory, so depositing
    /// first is what makes a full inventory available to the cube.
    pub store_all_first: bool,

    /// Seconds between runs while the task is enabled.
    ///
    /// The floor, not the schedule: `after_chest` is what usually decides when
    /// a run happens.
    pub interval_secs: u32,

    /// Run as soon as a chest pass has collected something.
    ///
    /// A chest puts items in the inventory, and the inventory has a fixed
    /// number of slots. A night of collecting without synthesizing filled them
    /// and the game stopped accepting anything more, which no interval short
    /// of the time it takes to fill the slots would have prevented. Tying the
    /// run to the thing that fills the slots does.
    pub after_chest: bool,

    /// Upper bound on syntheses in one run.
    ///
    /// A stop that does not depend on reading the screen correctly. If the
    /// enabled check ever misreads, this is what keeps a loop finite.
    pub max_per_run: u16,

    /// The cube icon on the main menu's bottom row.
    pub menu_icon: NormalizedPoint,
    /// The button that fills the grid.
    pub auto_fill: NormalizedPoint,
    /// The button that performs the synthesis.
    pub synthesize: NormalizedPoint,

    /// How to tell whether the synthesize button is enabled.
    pub synthesize_enabled: ChannelLead,

    /// How to tell the cube is in synthesis mode rather than one of the seven
    /// others the mode selector offers.
    ///
    /// In synthesis mode the selector icon is blue; in every other mode it is
    /// not. Without this check, a cube left on Creation looks exactly like an
    /// exhausted inventory: auto-fill does nothing useful and the synthesize
    /// button stays grey.
    pub mode_is_synthesis: RegionLead,

    /// How to tell whether the cube panel is open at all.
    ///
    /// Without this, a panel that failed to reopen and an inventory with
    /// nothing left to combine look identical: in both cases auto-fill leaves
    /// the synthesize button grey.
    pub panel_visible: ChannelLead,
}

/// Everything that can go wrong while loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The path that failed.
        path: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },

    /// The file is not valid TOML, or does not match the schema.
    #[error("cannot parse {path}: {source}")]
    Parse {
        /// The path that failed.
        path: String,
        /// The underlying failure.
        #[source]
        source: toml::de::Error,
    },
}
