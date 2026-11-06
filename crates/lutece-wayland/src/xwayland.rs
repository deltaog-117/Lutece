//! X11 programs, through Xwayland.
//!
//! Xwayland is an X server that is itself a client of this compositor: it turns each X11
//! window into a Wayland surface. This module is the window manager Xwayland needs. A
//! window that asks to be mapped is granted that, then reported to the core like any
//! other window and placed by its scenes. Menus and tooltips (X11 *override-redirect*
//! windows) are not managed: they stay where their program put them, drawn above the
//! windows.
//!
//! Smithay runs Xwayland and its window manager on a `calloop` loop. The backend keeps
//! that loop inside itself and steps it without waiting, so the caller still owns the
//! only loop that blocks.

use crate::compositor::State;
use crate::focus::Focus;
use lutece::{Event, WinId, WindowInfo};
use smithay::delegate_xwayland_shell;
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::selection::SelectionTarget;
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge, XwmId};
use smithay::xwayland::{X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler};
use std::io;
use std::os::fd::OwnedFd;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use x11rb::connection::Connection;
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, PropMode, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// How many display numbers [`Displays`] cycles through.
const DISPLAY_RANGE: u32 = 256;

/// Hands out X display numbers so that no two Xwayland instances of one process share one
/// until the whole range has been used.
///
/// Smithay would pick the lowest free number, so a number released by one instance is taken
/// at once by the next. Each instance cleans up its lock and socket files by path, and an
/// older Xwayland that is still shutting down deletes them from under the newer one, which
/// then refuses every client. A number that was not used before has no older instance.
#[derive(Debug, Default)]
struct Displays {
    next: AtomicU32,
}

impl Displays {
    /// The next candidate. The first one is 0, so a lone Xwayland still gets the lowest
    /// free number once the busy ones are skipped.
    fn take(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) % DISPLAY_RANGE
    }
}

static DISPLAYS: Displays = Displays {
    next: AtomicU32::new(0),
};

/// Starts Xwayland and, once it is ready, the window manager for it. The outcome is
/// recorded in `State::x_display` or `State::xwayland_failure`.
///
/// # Errors
///
/// Returns an error if the `Xwayland` program cannot be started or added to the loop, or
/// if every display number is in use.
pub(crate) fn start(
    handle: &LoopHandle<'static, State>,
    display: &DisplayHandle,
) -> io::Result<()> {
    let mut spawned = None;
    for _ in 0..DISPLAY_RANGE {
        let attempt = XWayland::spawn(
            display,
            Some(DISPLAYS.take()),
            std::iter::empty::<(String, String)>(),
            true,
            Stdio::null(),
            Stdio::null(),
            |_| {},
        );
        match attempt {
            Ok(started) => {
                spawned = Some(started);
                break;
            }
            // Another program holds this number; try the next.
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(error),
        }
    }
    let (xwayland, client) = spawned.ok_or_else(|| {
        io::Error::new(io::ErrorKind::AddrInUse, "every X display number is in use")
    })?;
    let wm_handle = handle.clone();
    handle
        .insert_source(xwayland, move |event, (), state| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => match X11Wm::start_wm(wm_handle.clone(), x11_socket, client.clone()) {
                Ok(xwm) => match StateReader::connect(display_number) {
                    Ok(reader) => {
                        state.xwm = Some(xwm);
                        state.x_state = Some(reader);
                        state.x_display = Some(display_number);
                    }
                    Err(error) => {
                        state.xwayland_failure =
                            Some(format!("cannot read the state of its windows: {error}"));
                    }
                },
                Err(error) => {
                    state.xwayland_failure =
                        Some(format!("cannot start its window manager: {error}"));
                }
            },
            XWaylandEvent::Error => {
                state.xwayland_failure = Some("it exited while starting".to_owned());
            }
        })
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(())
}

