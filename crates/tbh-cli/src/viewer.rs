//! The `show` and `hide` commands.
//!
//! The whole point of the isolated display is that the game stays out of the
//! way, so the mirror is something you turn on to look and turn off again, not
//! something that runs alongside the bot.
//!
//! It attaches to the running server over VNC rather than switching the server
//! to Xephyr, because switching would mean restarting the display and therefore
//! relaunching the game.

use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::Context as _;

/// The mirror, while it is up.
#[derive(Debug, Default)]
pub struct Viewer {
    server: Option<Child>,
    window: Option<Child>,
}

impl Viewer {
    /// Whether the mirror is currently up.
    #[must_use]
    pub const fn is_shown(&self) -> bool {
        self.window.is_some()
    }

    /// Mirror `display` into a window on the host desktop.
    ///
    /// # Errors
    /// Fails if either program is missing or cannot start.
    pub fn show(&mut self, display: &str, port: u16, auth: &Path) -> anyhow::Result<()> {
        if self.is_shown() {
            return Ok(());
        }

        // x11vnc refuses to start when it sees a Wayland session in the
        // environment, even though the display it is asked to mirror is a plain
        // X server with no compositor anywhere near it.
        let server = Command::new("x11vnc")
            .args(["-display", display])
            .arg("-auth")
            .arg(auth)
            .args(["-rfbport", &port.to_string()])
            .args(["-localhost", "-nopw", "-forever", "-shared", "-quiet"])
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("XDG_SESSION_TYPE")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("starting x11vnc; is it on PATH?")?;
        self.server = Some(server);

        wait_for_port(port).context("x11vnc did not start listening")?;

        // XAUTHORITY is cleared for the viewer: it draws its own window on the
        // host desktop, and the cookie this process carries is the isolated
        // display's, which the host server would reject.
        let window = Command::new("vncviewer")
            .arg(format!("localhost:{port}"))
            .env_remove("XAUTHORITY")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("starting vncviewer; is it on PATH?")?;
        self.window = Some(window);

        Ok(())
    }

    /// Take the mirror down, leaving nothing listening.
    pub fn hide(&mut self) {
        for child in [self.window.take(), self.server.take()]
            .into_iter()
            .flatten()
        {
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.hide();
    }
}

/// Wait for something to start listening on a loopback port.
fn wait_for_port(port: u16) -> anyhow::Result<()> {
    use std::net::TcpStream;
    use std::time::Duration;

    for _ in 0..50 {
        if TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(100))
            .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("nothing is listening on 127.0.0.1:{port}")
}
