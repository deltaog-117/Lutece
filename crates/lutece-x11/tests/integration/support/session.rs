//! A private X server with the example window manager running on it.

use super::client::{Client, Geometry, Spec};
use super::server::{XServer, stop};
use super::{SCREEN_HEIGHT, SCREEN_WIDTH};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Once;

pub struct Session {
    // Field order is drop order: the window manager goes before its server.
    wm: Option<Child>,
    server: XServer,
    pub client: Client,
}

impl Session {
    /// Starts a server and a client, without a window manager yet. Returns `None` (after
    /// saying why) when there is no `Xvfb`, unless `LUTECE_REQUIRE_X11` is set, in which
    /// case that is a failure.
    pub fn bare() -> Option<Self> {
        if !XServer::available() {
            assert!(
                std::env::var_os("LUTECE_REQUIRE_X11").is_none(),
                "Xvfb is required (LUTECE_REQUIRE_X11 is set) but cannot be run"
            );
            eprintln!("skipped: Xvfb is not installed");
            return None;
        }
        let server = XServer::start().unwrap_or_else(|e| panic!("{e}"));
        let client = Client::connect(server.display());
        Some(Self {
            wm: None,
            server,
            client,
        })
    }

    /// Starts a server with the window manager already managing it.
    pub fn start() -> Option<Self> {
        let mut session = Self::bare()?;
        session.start_wm();
        Some(session)
    }

    /// Starts the window manager and returns once it has taken over the screen.
    pub fn start_wm(&mut self) {
        let wm = Command::new(example_binary())
            .env("DISPLAY", self.server.display())
            .stdin(Stdio::null())
            .spawn()
            .expect("start the example window manager");
        self.wm = Some(wm);
        self.wait_until_managing();
    }

    /// The display name, for tests that open more connections.
    pub fn display(&self) -> &str {
        self.server.display()
    }

    /// Whether the window manager process has exited, waiting for it if it is quitting.
    pub fn wm_exited(&mut self) -> bool {
        let Some(wm) = self.wm.as_mut() else {
            return true;
        };
        self.client.wait_for("the window manager to exit", || {
            wm.try_wait().ok().flatten().map(|_| ())
        });
        true
    }

    /// A window that is mapped before the window manager runs is left alone until it
    /// takes over, and is then tiled. Opening one and waiting for it to be resized
    /// tells that the window manager is running, grabbed its keys and reached its loop.
    fn wait_until_managing(&self) {
        let asked = Geometry::new(10, 10, 50, 50);
        let probe = self.client.open(Spec::at(10, 10, 50, 50));
        self.client.wait_for("the window manager to take over", || {
            self.client.geometry(probe).filter(|g| *g != asked)
        });
        self.client.destroy(probe);
    }

    /// The whole screen as a geometry.
    pub const fn screen() -> Geometry {
        Geometry::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(wm) = self.wm.as_mut() {
            stop(wm);
        }
    }
}

/// Path of the example window manager, built first if `cargo test` has not built it.
fn example_binary() -> PathBuf {
    static BUILD: Once = Once::new();
    let exe = std::env::current_exe().expect("path of the test executable");
    // target/<profile>/deps/<test>  ->  target/<profile>/examples/minimal
    let path = exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("test executable sits in target/<profile>/deps")
        .join("examples")
        .join("minimal");
    BUILD.call_once(|| {
        if path.exists() {
            return;
        }
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let status = Command::new(cargo)
            .args([
                "build",
                "--quiet",
                "-p",
                "lutece-x11",
                "--example",
                "minimal",
            ])
            .status()
            .expect("run cargo to build the example");
        assert!(
            status.success(),
            "building the example window manager failed"
        );
    });
    path
}