/// A second connection to Xwayland for what Smithay's window manager leaves alone.
///
/// It reads the `_NET_WM_STATE` a program sets before it maps its window: Smithay treats
/// that property as the window manager's to write, so a program that starts fullscreen
/// would otherwise never be seen asking. It also writes `_NET_ACTIVE_WINDOW` on the root:
/// Smithay only hears of focus changes on the root window, so what it writes there is the
/// root itself or the window that just lost the focus, never the window that has it.
///
/// A read waits for Xwayland's answer. That is no new kind of wait: Smithay's own window
/// manager already asks Xwayland about every window it maps, on this same thread.
pub(crate) struct StateReader {
    conn: RustConnection,
    root: Window,
    state: Atom,
    fullscreen: Atom,
    active: Atom,
}

impl StateReader {
    /// Connects to the Xwayland that serves display `number`.
    ///
    /// # Errors
    ///
    /// Returns what went wrong if the display cannot be reached or the two atoms cannot
    /// be looked up.
    fn connect(number: u32) -> Result<Self, String> {
        let (conn, screen) =
            RustConnection::connect(Some(&format!(":{number}"))).map_err(|e| e.to_string())?;
        let root = conn.setup().roots[screen].root;
        let intern = |name: &[u8]| {
            conn.intern_atom(false, name)
                .map_err(|e| e.to_string())?
                .reply()
                .map(|reply| reply.atom)
                .map_err(|e| e.to_string())
        };
        let state = intern(b"_NET_WM_STATE")?;
        let fullscreen = intern(b"_NET_WM_STATE_FULLSCREEN")?;
        let active = intern(b"_NET_ACTIVE_WINDOW")?;
        // Hear of every write to the root's properties, to notice Smithay overwriting
        // the active window. Unlike redirecting its children, this is open to any client.
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(|e| e.to_string())?;
        conn.flush().map_err(|e| e.to_string())?;
        Ok(Self {
            conn,
            root,
            state,
            fullscreen,
            active,
        })
    }

    /// Sets `_NET_ACTIVE_WINDOW` to the X window numbered `window`; 0 names none. If the
    /// server is gone there is nothing to tell.
    fn set_active_window(&self, window: Window) {
        drop(self.conn.change_property32(
            PropMode::REPLACE,
            self.root,
            self.active,
            AtomEnum::WINDOW,
            &[window],
        ));
        drop(self.conn.flush());
    }

    /// The window `_NET_ACTIVE_WINDOW` names now; 0 when it names none or cannot be read.
    fn active_window(&self) -> Window {
        self.conn
            .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().and_then(|mut windows| windows.next()))
            .unwrap_or(0)
    }

    /// Whether `_NET_ACTIVE_WINDOW` was written since this was last asked, by anyone,
    /// this connection included.
    fn active_window_written(&self) -> bool {
        let mut written = false;
        while let Ok(Some(event)) = self.conn.poll_for_event() {
            written |= matches!(event, XEvent::PropertyNotify(n) if n.atom == self.active);
        }
        written
    }

    /// Whether the X window `window` has the fullscreen state set. A window that is gone
    /// or has no such property is not fullscreen.
    pub(crate) fn is_fullscreen(&self, window: u32) -> bool {
        self.conn
            .get_property(false, window, self.state, AtomEnum::ATOM, 0, STATE_ATOMS)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| {
                reply
                    .value32()
                    .map(|mut atoms| atoms.any(|a| a == self.fullscreen))
            })
            .unwrap_or(false)
    }
}

/// How many atoms of `_NET_WM_STATE` are read; a window sets a handful at most.
const STATE_ATOMS: u32 = 64;

