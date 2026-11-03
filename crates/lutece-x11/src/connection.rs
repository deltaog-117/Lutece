//! The connection to the X server, and the backend built on it.

use crate::apply::{Flow, execute, plan, run_effects};
use crate::docks::{Reserve, Strut};
use crate::drag::Drags;
use crate::error::{Error, Result};
use crate::grabs::{Keyboard, grab_drag_buttons, grab_keys, regrab_keys, release_drag_buttons};
use crate::outputs;
use crate::translate::{existing, fullscreen_request, key_press, translate};
use crate::windows::Windows;
use lutece::{Effect, Event, KeyPress, Modifiers, Scene, WindowInfo, diff};
use std::mem;
use std::os::fd::{AsFd, BorrowedFd};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::ErrorKind;
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, CreateWindowAux,
    EventMask, MapState, Mapping, PropMode, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT};

x11rb::atom_manager! {
    pub(crate) Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        UTF8_STRING,
        _NET_WM_NAME,
        _NET_SUPPORTED,
        _NET_SUPPORTING_WM_CHECK,
        _NET_WM_STATE,
        _NET_WM_STATE_FULLSCREEN,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_STRUT,
        _NET_WM_STRUT_PARTIAL,
    }
}

/// How much of a text property to fetch, in 4-byte units. Longer titles are cut.
const TEXT_PROPERTY_LEN: u32 = 256;

/// A window manager's connection to an X server.
///
/// The caller owns the loop: wait for [`next_events`](Self::next_events), feed each event
/// to the engine, then show the engine's scene with [`apply_scene`](Self::apply_scene)
/// and carry out its effects with [`apply_effects`](Self::apply_effects). To wait on
/// other inputs too, poll the descriptor from [`AsFd`] and call
/// [`pending_events`](Self::pending_events) when it is readable.
pub struct X11Backend {
    conn: RustConnection,
    root: Window,
    /// Whether the server lists monitors (RandR 1.5), to tell what to ask on a change.
    monitors: bool,
    atoms: Atoms,
    keyboard: Keyboard,
    /// The key combinations grabbed so far, to grab again after a keyboard remap.
    grabbed: Vec<KeyPress>,
    windows: Windows,
    /// The bars, and what the core has been told they reserve.
    reserve: Reserve,
    drags: Drags,
    /// What the X server is showing, so a new scene is applied as a difference.
    shown: Scene,
    /// Events produced before the first X event, such as the monitors.
    queued: Vec<Event>,
}

