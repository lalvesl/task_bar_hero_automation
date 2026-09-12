//! Putting the game's UI back where the tasks expect it.
//!
//! Run at startup, and again whenever a person has been clicking around. Rather
//! than working out what changed, this drives the UI to a definite state: no
//! dialog, menu open, cube and stash open.
//!
//! Two rules shape every step here.
//!
//! It only ever opens. An earlier version pressed Tab twice to cycle an open
//! menu closed and open again, which reset nothing that matters and flashed the
//! whole interface at whoever had just been using it.
//!
//! It gives way. The sequence takes several seconds, and a person who paused
//! for breath and then carried on would otherwise have it click straight
//! through everything they did next. Between steps it asks whether anyone has
//! taken over, and stops if so.

use std::thread::sleep;
use std::time::Duration;

use tbh_capture::Capture;
use tbh_input::{Button, Key, Keyboard, NormalizedPoint, Pointer};
use tbh_vision::ChannelLead;

use crate::config::Config;
use crate::{Task, TaskError};

/// How long to let each step land before the next one.
const STEP: Duration = Duration::from_millis(700);

/// How many times to click a control before giving up on a state.
const ATTEMPTS: u8 = 4;

/// How the sequence ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restored {
    /// The UI is in the expected state.
    Done,
    /// Someone is using the display, so the sequence stopped partway.
    Interrupted,
}

/// Dismiss the launch dialog, open the menu, and open the cube and the stash.
///
/// # Errors
/// Fails if the window cannot be read or a click or key cannot be delivered.
pub fn run<A: Pointer + Keyboard>(
    config: &Config,
    capture: &mut dyn Capture,
    actor: &mut A,
) -> Result<Restored, TaskError> {
    // Without this the keys below go nowhere: the display has no window
    // manager, so nothing has ever set a focus.
    capture.focus()?;

    for step in [Step::Popup, Step::Menu, Step::Cube, Step::Stash] {
        if actor.interrupted()? {
            return Ok(Restored::Interrupted);
        }
        step.run(config, capture, actor)?;
    }

    Ok(Restored::Done)
}

/// One stage of the sequence, named so the loop above can check for a person
/// between each.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Close the launch dialog, if it is up.
    Popup,
    /// Leave the main menu open.
    Menu,
    /// Leave the cube panel open.
    Cube,
    /// Leave the stash panel open.
    Stash,
}

impl Step {
    fn run<A: Pointer + Keyboard>(
        self,
        config: &Config,
        capture: &mut dyn Capture,
        actor: &mut A,
    ) -> Result<(), TaskError> {
        match self {
            Self::Popup => dismiss_popup(config, capture, actor),
            Self::Menu => open_menu(config, capture, actor),
            Self::Cube => ensure_open(
                capture,
                actor,
                config.cube.menu_icon,
                &config.cube.panel_visible,
                "opening the cube panel",
            ),
            Self::Stash => ensure_open(
                capture,
                actor,
                config.stash.menu_icon,
                &config.stash.panel_visible,
                "opening the stash panel",
            ),
        }
    }
}

/// Close the launch dialog, if it is up.
///
/// Checked rather than clicked blindly. With no dialog on screen that position
/// sits inside the hero panel, so an unconditional click there would press
/// whatever the game happens to be drawing.
fn dismiss_popup(
    config: &Config,
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
) -> Result<(), TaskError> {
    let frame = capture.grab()?;
    if !config.popup.visible.holds(&frame)? {
        return Ok(());
    }

    click(capture, pointer, config.popup.close)?;
    sleep(STEP);
    Ok(())
}

/// Press Tab until the menu is open, and not once more.
fn open_menu<A: Pointer + Keyboard>(
    config: &Config,
    capture: &mut dyn Capture,
    actor: &mut A,
) -> Result<(), TaskError> {
    for _ in 0..ATTEMPTS {
        let frame = capture.grab()?;
        if config.menu.visible.holds(&frame)? {
            return Ok(());
        }
        actor.press(Key::Tab)?;
        sleep(STEP);
    }

    Err(TaskError::Timeout {
        task: Task::Cube,
        state: "opening the main menu",
    })
}

/// Click a menu icon until its panel is open.
///
/// The icons toggle, so a blind click opens a closed panel and closes an open
/// one. An earlier version clicked each once on the assumption that the panels
/// start closed. When they did not, the restore closed the cube it was supposed
/// to be opening, and the synthesis task then read the empty space where the
/// mode selector should have been and reported the wrong mode.
fn ensure_open<A: Pointer>(
    capture: &mut dyn Capture,
    actor: &mut A,
    icon: NormalizedPoint,
    visible: &ChannelLead,
    what: &'static str,
) -> Result<(), TaskError> {
    for _ in 0..ATTEMPTS {
        let frame = capture.grab()?;
        if visible.holds(&frame)? {
            return Ok(());
        }
        click(capture, actor, icon)?;
        sleep(STEP);
    }

    Err(TaskError::Timeout {
        task: Task::Cube,
        state: what,
    })
}

/// Resolve a point against the window as it is right now, and click it.
fn click(
    capture: &mut dyn Capture,
    pointer: &mut dyn Pointer,
    at: NormalizedPoint,
) -> Result<(), TaskError> {
    let rect = capture.window_rect()?;
    pointer.click(at, rect, Button::Left)?;
    Ok(())
}
