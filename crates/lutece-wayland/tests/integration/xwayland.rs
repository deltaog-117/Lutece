//! X11 programs: a real `Xwayland` runs under the backend and the tests act as an X11
//! client of it.
//!
//! They need the `Xwayland` program. Without it each test prints a notice and passes;
//! set `LUTECE_REQUIRE_X11=1` (as CI should) to make a missing program a failure.

use crate::support::{TestClient, backend, engine};
use lutece::{Engine, Event, Insets, OutputId, WinId};
use lutece_wayland::WaylandBackend;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeGCAux, ClientMessageEvent, ConnectionExt, CreateGCAux, CreateWindowAux,
    EventMask, Rectangle, SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent,
    Window, WindowClass,
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

    /// Records events the backend reported while a test stepped it some other way.
    fn absorb(&mut self, events: Vec<Event>) {
        self.seen.extend(events.iter().cloned());
        self.waiting.extend(events);
    }

    /// Lets `client` and the backend react to each other, and records what the backend says.
    fn pump(&mut self, client: &mut TestClient) {
        let events = client.settle(&mut self.backend);
        self.absorb(events);
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
        self.create_and_map(window, at, size, title, class, menu);
        window
    }

    fn intern(&self, name: &str) -> u32 {
        self.x
            .intern_atom(false, name.as_bytes())
            .expect("intern request")
            .reply()
            .expect("intern reply")
            .atom
    }

    /// Asks the window manager, as an X11 program does, to add (1), remove (0) or toggle (2)
    /// the fullscreen state of `window`.
    fn request_fullscreen(&self, window: Window, action: u32) {
        let message = ClientMessageEvent::new(
            32,
            window,
            self.intern("_NET_WM_STATE"),
            [action, self.intern("_NET_WM_STATE_FULLSCREEN"), 0, 1, 0],
        );
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        self.x
            .send_event(false, self.root, mask, message)
            .expect("send _NET_WM_STATE");
        self.x.flush().expect("flush");
    }

    /// Like [`open`](Self::open) for a window that sets the fullscreen state before it maps,
    /// as a video player started in fullscreen does.
    fn open_fullscreen(&self, size: (u16, u16), title: &str, class: &str) -> Window {
        let window = self.x.generate_id().expect("window id");
        self.create(window, (10, 10), size, title, class, false);
        let state = [self.intern("_NET_WM_STATE_FULLSCREEN")];
        self.x
            .change_property32(
                x11rb::protocol::xproto::PropMode::REPLACE,
                window,
                self.intern("_NET_WM_STATE"),
                AtomEnum::ATOM,
                &state,
            )
            .expect("_NET_WM_STATE");
        self.map_and_draw(window, size);
        window
    }

    fn create_and_map(
        &self,
        window: Window,
        at: (i16, i16),
        size: (u16, u16),
        title: &str,
        class: &str,
        menu: bool,
    ) {
        self.create(window, at, size, title, class, menu);
        self.map_and_draw(window, size);
    }

    fn create(
        &self,
        window: Window,
        at: (i16, i16),
        size: (u16, u16),
        title: &str,
        class: &str,
        menu: bool,
    ) {
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
    }

    fn map_and_draw(&self, window: Window, size: (u16, u16)) {
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

/// A 30-pixel band along the top that windows are not tiled into, as a bar would leave.
fn reserve_a_top_band(fixture: &mut Fixture) {
    fixture.waiting.push(Event::ReservedChanged {
        output: OutputId::new(0),
        insets: Insets::new(0, 30, 0, 0),
    });
}

#[test]
fn an_x11_program_can_ask_to_be_fullscreen_and_to_stop() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    reserve_a_top_band(&mut fixture);
    let window = fixture.open((10, 10), (50, 50), "video", "Player", false);
    fixture.wait_for_events("the window", |events| !opened(events).is_empty());
    fixture.drive();
    fixture.wait_for("the window below the band", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 30, 1000, 570))
    });

    fixture.request_fullscreen(window, 1);
    fixture.wait_for_events("the request", |events| {
        events
            .iter()
            .any(|event| matches!(event, Event::FullscreenRequested { on: true, .. }))
    });
    fixture.drive();
    fixture.wait_for("the window to cover the band", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 0, 1000, 600))
    });

    fixture.request_fullscreen(window, 0);
    fixture.wait_for_events("the request to stop", |events| {
        events
            .iter()
            .any(|event| matches!(event, Event::FullscreenRequested { on: false, .. }))
    });
    fixture.drive();
    fixture.wait_for("the window back below the band", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 30, 1000, 570))
    });
}