impl X11Backend {
    /// Connects to the display named by `DISPLAY` and becomes its window manager.
    ///
    /// Windows already on screen are taken over. The first call to
    /// [`next_events`](Self::next_events) reports the monitors, then those windows, bottom
    /// of the stack first. Each monitor is an output, and a change to them (a monitor
    /// plugged in, removed or moved) is reported as a new set of outputs. A server
    /// without RandR 1.5 has one output, the whole screen.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AnotherWindowManager`] if one is already running, or an error
    /// if the display cannot be reached.
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) = RustConnection::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let atoms = Atoms::new(&conn)?.reply()?;
        let keyboard = Keyboard::fetch(&conn)?;

        let redirect = ChangeWindowAttributesAux::new()
            .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY);
        match conn.change_window_attributes(root, &redirect)?.check() {
            Err(ReplyError::X11Error(e)) if e.error_kind == ErrorKind::Access => {
                return Err(Error::AnotherWindowManager);
            }
            other => other?,
        }

        let mut windows = Windows::default();
        let mut reserve = Reserve::default();
        let monitors = outputs::watch(&conn, root)?;
        let reported = outputs::query(&conn, root, monitors)?;
        reserve.set_outputs(&reported);
        let mut queued = vec![Event::OutputsChanged(reported)];
        // Asked after the redirect is in place: a window that maps from now on sends a
        // map request, and one that mapped earlier is in this list, so none is missed.
        let opened = existing_windows(&conn, &atoms, root, &mut windows, &mut reserve)?;
        // The bars come before the windows, so the first scene already leaves them room.
        queued.extend(reserve.changes(screen_size(&conn, root)?));
        queued.extend(opened);
        advertise(&conn, &atoms, root)?;

        Ok(Self {
            conn,
            root,
            monitors,
            atoms,
            keyboard,
            grabbed: Vec::new(),
            windows,
            reserve,
            drags: Drags::default(),
            shown: Scene::empty(),
            queued,
        })
    }

    /// Waits until at least one event for the core is available, and returns all that
    /// are ready.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection to the X server breaks.
    pub fn next_events(&mut self) -> Result<Vec<Event>> {
        while self.queued.is_empty() {
            let event = self.conn.wait_for_event()?;
            self.translate_into_queue(&event)?;
        }
        self.pending_events()
    }

    /// Returns the events that are ready now, without waiting. The list may be empty.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection to the X server breaks.
    pub fn pending_events(&mut self) -> Result<Vec<Event>> {
        while let Some(event) = self.conn.poll_for_event()? {
            self.translate_into_queue(&event)?;
        }
        self.conn.flush()?;
        Ok(mem::take(&mut self.queued))
    }

    /// Grabs `presses` on the whole screen, so each press reaches the core as an
    /// [`Event::KeyPressed`] whichever window has the focus. Pass it the engine's
    /// `grabs()`. CapsLock and NumLock do not stop a press from matching.
    ///
    /// A key is named by what it produces without Shift: grab Super+Shift+q as the
    /// keysym of `q` with the Shift modifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnmappedKeysym`] if no key on the keyboard produces a keysym,
    /// [`Error::KeyAlreadyGrabbed`] if another client holds a combination, or an error
    /// if the connection to the X server breaks.
    pub fn grab_keys(&mut self, presses: impl IntoIterator<Item = KeyPress>) -> Result<()> {
        let presses: Vec<KeyPress> = presses.into_iter().collect();
        grab_keys(
            &self.conn,
            &self.keyboard,
            self.root,
            presses.iter().copied(),
        )?;
        // Kept so the same keys can be grabbed again when the keyboard is remapped.
        for press in presses {
            if !self.grabbed.contains(&press) {
                self.grabbed.push(press);
            }
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Sets the modifier that turns a press of the left or right button into a drag of the
    /// window under the pointer, reported as [`Event::FloatAdjusted`]: the left button
    /// moves it and the right button resizes it, and the window's client sees none of it.
    /// The core ignores the drag unless the window floats. [`Modifiers::NONE`], which is
    /// the state until this is called, leaves the buttons to the clients.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DragAlreadyGrabbed`] if another client holds the buttons with this
    /// modifier, or an error if the connection to the X server breaks.
    pub fn set_drag_modifier(&mut self, modifier: Modifiers) -> Result<()> {
        release_drag_buttons(&self.conn, self.root)?;
        if modifier != Modifiers::NONE {
            grab_drag_buttons(&self.conn, self.root, modifier)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Makes the screen show `scene`, sending only what differs from what is shown.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ForeignWindow`] if the scene names a window this backend never
    /// reported, or an error if the connection to the X server breaks.
    pub fn apply_scene(&mut self, scene: &Scene) -> Result<()> {
        let ops = plan(&mut self.windows, &diff(&self.shown, scene))?;
        execute(&self.conn, &self.atoms, self.root, &ops)?;
        self.shown = scene.clone();
        Ok(())
    }

    /// Carries out `effects` in order. Stops at the first [`Effect::Quit`] and returns
    /// [`Flow::Quit`] for it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Spawn`] if a program cannot be started, or an error if the
    /// connection to the X server breaks.
    ///
    /// [`Effect::Release`] shows the window and stops reporting it, so its client sizes it.
    pub fn apply_effects(&mut self, effects: &[Effect]) -> Result<Flow> {
        run_effects(&self.conn, &self.atoms, &mut self.windows, effects)
    }

    fn translate_into_queue(&mut self, event: &XEvent) -> Result<()> {
        let Self {
            conn,
            root,
            monitors,
            atoms,
            keyboard,
            grabbed,
            windows,
            reserve,
            drags,
            queued,
            ..
        } = self;

        if let XEvent::KeyPress(press) = event {
            queued.extend(key_press(keyboard, press));
            return Ok(());
        }

        // These arrive only through the drag button grab, which is why they all mean a drag.
        match event {
            XEvent::ButtonPress(press) => {
                let at = (press.root_x, press.root_y);
                drags.press(press.detail, press.child, at, windows);
                return Ok(());
            }
            XEvent::MotionNotify(motion) => {
                queued.extend(drags.motion((motion.root_x, motion.root_y)));
                return Ok(());
            }
            XEvent::ButtonRelease(release) => {
                drags.release(release.detail);
                return Ok(());
            }
            _ => {}
        }

        if let XEvent::MappingNotify(mapping) = event {
            // Only a keycode table change matters; modifier and pointer remaps leave the
            // grabs valid, since CapsLock and NumLock are always grabbed on Lock and Mod2.
            if mapping.request == Mapping::KEYBOARD {
                *keyboard = Keyboard::fetch(conn)?;
                regrab_keys(conn, keyboard, *root, grabbed)?;
            }
            return Ok(());
        }

        if matches!(
            event,
            XEvent::RandrScreenChangeNotify(_) | XEvent::RandrNotify(_)
        ) {
            // The core takes the complete new set, so a repeated report is harmless.
            let reported = outputs::query(conn, *root, *monitors)?;
            reserve.set_outputs(&reported);
            queued.push(Event::OutputsChanged(reported));
            queued.extend(reserve.changes(screen_size(conn, *root)?));
            return Ok(());
        }

        if let XEvent::ClientMessage(message) = event {
            queued.extend(fullscreen_request(
                windows,
                atoms._NET_WM_STATE,
                atoms._NET_WM_STATE_FULLSCREEN,
                message,
            ));
            return Ok(());
        }

        // Bars are not managed: they are shown where they ask, and only what they reserve
        // reaches the core.
        if let XEvent::MapRequest(request) = event
            && (reserve.is_dock(request.window) || is_dock(conn, atoms, request.window))
        {
            track_dock(conn, atoms, reserve, request.window)?;
            conn.map_window(request.window)?;
            queued.extend(reserve.changes(screen_size(conn, *root)?));
            return Ok(());
        }
        let gone = match event {
            XEvent::UnmapNotify(unmap) => Some(unmap.window),
            XEvent::DestroyNotify(destroy) => Some(destroy.window),
            _ => None,
        };
        if gone.is_some_and(|window| reserve.remove_dock(window)) {
            queued.extend(reserve.changes(screen_size(conn, *root)?));
            return Ok(());
        }
        if let XEvent::PropertyNotify(change) = event {
            let strut_changed =
                change.atom == atoms._NET_WM_STRUT || change.atom == atoms._NET_WM_STRUT_PARTIAL;
            if strut_changed && reserve.is_dock(change.window) {
                track_dock(conn, atoms, reserve, change.window)?;
                queued.extend(reserve.changes(screen_size(conn, *root)?));
            }
            return Ok(());
        }

        if let XEvent::ConfigureRequest(request) = event {
            // A managed window is sized by the layout, so its own request is refused. A
            // window we do not manage yet gets what it asked for.
            if !windows.is_managed(request.window) {
                conn.configure_window(
                    request.window,
                    &ConfigureWindowAux::from_configure_request(request),
                )?;
            }
            return Ok(());
        }

        let opened = translate(windows, event, |window| adopt(conn, atoms, window));
        // A window may ask to start fullscreen by setting its state before it maps.
        let fullscreen = opened
            .as_ref()
            .and_then(|event| starts_fullscreen(conn, atoms, event));
        queued.extend(opened);
        queued.extend(fullscreen);
        Ok(())
    }
}

impl AsFd for X11Backend {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.conn.stream().as_fd()
    }
}

/// Opens the windows that were already showing, bottom of the stack first.
fn existing_windows(
    conn: &RustConnection,
    atoms: &Atoms,
    root: Window,
    windows: &mut Windows,
    reserve: &mut Reserve,
) -> Result<Vec<Event>> {
    let children = conn.query_tree(root)?.reply()?.children;
    // Ask about all of them before reading any answer, to save a round trip each.
    let cookies = children
        .iter()
        .map(|child| conn.get_window_attributes(*child))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut events = Vec::new();
    for (child, cookie) in children.into_iter().zip(cookies) {
        // A window destroyed since the tree was read has no attributes left to adopt.
        let Ok(attributes) = cookie.reply() else {
            continue;
        };
        // A bar that was already showing is tracked, not managed.
        let showing = attributes.map_state == MapState::VIEWABLE && !attributes.override_redirect;
        if showing && is_dock(conn, atoms, child) {
            track_dock(conn, atoms, reserve, child)?;
            continue;
        }
        let opened = existing(
            windows,
            child,
            attributes.map_state,
            attributes.override_redirect,
            |window| adopt(conn, atoms, window),
        );
        let fullscreen = opened
            .as_ref()
            .and_then(|event| starts_fullscreen(conn, atoms, event));
        events.extend(opened);
        events.extend(fullscreen);
    }
    Ok(events)
}

/// The size of the whole X screen, which bars measure their struts from.
fn screen_size(conn: &RustConnection, root: Window) -> Result<(u32, u32)> {
    let geometry = conn.get_geometry(root)?.reply()?;
    Ok((u32::from(geometry.width), u32::from(geometry.height)))
}

/// The values of a property of 32-bit numbers, or none if it is not set or the window is
/// gone.
fn cardinals(conn: &RustConnection, window: Window, property: u32, kind: AtomEnum) -> Vec<u32> {
    conn.get_property(false, window, property, kind, 0, 64)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(Iterator::collect))
        .unwrap_or_default()
}

