//! The connection to the X server, and the backend built on it.

use crate::apply::{Flow, execute, plan, run_effects};
use crate::error::{Error, Result};
use crate::grabs::{Keyboard, grab_keys};
use crate::translate::{key_press, translate};
use crate::windows::Windows;
use lutece::{Effect, Event, KeyPress, Output, OutputId, Rect, Scene, WindowInfo, diff};
use std::mem;
use std::os::fd::{AsFd, BorrowedFd};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::ErrorKind;
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt, EventMask, Window,
};
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    pub(crate) Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        UTF8_STRING,
        _NET_WM_NAME,
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
    atoms: Atoms,
    keyboard: Keyboard,
    windows: Windows,
    /// What the X server is showing, so a new scene is applied as a difference.
    shown: Scene,
    /// Events produced before the first X event, such as the screen size.
    queued: Vec<Event>,
}

impl X11Backend {
    /// Connects to the display named by `DISPLAY` and becomes its window manager.
    ///
    /// The first call to [`next_events`](Self::next_events) reports the screen.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AnotherWindowManager`] if one is already running, or an error
    /// if the display cannot be reached.
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) = RustConnection::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let area = Rect::new(
            0,
            0,
            u32::from(screen.width_in_pixels),
            u32::from(screen.height_in_pixels),
        )?;
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

        let output = Output {
            id: OutputId::new(0),
            rect: area,
        };
        Ok(Self {
            conn,
            root,
            atoms,
            keyboard,
            windows: Windows::default(),
            shown: Scene::empty(),
            queued: vec![Event::OutputsChanged(vec![output])],
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
        grab_keys(&self.conn, &self.keyboard, self.root, presses)?;
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
        execute(&self.conn, self.root, &ops)?;
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
    pub fn apply_effects(&mut self, effects: &[Effect]) -> Result<Flow> {
        run_effects(&self.conn, &self.atoms, effects)
    }

    fn translate_into_queue(&mut self, event: &XEvent) -> Result<()> {
        let Self {
            conn,
            atoms,
            keyboard,
            windows,
            queued,
            ..
        } = self;

        if let XEvent::KeyPress(press) = event {
            queued.extend(key_press(keyboard, press));
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

        queued.extend(translate(windows, event, |window| {
            adopt(conn, atoms, window)
        }));
        Ok(())
    }
}

impl AsFd for X11Backend {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.conn.stream().as_fd()
    }
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
