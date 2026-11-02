//! X11 programs: a real `Xwayland` runs under the backend and the tests act as an X11
//! client of it.
//!
//! They need the `Xwayland` program. Without it each test prints a notice and passes;
//! set `LUTECE_REQUIRE_X11=1` (as CI should) to make a missing program a failure.

use crate::support::{backend, engine};
use lutece::{Engine, Event, WinId};
use lutece_wayland::WaylandBackend;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeGCAux, ConnectionExt, CreateGCAux, CreateWindowAux, Rectangle, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT};

const WAIT: Duration = Duration::from_secs(10);
const STEP: Duration = Duration::from_millis(5);

/// Whether `Xwayland` can be run at all. Returns `false` (after saying why) when it
/// cannot, unless `LUTECE_REQUIRE_X11` is set, in which case that is a failure.
fn available() -> bool {
    let found = Command::new("Xwayland")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok();
    if !found {
        assert!(
            std::env::var_os("LUTECE_REQUIRE_X11").is_none(),
            "Xwayland is required (LUTECE_REQUIRE_X11 is set) but cannot be run"
        );
        eprintln!("skipped: Xwayland is not installed");
    }
    found
}

/// The backend with Xwayland running, an engine, and an X11 client connected to it.
struct Fixture {
    backend: WaylandBackend,
    engine: Engine,
    x: RustConnection,
    root: Window,
    /// Events the backend reported that the engine has not seen yet.
    waiting: Vec<Event>,
    /// Every event the backend reported.
    seen: Vec<Event>,
}

impl Fixture {
    fn start() -> Option<Self> {
        if !available() {
            return None;
        }
        let mut backend = backend();
        backend.enable_xwayland().expect("Xwayland starts");
        let mut waiting = Vec::new();
        let deadline = Instant::now() + WAIT;
        while backend.xwayland_display().is_none() {
            assert!(Instant::now() < deadline, "Xwayland never became ready");
            waiting.extend(backend.next_events(STEP).expect("events"));
        }
        let display = format!(":{}", backend.xwayland_display().expect("ready"));
        let (x, screen) = RustConnection::connect(Some(&display))
            .unwrap_or_else(|error| panic!("cannot connect to {display}: {error}"));
        let root = x.setup().roots[screen].root;
        Some(Self {
            backend,
            engine: engine(),
            x,
            root,
            seen: waiting.clone(),
            waiting,
        })
    }

    /// Steps the backend until the events seen so far satisfy `done`.
    fn wait_for_events(&mut self, what: &str, done: impl Fn(&[Event]) -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done(&self.seen) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            let events = self.backend.next_events(STEP).expect("events");
            self.seen.extend(events.iter().cloned());
            self.waiting.extend(events);
        }
    }

    /// Steps the backend until `check` returns a value.
    fn wait_for<T>(&mut self, what: &str, mut check: impl FnMut(&mut Self) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(found) = check(self) {
                return found;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            let events = self.backend.next_events(STEP).expect("events");
            self.seen.extend(events.iter().cloned());
            self.waiting.extend(events);
        }
    }

    /// Gives the engine the events it has not seen and applies what it answers.
    fn drive(&mut self) {
        for event in std::mem::take(&mut self.waiting) {
            self.backend
                .apply_effects(&self.engine.handle(event))
                .expect("effects apply");
        }
        self.backend
            .apply_scene(&self.engine.scene().expect("a valid scene"))
            .expect("scene applies");
    }

    /// Opens, describes, maps and draws on an X11 window.
    fn open(
        &self,
        at: (i16, i16),
        size: (u16, u16),
        title: &str,
        class: &str,
        menu: bool,
    ) -> Window {
        let window = self.x.generate_id().expect("window id");
        let aux = CreateWindowAux::new()
            .background_pixel(0x00ff_0000)
            .override_redirect(u32::from(menu));
        self.x
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                self.root,
                at.0,
                at.1,
                size.0,
                size.1,
                0,
                WindowClass::INPUT_OUTPUT,
                COPY_FROM_PARENT,
                &aux,
            )
            .expect("create_window");
        self.x
            .change_property8(
                x11rb::protocol::xproto::PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .expect("title");
        self.x
            .change_property8(
                x11rb::protocol::xproto::PropMode::REPLACE,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                format!("{}\0{class}\0", class.to_lowercase()).as_bytes(),
            )
            .expect("class");
        self.x.map_window(window).expect("map_window");
        // Xwayland shows a window on a surface only once there is something to show.
        let gc = self.x.generate_id().expect("gc id");
        self.x
            .create_gc(gc, window, &CreateGCAux::new().foreground(0x0000_ff00))
            .expect("create_gc");
        self.x
            .change_gc(gc, &ChangeGCAux::new().foreground(0x0000_00ff))
            .expect("change_gc");
        self.x
            .poly_fill_rectangle(
                window,
                gc,
                &[Rectangle {
                    x: 0,
                    y: 0,
                    width: size.0,
                    height: size.1,
                }],
            )
            .expect("poly_fill_rectangle");
        self.x.flush().expect("flush");
        window
    }

    /// Where `window` is on the screen, whatever it is parented to.
    fn position(&self, window: Window) -> Option<(i16, i16, u16, u16)> {
        let size = self.x.get_geometry(window).ok()?.reply().ok()?;
        let at = self
            .x
            .translate_coordinates(window, self.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        Some((at.dst_x, at.dst_y, size.width, size.height))
    }
}