/// Whether `window` says it is a bar (`_NET_WM_WINDOW_TYPE_DOCK`).
fn is_dock(conn: &RustConnection, atoms: &Atoms, window: Window) -> bool {
    cardinals(conn, window, atoms._NET_WM_WINDOW_TYPE, AtomEnum::ATOM)
        .contains(&atoms._NET_WM_WINDOW_TYPE_DOCK)
}

/// Starts tracking a bar, or reads again what it reserves, and listens for it to change.
fn track_dock(
    conn: &RustConnection,
    atoms: &Atoms,
    reserve: &mut Reserve,
    window: Window,
) -> Result<()> {
    // The window can be destroyed while we ask; its destruction then arrives as an event.
    let _ = conn.change_window_attributes(
        window,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    );
    let partial = cardinals(
        conn,
        window,
        atoms._NET_WM_STRUT_PARTIAL,
        AtomEnum::CARDINAL,
    );
    let plain = cardinals(conn, window, atoms._NET_WM_STRUT, AtomEnum::CARDINAL);
    reserve.set_dock(window, Strut::from_properties(&partial, &plain));
    Ok(())
}

/// The request to make a window fullscreen, if it was just opened with that state set.
fn starts_fullscreen(conn: &RustConnection, atoms: &Atoms, opened: &Event) -> Option<Event> {
    let Event::WindowOpened { id, .. } = opened else {
        return None;
    };
    let window = Window::try_from(id.get()).ok()?;
    cardinals(conn, window, atoms._NET_WM_STATE, AtomEnum::ATOM)
        .contains(&atoms._NET_WM_STATE_FULLSCREEN)
        .then_some(Event::FullscreenRequested { id: *id, on: true })
}