#[test]
fn an_x11_program_that_starts_fullscreen_is_seen_asking() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    reserve_a_top_band(&mut fixture);
    let window = fixture.open_fullscreen((50, 50), "video", "Player");

    // The request comes after the window opens, so the core knows the window by then.
    fixture.wait_for_events("the window and its request", |events| {
        let opened_at = events
            .iter()
            .position(|event| matches!(event, Event::WindowOpened { .. }));
        let asked_at = events
            .iter()
            .position(|event| matches!(event, Event::FullscreenRequested { on: true, .. }));
        matches!((opened_at, asked_at), (Some(opened), Some(asked)) if opened < asked)
    });

    fixture.drive();
    fixture.wait_for("the window to cover the band", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 0, 1000, 600))
    });
}

#[test]
fn an_x11_program_that_does_not_ask_stays_tiled_below_the_band() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    reserve_a_top_band(&mut fixture);
    let window = fixture.open((10, 10), (50, 50), "plain", "Plain", false);
    fixture.wait_for_events("the window", |events| !opened(events).is_empty());
    fixture.drive();
    fixture.wait_for("the window below the band", |fixture| {
        fixture
            .position(window)
            .filter(|at| *at == (0, 30, 1000, 570))
    });

    assert!(
        !fixture
            .seen
            .iter()
            .any(|event| matches!(event, Event::FullscreenRequested { .. })),
        "no request was made"
    );
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
            .filter(|event| **event == Event::PointerEntered(id))
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

impl Fixture {
    /// A window-list property of the root (`_NET_ACTIVE_WINDOW`, `_NET_CLIENT_LIST_STACKING`).
    fn root_windows(&self, name: &str) -> Vec<Window> {
        let atom = self.intern(name);
        self.x
            .get_property(false, self.root, atom, AtomEnum::WINDOW, 0, 64)
            .expect("get_property")
            .reply()
            .expect("a reply")
            .value32()
            .map(Iterator::collect)
            .unwrap_or_default()
    }

    /// The window `_NET_ACTIVE_WINDOW` names; 0 when it names none.
    fn active_window(&self) -> Window {
        self.root_windows("_NET_ACTIVE_WINDOW")
            .first()
            .copied()
            .unwrap_or(0)
    }
}

#[test]
fn the_active_window_hint_follows_the_focus_and_clears_when_a_wayland_window_has_it() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let window = fixture.open((10, 10), (50, 50), "x11", "X11", false);
    fixture.wait_for_events("the X11 window", |events| opened(events).len() == 1);
    let x_id = opened(&fixture.seen)[0].0;
    let mut client = fixture.wayland_client();
    fixture.wait_for_events("both windows", |events| opened(events).len() == 2);
    let wayland_id = opened(&fixture.seen)[1].0;

    fixture.focus(x_id, &mut client);
    fixture.wait_for("the hint to name the X11 window", |fixture| {
        (fixture.active_window() == window).then_some(())
    });

    fixture.focus(wayland_id, &mut client);
    fixture.wait_for("the hint to be cleared", |fixture| {
        (fixture.active_window() == 0).then_some(())
    });
    // Smithay writes the hint when the X server reports the focus change, which can be
    // after the first reading; the hint must stay cleared.
    for _ in 0..20 {
        fixture.pump(&mut client);
        assert_eq!(fixture.active_window(), 0, "the old window came back");
    }
}

#[test]
fn the_x_stacking_order_follows_the_scene() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let first = fixture.open((10, 10), (50, 50), "one", "One", false);
    fixture.wait_for_events("the first window", |events| opened(events).len() == 1);
    let second = fixture.open((20, 20), (50, 50), "two", "Two", false);
    fixture.wait_for_events("the second window", |events| opened(events).len() == 2);
    let ids = [opened(&fixture.seen)[0].0, opened(&fixture.seen)[1].0];
    fixture.drive();
    // Tiles do not overlap, so which is lower is the scene's business; X must agree.
    let listed = fixture.wait_for("both windows to be listed", |fixture| {
        let list = fixture.root_windows("_NET_CLIENT_LIST_STACKING");
        (list.len() == 2).then_some(list)
    });
    let (bottom, top) = (listed[0], listed[1]);
    let bottom_id = if bottom == first { ids[0] } else { ids[1] };
    assert!([first, second].contains(&top) && top != bottom);

    // A fullscreen window is drawn above everything, so it is above in X too.
    fixture.waiting.push(Event::FocusRequested(bottom_id));
    fixture.waiting.push(Event::FullscreenRequested {
        id: bottom_id,
        on: true,
    });
    fixture.drive();
    fixture.wait_for("the fullscreen window on top", |fixture| {
        (fixture.root_windows("_NET_CLIENT_LIST_STACKING") == [top, bottom]).then_some(())
    });

    // And back under, with the other one on top again, when it stops.
    fixture.waiting.push(Event::FullscreenRequested {
        id: bottom_id,
        on: false,
    });
    fixture.drive();
    fixture.wait_for("the order to return", |fixture| {
        (fixture.root_windows("_NET_CLIENT_LIST_STACKING") == [bottom, top]).then_some(())
    });
}