fn opened(events: &[Event]) -> Vec<(WinId, String, String)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::WindowOpened { id, info } => Some((*id, info.title.clone(), info.class.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn an_x11_window_is_reported_to_the_core_with_its_title_and_class() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };

    fixture.open((10, 10), (50, 50), "xterm", "XTerm", false);
    fixture.wait_for_events("the X11 window to be reported", |events| {
        !opened(events).is_empty()
    });

    let windows = opened(&fixture.seen);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].1, "xterm");
    assert_eq!(windows[0].2, "XTerm");
}

#[test]
fn the_core_places_an_x11_window_like_any_other() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let window = fixture.open((10, 10), (50, 50), "one", "One", false);
    fixture.wait_for_events("the X11 window to be reported", |events| {
        !opened(events).is_empty()
    });

    fixture.drive();

    // A single tiled window fills the 1000x600 output.
    let at = fixture.wait_for("the window to be placed", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 0, 1000, 600))
    });
    assert_eq!(at, (0, 0, 1000, 600));
}

#[test]
fn a_second_x11_window_shares_the_screen() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let first = fixture.open((10, 10), (50, 50), "one", "One", false);
    fixture.wait_for_events("the first window", |events| opened(events).len() == 1);
    let second = fixture.open((20, 20), (50, 50), "two", "Two", false);
    fixture.wait_for_events("the second window", |events| opened(events).len() == 2);

    fixture.drive();

    // The newest window takes the master column, 55% of the width.
    fixture.wait_for("both windows to be placed", |fixture| {
        let (a, b) = (fixture.position(first)?, fixture.position(second)?);
        (b == (0, 0, 550, 600) && a == (550, 0, 450, 600)).then_some(())
    });
}

#[test]
fn closing_an_x11_window_tells_the_core() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let window = fixture.open((10, 10), (50, 50), "gone", "Gone", false);
    fixture.wait_for_events("the window", |events| !opened(events).is_empty());
    let id = opened(&fixture.seen)[0].0;

    fixture.x.destroy_window(window).expect("destroy_window");
    fixture.x.flush().expect("flush");

    fixture.wait_for_events("the close", |events| {
        events.contains(&Event::WindowClosed(id))
    });
}

#[test]
fn an_x11_menu_is_not_managed_and_is_in_front_of_the_window_below() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let window = fixture.open((10, 10), (50, 50), "app", "App", false);
    fixture.wait_for_events("the window", |events| opened(events).len() == 1);
    let id = opened(&fixture.seen)[0].0;
    fixture.drive();
    fixture.wait_for("the window to be placed", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 0, 1000, 600))
    });

    // A menu is mapped without asking the window manager.
    let menu = fixture.open((300, 200), (120, 80), "menu", "Menu", true);

    // The window still shows the 50x50 buffer it was drawn with, so the pointer is kept
    // inside that. It goes off the menu, onto it and off again. Over the window it asks for
    // that window's focus once. Over the menu, which belongs to no window, it asks for
    // nothing, but it has left the window: coming back asks again. So a second request
    // means the menu was in front.
    fixture.wait_for("the menu to be in front", |fixture| {
        fixture.backend.pointer_motion(20.0, 20.0, 0);
        fixture.backend.pointer_motion(350.0, 220.0, 1);
        fixture.backend.pointer_motion(20.0, 20.0, 2);
        let asks = fixture
            .seen
            .iter()
            .filter(|event| **event == Event::FocusRequested(id))
            .count();
        (asks >= 2).then_some(())
    });

    assert_eq!(
        opened(&fixture.seen).len(),
        1,
        "the menu must not be managed"
    );
    // The menu keeps the place its program gave it.
    assert_eq!(fixture.position(menu), Some((300, 200, 120, 80)));
}

#[test]
fn the_window_the_core_focuses_gets_the_x11_input_focus() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let first = fixture.open((10, 10), (50, 50), "one", "One", false);
    fixture.wait_for_events("the first window", |events| opened(events).len() == 1);
    let second = fixture.open((20, 20), (50, 50), "two", "Two", false);
    fixture.wait_for_events("the second window", |events| opened(events).len() == 2);

    // The newest window is focused by the engine.
    fixture.drive();
    fixture.wait_for("the second window to have the input focus", |fixture| {
        let focus = fixture.x.get_input_focus().ok()?.reply().ok()?.focus;
        (focus == second).then_some(())
    });
    assert_ne!(first, second);
}
