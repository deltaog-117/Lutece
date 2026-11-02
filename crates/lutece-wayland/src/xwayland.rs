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
use lutece::{Event, WinId, WindowInfo};
use smithay::delegate_xwayland_shell;
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::xwayland_shell::{XWaylandShellHandler, XWaylandShellState};
use smithay::xwayland::xwm::{Reorder, ResizeEdge, XwmId};
use smithay::xwayland::{X11Surface, X11Wm, XWayland, XWaylandEvent, XwmHandler};
use std::io;
use std::process::Stdio;

/// Starts Xwayland on `display` and, once it is ready, the window manager for it. The
/// outcome is recorded in `State::x_display` or `State::xwayland_failure`.
///
/// # Errors
///
/// Returns an error if the `Xwayland` program cannot be started or added to the loop.
pub(crate) fn start(
    handle: &LoopHandle<'static, State>,
    display: &DisplayHandle,
) -> io::Result<()> {
    let (xwayland, client) = XWayland::spawn(
        display,
        None,
        std::iter::empty::<(String, String)>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| {},
    )?;
    let wm_handle = handle.clone();
    handle
        .insert_source(xwayland, move |event, (), state| match event {
            XWaylandEvent::Ready {
                x11_socket,
                display_number,
            } => match X11Wm::start_wm(wm_handle.clone(), x11_socket, client.clone()) {
                Ok(xwm) => {
                    state.xwm = Some(xwm);
                    state.x_display = Some(display_number);
                }
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
        }
    }

    /// Forgets an X11 window that went away, telling the core if it knew it.
    fn forget_x11(&mut self, window: &X11Surface) {
        if let Some(id) = self.windows.remove_x11(window) {
            self.closed(id);
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

    fn disconnected(&mut self, _xwm: XwmId) {
        self.xwm = None;
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
