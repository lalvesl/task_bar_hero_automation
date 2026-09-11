//! The `click` and `key` subcommands.
//!
//! These exist so the input layer can be driven by hand while click points are
//! being picked, without a task having to exist first. The tasks call the same
//! crate; nothing here is a parallel implementation.

use anyhow::Context as _;
use tbh_capture::{Capture as _, x11::X11Capture};
use tbh_input::{Button, Key, Keyboard as _, NormalizedPoint, Pointer as _, x11::X11Input};

/// Which display and window to act on.
#[derive(Debug, clap::Args)]
pub struct Target {
    /// The X display the game was launched onto.
    #[arg(long, default_value = ":9", global = true)]
    pub display: String,

    /// Substring of the game window's title.
    #[arg(long, default_value = "TaskBarHero", global = true)]
    pub window: String,
}

/// Arguments for `tbh click`.
#[derive(Debug, clap::Args)]
pub struct ClickArgs {
    /// Horizontal position as a fraction of the window's width.
    #[arg(long)]
    x: f32,

    /// Vertical position as a fraction of the window's height.
    #[arg(long)]
    y: f32,

    /// Use the right button instead of the left.
    #[arg(long)]
    right: bool,
}

/// Arguments for `tbh key`.
#[derive(Debug, clap::Args)]
pub struct KeyArgs {
    /// Which key to press.
    #[arg(value_enum)]
    key: KeyName,
}

/// The keys the binary will send.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum KeyName {
    /// Opens the game's main menu.
    Tab,
    /// Backs out of a menu.
    Escape,
}

impl From<KeyName> for Key {
    fn from(name: KeyName) -> Self {
        match name {
            KeyName::Tab => Self::Tab,
            KeyName::Escape => Self::Escape,
        }
    }
}

/// Click one normalized point in the game window.
///
/// # Errors
/// Fails if the display is unreachable, no window matches, or the point falls
/// outside the window.
pub fn click(target: &Target, args: &ClickArgs) -> anyhow::Result<()> {
    let mut capture = connect(target)?;
    let rect = capture.window_rect().context("reading window geometry")?;

    let mut input = X11Input::connect(&target.display)
        .with_context(|| format!("opening XTEST on {}", target.display))?;
    let button = if args.right {
        Button::Right
    } else {
        Button::Left
    };

    let at = NormalizedPoint {
        x: args.x,
        y: args.y,
    };
    let (screen_x, screen_y) = input.click(at, rect, button).context("clicking")?;

    println!("clicked {button:?} at ({screen_x}, {screen_y}) on screen");
    Ok(())
}

/// Press one key.
///
/// # Errors
/// Fails if the display is unreachable or the layout has no such key.
pub fn key(target: &Target, args: &KeyArgs) -> anyhow::Result<()> {
    // Focused first, not merely located. The isolated display has no window
    // manager, so the server stays on `PointerRoot` and an unfocused key press
    // lands wherever the pointer happens to be sitting.
    let mut capture = connect(target)?;
    capture.focus().context("focusing the game window")?;

    let mut input = X11Input::connect(&target.display)
        .with_context(|| format!("opening XTEST on {}", target.display))?;
    input.press(args.key.into()).context("pressing the key")?;

    println!("pressed {:?}", args.key);
    Ok(())
}

/// Connect to the display and bind to the game window, moving it on-screen if
/// the game placed it over an edge.
fn connect(target: &Target) -> anyhow::Result<X11Capture> {
    let mut capture = X11Capture::connect(&target.display, &target.window).with_context(|| {
        format!(
            "connecting to {} for window {:?}",
            target.display, target.window
        )
    })?;
    capture
        .ensure_onscreen()
        .context("moving the window fully on-screen")?;
    Ok(capture)
}