impl State {
    /// Reports every X11 window that now has a committed surface to the core.
    pub(crate) fn announce_x11_windows(&mut self) {
        for (id, window) in self.windows.announce_x11() {
            let transient_for = window
                .is_transient_for()
                .and_then(|parent| self.windows.id_of_x11(parent));
            self.events.push(Event::WindowOpened {
                id,
                info: WindowInfo {
                    title: window.title(),
                    class: window.class(),
                    transient_for,
                },
            });
            // After the window opens, so the core knows it when it is asked.
            if self.windows.take_early_x11_fullscreen(window.window_id()) {
                self.events
                    .push(Event::FullscreenRequested { id, on: true });
            }
        }
    }

    /// Puts the stacking order of the X11 windows in step with the scene's: Xwayland
    /// keeps `_NET_CLIENT_LIST_STACKING` in the order the windows mapped otherwise, so a
    /// pager or a program that restacks itself would see the wrong one.
    pub(crate) fn restack_x11(&mut self) {
        let Some(xwm) = self.xwm.as_mut() else {
            return;
        };
        let order = self.windows.x11_in_order();
        // If Xwayland is gone there is nothing to order.
        // Smithay takes the order from the top down.
        drop(xwm.update_stacking_order_downwards(order.iter().rev()));
    }

    /// Makes `_NET_ACTIVE_WINDOW` name the X11 window that has the keyboard focus, or no
    /// window when a Wayland surface has it. A write is only made when the property is
    /// not right, so it settles even though this connection hears its own writes.
    pub(crate) fn sync_active_window(&mut self) {
        let Some(reader) = self.x_state.as_ref() else {
            return;
        };
        let want = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
            .map_or(0, |focus| match focus {
                Focus::X11(window) => window.window_id(),
                _ => 0,
            });
        // Always drain, so a notice is never left to look new at the next call.
        let written = reader.active_window_written();
        if want != self.x_active || (written && reader.active_window() != want) {
            reader.set_active_window(want);
            self.x_active = want;
        }
    }

    /// Forgets an X11 window that went away, telling the core if it knew it.
    fn forget_x11(&mut self, window: &X11Surface) {
        if let Some(id) = self.windows.remove_x11(window) {
            self.closed(id);
        }
    }

    /// An X11 program asked to be fullscreen (`on`) or to stop being, with a
    /// `_NET_WM_STATE` message. A window that is not announced yet is ignored here; one
    /// that set the state before it mapped is found by [`StateReader`] when it asks to map.
    fn x11_fullscreen(&mut self, window: &X11Surface, on: bool) {
        if let Some(id) = self.windows.id_of_x11(window.window_id()) {
            self.events.push(Event::FullscreenRequested { id, on });
        }
    }

    fn closed(&mut self, id: WinId) {
        self.events.push(Event::WindowClosed(id));
    }
}

impl XwmHandler for State {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm
            .as_mut()
            .expect("the window manager only reports events while it is stored in the state")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        // A window that cannot be mapped is already gone; nothing is left to manage.
        if window.set_mapped(true).is_err() {
            return;
        }
        // Read now: the program set the state before asking to map, and the window is not
        // announced until its surface is committed.
        if self
            .x_state
            .as_ref()
            .is_some_and(|reader| reader.is_fullscreen(window.window_id()))
        {
            self.windows.remember_x11_fullscreen(window.window_id());
        }
        self.windows.track_x11(window);
        self.announce_x11_windows();
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.windows.show_override_redirect(window);
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.forget_x11(&window);
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        self.forget_x11(&window);
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<Reorder>,
    ) {
        // A managed window is placed by the core: its program is only told what it has.
        // Any other window (one not mapped yet) gets what it asks for.
        let managed = self.windows.id_of_x11(window.window_id()).is_some();
        let rect = (!managed).then(|| requested(window.geometry(), x, y, w, h));
        // If the program is gone there is nobody to tell.
        drop(window.configure(rect));
    }

    fn fullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        self.x11_fullscreen(&window, true);
    }

    fn unfullscreen_request(&mut self, _xwm: XwmId, window: X11Surface) {
        self.x11_fullscreen(&window, false);
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
    }

    // The core owns the geometry of managed windows, so a program asking to move or
    // resize itself with the mouse is ignored.
    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _edge: ResizeEdge,
    ) {
    }

    fn move_request(&mut self, _xwm: XwmId, _window: X11Surface, _button: u32) {}

    // An X11 program may read what Wayland clients copied only while an X11 window has
    // the keyboard focus, the way a Wayland client may only while one of its surfaces
    // does. Without this, any background X11 program could read the clipboard at will.
    fn allow_selection_access(&mut self, xwm: XwmId, _selection: SelectionTarget) -> bool {
        self.seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
            .is_some_and(
                |focus| matches!(focus, Focus::X11(window) if window.xwm_id() == Some(xwm)),
            )
    }

    fn send_selection(
        &mut self,
        _xwm: XwmId,
        selection: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        self.send_wayland_selection(selection, mime_type, fd);
    }

    fn new_selection(&mut self, _xwm: XwmId, selection: SelectionTarget, mime_types: Vec<String>) {
        self.offer_x11_selection(selection, mime_types);
    }

    fn cleared_selection(&mut self, _xwm: XwmId, selection: SelectionTarget) {
        self.withdraw_x11_selection(selection);
    }

    fn disconnected(&mut self, _xwm: XwmId) {
        self.xwm = None;
        self.x_state = None;
        self.x_active = 0;
        self.x_display = None;
    }
}