/// The atoms the selection tests talk in.
struct Atoms {
    clipboard: u32,
    text: u32,
    targets: u32,
    /// Where a paste is delivered.
    property: u32,
}

impl Fixture {
    fn atoms(&self) -> Atoms {
        let atom = |name: &[u8]| {
            self.x
                .intern_atom(false, name)
                .expect("intern_atom")
                .reply()
                .expect("an atom")
                .atom
        };
        Atoms {
            clipboard: atom(b"CLIPBOARD"),
            text: atom(b"TEXT"),
            targets: atom(b"TARGETS"),
            property: atom(b"LUTECE_PASTE"),
        }
    }

    /// A Wayland client with a shown window, opened after the windows already there.
    fn wayland_client(&mut self) -> TestClient {
        let (mut client, events) = TestClient::connect(&mut self.backend);
        self.absorb(events);
        let window = client.open_window("wayland", "Wayland");
        self.pump(&mut client);
        client.map_window(window, 100, 100);
        self.pump(&mut client);
        client
    }

    /// Makes the engine focus `id`, and lets the client hear of it.
    fn focus(&mut self, id: WinId, client: &mut TestClient) {
        self.waiting.push(Event::FocusRequested(id));
        self.drive();
        self.pump(client);
    }

    /// Answers the requests X11 programs make of the selection `owner` holds, which is
    /// what a program that copied must do for others to paste.
    fn serve_selection(&self, atoms: &Atoms, payload: &[u8]) {
        while let Some(event) = self.x.poll_for_event().expect("X events") {
            let XEvent::SelectionRequest(request) = event else {
                continue;
            };
            self.answer(atoms, &request, payload);
        }
    }

    fn answer(&self, atoms: &Atoms, request: &SelectionRequestEvent, payload: &[u8]) {
        let mut property = request.property;
        if request.target == atoms.targets {
            self.x
                .change_property32(
                    x11rb::protocol::xproto::PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &[atoms.targets, atoms.text],
                )
                .expect("targets");
        } else if request.target == atoms.text {
            self.x
                .change_property8(
                    x11rb::protocol::xproto::PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::STRING,
                    payload,
                )
                .expect("text");
        } else {
            property = x11rb::NONE;
        }
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property,
        };
        self.x
            .send_event(false, request.requestor, EventMask::NO_EVENT, notify)
            .expect("send_event");
        self.x.flush().expect("flush");
    }

    /// Asks for the clipboard as text and waits for the answer: the property it was put
    /// in, or `NONE` if the request was refused.
    fn request_clipboard(&mut self, window: Window, atoms: &Atoms, client: &mut TestClient) -> u32 {
        // Asking before the bridge has taken the selection would be refused by the X server
        // itself, which says nothing about the access rule under test.
        self.wait_for("the clipboard to have an owner", |fixture| {
            fixture.pump(client);
            let owner = fixture
                .x
                .get_selection_owner(atoms.clipboard)
                .ok()?
                .reply()
                .ok()?
                .owner;
            (owner != x11rb::NONE).then_some(())
        });
        self.x
            .convert_selection(
                window,
                atoms.clipboard,
                atoms.text,
                atoms.property,
                x11rb::CURRENT_TIME,
            )
            .expect("convert_selection");
        self.x.flush().expect("flush");
        self.wait_for("the answer to the paste", |fixture| {
            fixture.pump(client);
            match fixture.x.poll_for_event().expect("X events") {
                Some(XEvent::SelectionNotify(notify)) => Some(notify.property),
                _ => None,
            }
        })
    }
}

