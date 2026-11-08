//! A test client: one connection that opens windows, reads where they are, and presses
//! keys through the XTEST extension.

use std::thread::sleep;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as _, MonitorInfo};
use x11rb::protocol::xproto::{
    AtomEnum, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ClientMessageEvent, ConnectionExt,
    CreateWindowAux, EventMask, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT, MapState,
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
    /// The twelve values of `_NET_WM_STRUT_PARTIAL`, for a window that is a bar.
    bar: Option<[u32; 12]>,
    fullscreen: bool,
    /// Whether the window opts out of management (override-redirect), as a menu does.
    unmanaged: bool,
}

impl Spec {
    /// A plain window that asks for the given geometry.
    pub const fn at(x: i16, y: i16, width: u16, height: u16) -> Self {
        Self {
            asked: Geometry::new(x, y, width, height),
            class: None,
            transient_for: None,
            bar: None,
            fullscreen: false,
            unmanaged: false,
        }
    }

    /// Makes the window override-redirect: the window manager is not asked about it and
    /// does not place it, as with a menu or a bar that manages itself.
    pub const fn unmanaged(mut self) -> Self {
        self.unmanaged = true;
        self
    }

    /// Makes the window a bar (type dock) reserving `depth` pixels along the top edge.
    pub const fn top_bar(mut self, depth: u32) -> Self {
        self.bar = Some(strut_partial(depth, u32::MAX));
        self
    }

    /// Makes the window a bar reserving `depth` pixels along the top edge, from pixel
    /// column `from` to `to` (inclusive).
    pub const fn top_bar_between(mut self, depth: u32, from: u32, to: u32) -> Self {
        let mut strut = strut_partial(depth, to);
        strut[8] = from;
        self.bar = Some(strut);
        self
    }