/// Tells clients which parts of EWMH this window manager honours. Toolkits decide from
/// this list whether to ask for fullscreen with `_NET_WM_STATE` or to resize the window
/// themselves, and they look for the check window to know a window manager is there.
fn advertise(conn: &RustConnection, atoms: &Atoms, root: Window) -> Result<()> {
    let check = conn.generate_id()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        check,
        root,
        -1,
        -1,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        COPY_FROM_PARENT,
        &CreateWindowAux::new().override_redirect(1),
    )?;
    for window in [root, check] {
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_SUPPORTING_WM_CHECK,
            AtomEnum::WINDOW,
            &[check],
        )?;
    }
    conn.change_property8(
        PropMode::REPLACE,
        check,
        atoms._NET_WM_NAME,
        atoms.UTF8_STRING,
        b"lutece",
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        root,
        atoms._NET_SUPPORTED,
        AtomEnum::ATOM,
        &[
            atoms._NET_SUPPORTING_WM_CHECK,
            atoms._NET_WM_STATE,
            atoms._NET_WM_STATE_FULLSCREEN,
            atoms._NET_WM_WINDOW_TYPE,
            atoms._NET_WM_WINDOW_TYPE_DOCK,
            atoms._NET_WM_STRUT,
            atoms._NET_WM_STRUT_PARTIAL,
        ],
    )?;
    conn.flush()?;
    Ok(())
}

/// Subscribes to a new window's pointer events and describes it.
fn adopt(conn: &RustConnection, atoms: &Atoms, window: Window) -> WindowInfo {
    // The window can be destroyed while we ask; its destruction then arrives as an event,
    // so an empty description and a missed subscription are harmless.
    let _ = conn.change_window_attributes(
        window,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::ENTER_WINDOW),
    );
    describe(conn, atoms, window).unwrap_or_default()
}

fn describe(conn: &RustConnection, atoms: &Atoms, window: Window) -> Result<WindowInfo> {
    let modern_title = conn
        .get_property(
            false,
            window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            0,
            TEXT_PROPERTY_LEN,
        )?
        .reply()?;
    let title = if modern_title.value.is_empty() {
        let legacy = conn
            .get_property(
                false,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                0,
                TEXT_PROPERTY_LEN,
            )?
            .reply()?;
        legacy.value
    } else {
        modern_title.value
    };

    // WM_CLASS holds "instance\0class\0"; the class is the second string.
    let class = conn
        .get_property(
            false,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            TEXT_PROPERTY_LEN,
        )?
        .reply()?;
    let class = class
        .value
        .split(|byte| *byte == 0)
        .nth(1)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();

    let transient_for = conn
        .get_property(
            false,
            window,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            0,
            1,
        )?
        .reply()?
        .value32()
        .and_then(|mut parents| parents.next())
        .filter(|parent| *parent != x11rb::NONE)
        .map(crate::translate::win_id);

    Ok(WindowInfo {
        title: String::from_utf8_lossy(&title).into_owned(),
        class,
        transient_for,
    })
}
