//! A test client: one connection that opens windows, reads where they are, and presses
//! keys through the XTEST extension.

use std::thread::sleep;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as _, MonitorInfo};
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt, CreateWindowAux, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, MapState,
    PropMode, Window, WindowClass,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME};

const WAIT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

/// Where a window is on the screen. The window manager does not reparent, so this is
/// the position the layout chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

impl Geometry {
    pub const fn new(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// What a new window asks for: where it would like to be and how it describes itself.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    asked: Geometry,
    class: Option<&'static str>,
    transient_for: Option<Window>,
}

impl Spec {
    /// A plain window that asks for the given geometry.
    pub const fn at(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self {
            asked: Geometry::new(x, y, width, height),
            class: None,
            transient_for: None,
        }
    }

    /// Sets the window's `WM_CLASS` class.
    pub const fn class(mut self, class: &'static str) -> Self {
        self.class = Some(class);
        self
    }

    /// Marks the window as a dialog of `parent`.
    pub const fn transient_for(mut self, parent: Window) -> Self {
        self.transient_for = Some(parent);
        self
    }
}

impl Default for Spec {
    fn default() -> Self {
        Self::at(10, 10, 50, 50)
    }
}

pub struct Client {
    conn: RustConnection,
    root: Window,
    /// First keycode of the keyboard map, and the keysyms of each keycode after it.
    first_keycode: u8,
    keysyms_per_keycode: usize,
    keysyms: Vec<u32>,
}

impl Client {
    pub fn connect(display: &str) -> Self {
        let (conn, screen) = RustConnection::connect(Some(display))
            .unwrap_or_else(|e| panic!("cannot connect to {display}: {e}"));
        let root = conn.setup().roots[screen].root;
        let first_keycode = conn.setup().min_keycode;
        let count = conn.setup().max_keycode - first_keycode + 1;
        let map = conn
            .get_keyboard_mapping(first_keycode, count)
            .expect("keyboard mapping request")
            .reply()
            .expect("keyboard mapping reply");
        Self {
            conn,
            root,
            first_keycode,
            keysyms_per_keycode: usize::from(map.keysyms_per_keycode),
            keysyms: map.keysyms,
        }
    }