#[test]
fn an_x11_program_can_paste_what_a_wayland_client_copied() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let window = fixture.open((10, 10), (50, 50), "x11", "X11", false);
    fixture.wait_for_events("the X11 window", |events| opened(events).len() == 1);
    let x_id = opened(&fixture.seen)[0].0;
    let mut client = fixture.wayland_client();
    fixture.wait_for_events("both windows", |events| opened(events).len() == 2);
    let atoms = fixture.atoms();

    // The Wayland window is the newest, so it has the focus and may copy.
    fixture.drive();
    fixture.pump(&mut client);
    client.copy(b"from a wayland client");
    fixture.pump(&mut client);
    fixture.focus(x_id, &mut client);
    fixture.wait_for("the X11 window to have the input focus", |fixture| {
        let focus = fixture.x.get_input_focus().ok()?.reply().ok()?.focus;
        (focus == window).then_some(())
    });

    let property = fixture.request_clipboard(window, &atoms, &mut client);

    assert_eq!(property, atoms.property, "the paste was refused");
    let pasted = fixture
        .x
        .get_property(true, window, property, AtomEnum::ANY, 0, 64)
        .expect("get_property")
        .reply()
        .expect("a reply");
    assert_eq!(pasted.value, b"from a wayland client");
}

#[test]
fn an_x11_program_without_focus_cannot_read_the_clipboard() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let window = fixture.open((10, 10), (50, 50), "x11", "X11", false);
    fixture.wait_for_events("the X11 window", |events| opened(events).len() == 1);
    let x_id = opened(&fixture.seen)[0].0;
    let mut client = fixture.wayland_client();
    fixture.wait_for_events("both windows", |events| opened(events).len() == 2);
    let wayland_id = opened(&fixture.seen)[1].0;
    let atoms = fixture.atoms();

    fixture.drive();
    fixture.pump(&mut client);
    client.copy(b"private");
    fixture.pump(&mut client);
    // The X11 window has the focus for a moment, which is also what makes Xwayland register
    // the copied text, and then gives it back: the program is in the background.
    fixture.focus(x_id, &mut client);
    fixture.wait_for("the X11 window to have the input focus", |fixture| {
        let focus = fixture.x.get_input_focus().ok()?.reply().ok()?.focus;
        (focus == window).then_some(())
    });
    fixture.focus(wayland_id, &mut client);
    fixture.wait_for("the X11 window to lose the input focus", |fixture| {
        let focus = fixture.x.get_input_focus().ok()?.reply().ok()?.focus;
        (focus != window).then_some(())
    });

    let property = fixture.request_clipboard(window, &atoms, &mut client);

    assert_eq!(
        property,
        x11rb::NONE,
        "a background program read the clipboard"
    );
}

#[test]
fn a_wayland_client_can_paste_what_an_x11_program_copied() {
    let Some(mut fixture) = Fixture::start() else {
        return;
    };
    let mut client = fixture.wayland_client();
    fixture.wait_for_events("the Wayland window", |events| opened(events).len() == 1);
    let atoms = fixture.atoms();
    // The program that copies needs no window on the screen, only one to own the selection.
    let owner = fixture.x.generate_id().expect("window id");
    fixture
        .x
        .create_window(
            COPY_DEPTH_FROM_PARENT,
            owner,
            fixture.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            COPY_FROM_PARENT,
            &CreateWindowAux::new(),
        )
        .expect("create_window");
    fixture.drive();
    fixture.pump(&mut client);

    fixture
        .x
        .set_selection_owner(owner, atoms.clipboard, x11rb::CURRENT_TIME)
        .expect("set_selection_owner");
    fixture.x.flush().expect("flush");
    fixture.wait_for("the Wayland client to be offered the text", |fixture| {
        fixture.serve_selection(&atoms, b"from an x11 program");
        fixture.pump(&mut client);
        client.offered().map(<[String]>::to_vec)
    });
    assert_eq!(
        client.offered().map(<[String]>::to_vec),
        Some(vec!["text/plain".to_owned()])
    );

    let mut reader = client.paste();
    let (sender, receiver) = mpsc::channel();
    // The read ends only when the writer is closed, which needs the backend to keep running.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        let _ = sender.send(text);
    });
    let pasted = fixture.wait_for("the paste to arrive", |fixture| {
        fixture.serve_selection(&atoms, b"from an x11 program");
        fixture.pump(&mut client);
        receiver.try_recv().ok()
    });
    assert_eq!(pasted, "from an x11 program");
}
