//! A private headless X server on a display number nobody else is using.

use super::{SCREEN_HEIGHT, SCREEN_WIDTH};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::thread::sleep;
use std::time::{Duration, Instant};
use x11rb::rust_connection::RustConnection;

/// Display numbers are taken from this range, far above the ones a desktop session
/// uses, so a test server never lands on a live display.
const DISPLAYS: std::ops::RangeInclusive<u32> = 90..=190;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Tests run on several threads. One starts a server at a time, so two never race for
/// the same display number.
static STARTING: Mutex<()> = Mutex::new(());

/// A running `Xvfb`, stopped when dropped.
pub struct XServer {
    child: Child,
    display: String,
}

enum Startup {
    Ready,
    Exited,
    TimedOut,
}

impl XServer {
    /// Whether `Xvfb` can be run at all.
    pub fn available() -> bool {
        Command::new("Xvfb")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    /// Starts a server on the first display number that is not in use.
    pub fn start() -> Result<Self, String> {
        let _starting = STARTING.lock().unwrap_or_else(PoisonError::into_inner);
        for number in DISPLAYS {
            // Both files are checked: either one means a live server may own the number.
            if Path::new(&format!("/tmp/.X11-unix/X{number}")).exists()
                || Path::new(&format!("/tmp/.X{number}-lock")).exists()
            {
                continue;
            }
            let display = format!(":{number}");
            let screen = format!("{SCREEN_WIDTH}x{SCREEN_HEIGHT}x24");
            let mut child = Command::new("Xvfb")
                .args([
                    display.as_str(),
                    "-screen",
                    "0",
                    &screen,
                    // Without it the server restarts when its last client leaves, which
                    // is right after the readiness probe below, and the test client's own
                    // connection then lands in the restart and is reset.
                    "-noreset",
                    "-nolisten",
                    "tcp",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("cannot start Xvfb: {e}"))?;
            match wait_until_ready(&mut child, &display) {
                Startup::Ready => return Ok(Self { child, display }),
                // Another process took the number between the check and the start.
                Startup::Exited => {}
                Startup::TimedOut => {
                    stop(&mut child);
                    return Err(format!("Xvfb on {display} did not accept connections"));
                }
            }
        }
        Err("no free display number to start Xvfb on".into())
    }

    /// The display name, such as `:90`.
    pub fn display(&self) -> &str {
        &self.display
    }
}

impl Drop for XServer {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}

fn wait_until_ready(child: &mut Child, display: &str) -> Startup {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return Startup::Exited;
        }
        if RustConnection::connect(Some(display)).is_ok() {
            return Startup::Ready;
        }
        sleep(Duration::from_millis(20));
    }
    Startup::TimedOut
}

/// Asks the process to end, so `Xvfb` removes its socket and lock file, then reaps it.
pub fn stop(child: &mut Child) {
    let asked = Command::new("kill")
        .arg(child.id().to_string())
        .status()
        .is_ok_and(|status| status.success());
    if !asked {
        // The process is gone or cannot be signalled this way; either way, make sure.
        let _ = child.kill();
    }
    let _ = child.wait();
}