impl XWaylandShellHandler for State {
    fn xwayland_shell_state(&mut self) -> &mut XWaylandShellState {
        &mut self.xwayland_shell
    }

    fn surface_associated(&mut self, _xwm: XwmId, _surface: WlSurface, _window: X11Surface) {
        // The window now has its surface. It may have asked to be mapped before or after.
        self.announce_x11_windows();
    }
}

/// `current` with the parts a program asked to change replaced.
fn requested(
    current: Rectangle<i32, Logical>,
    x: Option<i32>,
    y: Option<i32>,
    w: Option<u32>,
    h: Option<u32>,
) -> Rectangle<i32, Logical> {
    let length = |asked: Option<u32>, now: i32| {
        asked.map_or(now, |asked| i32::try_from(asked).unwrap_or(i32::MAX))
    };
    Rectangle::new(
        (x.unwrap_or(current.loc.x), y.unwrap_or(current.loc.y)).into(),
        (length(w, current.size.w), length(h, current.size.h)).into(),
    )
}

delegate_xwayland_shell!(State);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_numbers_start_at_zero_and_do_not_repeat_within_the_range() {
        let displays = Displays::default();
        let taken: Vec<u32> = (0..DISPLAY_RANGE).map(|_| displays.take()).collect();
        assert_eq!(taken.first(), Some(&0));
        let mut sorted = taken.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), taken.len(), "no number repeated");
        assert!(taken.iter().all(|number| *number < DISPLAY_RANGE));
    }

    #[test]
    fn display_numbers_wrap_instead_of_overflowing() {
        let displays = Displays {
            next: AtomicU32::new(u32::MAX),
        };
        assert!(displays.take() < DISPLAY_RANGE);
        assert!(displays.take() < DISPLAY_RANGE, "the counter wrapped");
    }

    #[test]
    fn a_request_changes_only_what_it_names() {
        let current = Rectangle::new((10, 20).into(), (300, 200).into());
        assert_eq!(
            requested(current, None, Some(5), Some(640), None),
            Rectangle::new((10, 5).into(), (640, 200).into())
        );
    }

    #[test]
    fn an_empty_request_changes_nothing() {
        let current = Rectangle::new((1, 2).into(), (3, 4).into());
        assert_eq!(requested(current, None, None, None, None), current);
    }

    #[test]
    fn an_enormous_size_is_clamped_not_wrapped() {
        let current = Rectangle::new((0, 0).into(), (1, 1).into());
        assert_eq!(
            requested(current, None, None, Some(u32::MAX), None).size.w,
            i32::MAX
        );
    }
}