    /// Sets `_NET_WM_STATE` to fullscreen before the window maps, as a video player that
    /// starts fullscreen does.
    pub const fn fullscreen(mut self) -> Self {
        self.fullscreen = true;
        self
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

/// `_NET_WM_STRUT_PARTIAL` for a bar along the top edge up to column `end`.
const fn strut_partial(depth: u32, end: u32) -> [u32; 12] {
    [0, 0, depth, 0, 0, 0, 0, 0, 0, end, 0, 0]
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
                &CreateWindowAux::new().override_redirect(u32::from(spec.unmanaged)),
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
        if let Some(strut) = spec.bar {
            self.set_atoms(window, "_NET_WM_WINDOW_TYPE", &["_NET_WM_WINDOW_TYPE_DOCK"]);
            self.set_strut(window, strut);
        }
        if spec.fullscreen {
            self.set_atoms(window, "_NET_WM_STATE", &["_NET_WM_STATE_FULLSCREEN"]);
        }
        self.conn.map_window(window).expect("map_window");
        self.round_trip();
        window
    }

    /// Sets the window's title the modern way, as a toolkit does when the text changes.
    pub fn set_title(&self, window: Window, title: &str) {
        let atom = self.atom("_NET_WM_NAME");
        let utf8 = self.atom("UTF8_STRING");
        self.conn
            .change_property8(PropMode::REPLACE, window, atom, utf8, title.as_bytes())
            .expect("set _NET_WM_NAME");
        self.round_trip();
    }

    /// Puts the pointer at `at`, like a mouse would.
    pub fn move_pointer(&self, at: (i16, i16)) {
        self.fake_motion(at);
        self.round_trip();
    }

    /// Changes what a bar reserves, as a panel does when it resizes.
    pub fn set_strut(&self, window: Window, strut: [u32; 12]) {
        self.conn
            .change_property32(
                PropMode::REPLACE,
                window,
                self.atom("_NET_WM_STRUT_PARTIAL"),
                AtomEnum::CARDINAL,
                &strut,
            )
            .expect("set _NET_WM_STRUT_PARTIAL");
        self.round_trip();
    }

    /// Asks the window manager, as a client does, to add (1), remove (0) or toggle (2) the
    /// fullscreen state of `window`.
    pub fn request_fullscreen(&self, window: Window, action: u32) {
        let message = ClientMessageEvent::new(
            32,
            window,
            self.atom("_NET_WM_STATE"),
            [action, self.atom("_NET_WM_STATE_FULLSCREEN"), 0, 1, 0],
        );
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        self.conn
            .send_event(false, self.root, mask, message)
            .expect("send _NET_WM_STATE");
        self.round_trip();
    }

    /// Whether the window's `_NET_WM_STATE` says fullscreen.
    pub fn is_fullscreen(&self, window: Window) -> bool {
        self.atoms_of(window, "_NET_WM_STATE")
            .contains(&self.atom("_NET_WM_STATE_FULLSCREEN"))
    }

    /// The names of the EWMH hints the window manager lists as supported.
    pub fn supported(&self, name: &str) -> bool {
        self.atoms_of(self.root, "_NET_SUPPORTED")
            .contains(&self.atom(name))
    }

    fn atom(&self, name: &str) -> u32 {
        self.conn
            .intern_atom(false, name.as_bytes())
            .expect("intern request")
            .reply()
            .expect("intern reply")
            .atom
    }

    fn set_atoms(&self, window: Window, property: &str, values: &[&str]) {
        let values: Vec<u32> = values.iter().map(|name| self.atom(name)).collect();
        self.conn
            .change_property32(
                PropMode::REPLACE,
                window,
                self.atom(property),
                AtomEnum::ATOM,
                &values,
            )
            .expect("set an atom property");
    }

    fn atoms_of(&self, window: Window, property: &str) -> Vec<u32> {
        self.conn
            .get_property(false, window, self.atom(property), AtomEnum::ATOM, 0, 64)
            .expect("get_property request")
            .reply()
            .ok()
            .and_then(|reply| reply.value32().map(Iterator::collect))
            .unwrap_or_default()
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

    /// Drags the pointer from `from` to `to` with `button` and `modifiers` held, like a
    /// mouse would: the pointer is placed, the keys and the button go down, the pointer
    /// moves in a few steps, then the button and the keys come up.
    pub fn drag(&self, modifiers: &[u32], button: u8, from: (i16, i16), to: (i16, i16)) {
        const STEPS: i16 = 4;
        let codes: Vec<u8> = modifiers
            .iter()
            .map(|keysym| {
                self.keycode(*keysym)
                    .unwrap_or_else(|| panic!("no key produces keysym {keysym:#x}"))
            })
            .collect();
        self.fake_motion(from);
        for code in &codes {
            self.fake_key(KEY_PRESS_EVENT, *code);
        }
        self.fake_button(BUTTON_PRESS_EVENT, button);
        for step in 1..=STEPS {
            self.fake_motion((
                from.0 + (to.0 - from.0) * step / STEPS,
                from.1 + (to.1 - from.1) * step / STEPS,
            ));
        }
        self.fake_button(BUTTON_RELEASE_EVENT, button);
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
        // Both rows are copied before either is written, or the second copy would read
        // the first one's new contents and both keys would end up producing the same.
        let rows = [first, second].map(|row| self.keysyms[row * width..(row + 1) * width].to_vec());
        for (to, keysyms) in [(first, &rows[1]), (second, &rows[0])] {
            let code = self.first_keycode + u8::try_from(to).expect("keycode fits");
            self.conn
                .change_keyboard_mapping(1, code, u8::try_from(width).expect("width"), keysyms)
                .expect("change_keyboard_mapping");
            self.keysyms[to * width..(to + 1) * width].copy_from_slice(keysyms);
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

    fn fake_button(&self, kind: u8, button: u8) {
        self.conn
            .xtest_fake_input(kind, button, CURRENT_TIME, self.root, 0, 0, 0)
            .expect("fake button");
    }

    /// Puts the pointer at `at`, in root coordinates.
    fn fake_motion(&self, at: (i16, i16)) {
        self.conn
            .xtest_fake_input(
                MOTION_NOTIFY_EVENT,
                0,
                CURRENT_TIME,
                self.root,
                at.0,
                at.1,
                0,
            )
            .expect("fake motion");
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