    /// Creates and maps a window described by `spec`, and returns it.
    pub fn open(&self, spec: Spec) -> Window {
        let window = self.conn.generate_id().expect("window id");
        let Geometry {
            x,
            y,
            width,
            height,
        } = spec.asked;
        self.conn
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                self.root,
                x,
                y,
                width,
                height,
                0,
                WindowClass::INPUT_OUTPUT,
                COPY_FROM_PARENT,
                &CreateWindowAux::new(),
            )
            .expect("create_window");
        // Described before it is mapped: the window manager reads this at the map request.
        if let Some(class) = spec.class {
            let value = format!("{}\0{class}\0", class.to_lowercase());
            self.conn
                .change_property8(
                    PropMode::REPLACE,
                    window,
                    AtomEnum::WM_CLASS,
                    AtomEnum::STRING,
                    value.as_bytes(),
                )
                .expect("set WM_CLASS");
        }
        if let Some(parent) = spec.transient_for {
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    window,
                    AtomEnum::WM_TRANSIENT_FOR,
                    AtomEnum::WINDOW,
                    &[parent],
                )
                .expect("set WM_TRANSIENT_FOR");
        }
        self.conn.map_window(window).expect("map_window");
        self.round_trip();
        window
    }

    /// Splits the screen into a left and a right monitor of equal width, with RandR 1.5.
    /// The left one takes over the server's own outputs, so that the server's automatic
    /// monitor for them does not stay on top of the two.
    pub fn split_into_two_monitors(&self, width: u16, height: u16) {
        self.conn
            .randr_query_version(1, 5)
            .expect("randr version request")
            .reply()
            .expect("the server speaks RandR 1.5");
        let outputs = self
            .conn
            .randr_get_screen_resources_current(self.root)
            .expect("screen resources request")
            .reply()
            .expect("screen resources reply")
            .outputs;
        let half = width / 2;
        for (name, x, outputs) in [("left", 0, outputs), ("right", half, Vec::new())] {
            let name = self
                .conn
                .intern_atom(false, name.as_bytes())
                .expect("intern request")
                .reply()
                .expect("intern reply")
                .atom;
            let monitor = MonitorInfo {
                name,
                primary: false,
                automatic: false,
                x: i16::try_from(x).expect("small"),
                y: 0,
                width: half,
                height,
                width_in_millimeters: 0,
                height_in_millimeters: 0,
                outputs,
            };
            self.conn
                .randr_set_monitor(self.root, monitor)
                .expect("set monitor request");
        }
        self.round_trip();
    }

    /// Destroys a window this client created.
    pub fn destroy(&self, window: Window) {
        self.conn.destroy_window(window).expect("destroy_window");
        self.round_trip();
    }

    /// Where the window is now, or `None` if it no longer exists.
    pub fn geometry(&self, window: Window) -> Option<Geometry> {
        let reply = self.conn.get_geometry(window).ok()?.reply().ok()?;
        Some(Geometry::new(reply.x, reply.y, reply.width, reply.height))
    }

    /// Whether the window is mapped and so shown on screen.
    pub fn is_shown(&self, window: Window) -> bool {
        self.conn
            .get_window_attributes(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .is_some_and(|attributes| attributes.map_state == MapState::VIEWABLE)
    }

    /// Presses `key` with `modifiers` held, like a keyboard would, and releases them.
    pub fn press(&self, modifiers: &[u32], key: u32) {
        let codes: Vec<u8> = modifiers
            .iter()
            .chain([&key])
            .map(|keysym| {
                self.keycode(*keysym)
                    .unwrap_or_else(|| panic!("no key produces keysym {keysym:#x}"))
            })
            .collect();
        for code in &codes {
            self.fake_key(KEY_PRESS_EVENT, *code);
        }
        for code in codes.iter().rev() {
            self.fake_key(KEY_RELEASE_EVENT, *code);
        }
        self.round_trip();
    }

    /// Swaps the keys that produce `a` and `b` on the server, like a layout change would,
    /// so the window manager receives a `MappingNotify`.
    pub fn swap_keys(&mut self, a: u32, b: u32) {
        let width = self.keysyms_per_keycode;
        let row = |keysym: u32, client: &Self| {
            client
                .keycode(keysym)
                .map(|code| usize::from(code - client.first_keycode))
                .unwrap_or_else(|| panic!("no key produces keysym {keysym:#x}"))
        };
        let (first, second) = (row(a, self), row(b, self));
        for (to, from) in [(first, second), (second, first)] {
            let keysyms = self.keysyms[from * width..(from + 1) * width].to_vec();
            let code = self.first_keycode + u8::try_from(to).expect("keycode fits");
            self.conn
                .change_keyboard_mapping(1, code, u8::try_from(width).expect("width"), &keysyms)
                .expect("change_keyboard_mapping");
            self.keysyms[to * width..(to + 1) * width].copy_from_slice(&keysyms);
        }
        self.round_trip();
    }

    /// Polls `check` until it returns a value, and panics naming `what` after a timeout.
    pub fn wait_for<T>(&self, what: &str, mut check: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(found) = check() {
                return found;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            sleep(POLL);
        }
    }

    /// Waits until `window` has exactly `want`, and panics with what it had otherwise.
    pub fn expect_geometry(&self, window: Window, want: Geometry) {
        let deadline = Instant::now() + WAIT;
        loop {
            let have = self.geometry(window);
            if have == Some(want) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "window {window:#x}: wanted {want:?}, last saw {have:?}"
            );
            sleep(POLL);
        }
    }

    fn keycode(&self, keysym: u32) -> Option<u8> {
        let index = self
            .keysyms
            .chunks(self.keysyms_per_keycode)
            .position(|keysyms| keysyms.contains(&keysym))?;
        self.first_keycode.checked_add(u8::try_from(index).ok()?)
    }

    fn fake_key(&self, kind: u8, code: u8) {
        self.conn
            .xtest_fake_input(kind, code, CURRENT_TIME, self.root, 0, 0, 0)
            .expect("fake key");
    }

    /// Returns once the server has handled every request sent so far.
    fn round_trip(&self) {
        self.conn
            .get_input_focus()
            .expect("sync request")
            .reply()
            .expect("sync reply");
    }
}
