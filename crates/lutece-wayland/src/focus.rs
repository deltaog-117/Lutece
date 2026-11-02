//! What can hold the keyboard focus: a Wayland surface, or an X11 window.
//!
//! An X11 window needs the X server told which window has the focus, which Smithay does
//! when the window itself is the focus target; the surface alone is not enough.

use crate::windows::Shell;
use smithay::backend::input::KeyState;
use smithay::input::Seat;
use smithay::input::keyboard::{KeyboardTarget, KeysymHandle, ModifiersState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Serial};
use smithay::wayland::seat::WaylandFocus;
use smithay::xwayland::X11Surface;
use std::borrow::Cow;

/// The target of the keyboard focus.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Focus {
    Wayland(WlSurface),
    X11(X11Surface),
}

impl Focus {
    /// The surface the keys are delivered to, if there is one yet.
    pub(crate) fn surface(&self) -> Option<WlSurface> {
        match self {
            Self::Wayland(surface) => Some(surface.clone()),
            Self::X11(window) => window.wl_surface(),
        }
    }
}

impl Shell {
    /// The keyboard focus target that makes this window the focused one, if it has a
    /// surface.
    pub(crate) fn focus(&self) -> Option<Focus> {
        match self {
            Self::Xdg(toplevel) => Some(Focus::Wayland(toplevel.wl_surface().clone())),
            Self::X11(window) => window.wl_surface().map(|_| Focus::X11(window.clone())),
        }
    }
}

impl WaylandFocus for Focus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        self.surface().map(Cow::Owned)
    }
}

impl IsAlive for Focus {
    fn alive(&self) -> bool {
        match self {
            Self::Wayland(surface) => surface.alive(),
            Self::X11(window) => window.alive(),
        }
    }
}

impl KeyboardTarget<crate::compositor::State> for Focus {
    fn enter(
        &self,
        seat: &Seat<crate::compositor::State>,
        data: &mut crate::compositor::State,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(surface) => KeyboardTarget::enter(surface, seat, data, keys, serial),
            Self::X11(window) => KeyboardTarget::enter(window, seat, data, keys, serial),
        }
    }

    fn leave(
        &self,
        seat: &Seat<crate::compositor::State>,
        data: &mut crate::compositor::State,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(surface) => KeyboardTarget::leave(surface, seat, data, serial),
            Self::X11(window) => KeyboardTarget::leave(window, seat, data, serial),
        }
    }

    fn key(
        &self,
        seat: &Seat<crate::compositor::State>,
        data: &mut crate::compositor::State,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: u32,
    ) {
        match self {
            Self::Wayland(surface) => {
                KeyboardTarget::key(surface, seat, data, key, state, serial, time);
            }
            Self::X11(window) => KeyboardTarget::key(window, seat, data, key, state, serial, time),
        }
    }

    fn modifiers(
        &self,
        seat: &Seat<crate::compositor::State>,
        data: &mut crate::compositor::State,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        match self {
            Self::Wayland(surface) => {
                KeyboardTarget::modifiers(surface, seat, data, modifiers, serial);
            }
            Self::X11(window) => KeyboardTarget::modifiers(window, seat, data, modifiers, serial),
        }
    }
}
