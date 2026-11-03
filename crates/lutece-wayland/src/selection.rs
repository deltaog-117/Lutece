//! Copy and paste: the clipboard (data device) and the primary selection.
//!
//! The compositor never reads a selection itself. It only keeps the selection focus on
//! the client that has keyboard focus, which is what lets that client paste what
//! another client copied.
//!
//! X11 programs have selections of their own, kept by Xwayland. The two kinds are
//! bridged: what a Wayland client copies is offered to the X11 programs, and what an
//! X11 program copies is offered to the Wayland clients. Only the list of formats
//! crosses when a selection is set; the data moves when someone pastes, straight from
//! the program that copied it.

use crate::compositor::State;
use smithay::input::Seat;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
    clear_data_device_selection, current_data_device_selection_userdata,
    request_data_device_client_selection, set_data_device_focus, set_data_device_selection,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState, clear_primary_selection,
    current_primary_selection_userdata, request_primary_client_selection, set_primary_focus,
    set_primary_selection,
};
use smithay::wayland::selection::{SelectionHandler, SelectionSource, SelectionTarget};
use smithay::{delegate_data_device, delegate_primary_selection};
use std::os::fd::OwnedFd;

impl State {
    /// Points both selections at the client that owns `focused`, or at nobody.
    pub(crate) fn follow_selection_focus(&self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let client = focused.and_then(|surface| self.display.get_client(surface.id()).ok());
        set_data_device_focus(&self.display, seat, client.clone());
        set_primary_focus(&self.display, seat, client);
    }

    /// An X11 program copied: offers its formats to the Wayland clients. The data is
    /// fetched from it only when a client pastes.
    pub(crate) fn offer_x11_selection(&self, target: SelectionTarget, mime_types: Vec<String>) {
        match target {
            SelectionTarget::Clipboard => {
                set_data_device_selection(&self.display, &self.seat, mime_types, ());
            }
            SelectionTarget::Primary => {
                set_primary_selection(&self.display, &self.seat, mime_types, ());
            }
        }
    }

    /// An X11 program gave up its selection. A selection that a Wayland client holds is
    /// left alone: it was not the X11 program's to give up.
    pub(crate) fn withdraw_x11_selection(&self, target: SelectionTarget) {
        match target {
            SelectionTarget::Clipboard => {
                if current_data_device_selection_userdata(&self.seat).is_some() {
                    clear_data_device_selection(&self.display, &self.seat);
                }
            }
            SelectionTarget::Primary => {
                if current_primary_selection_userdata(&self.seat).is_some() {
                    clear_primary_selection(&self.display, &self.seat);
                }
            }
        }
    }

    /// Writes what the Wayland client holding the selection copied to `fd`, for an X11
    /// program that pastes.
    pub(crate) fn send_wayland_selection(
        &self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
    ) {
        // If the copying client went away meanwhile, `fd` is dropped and the paster reads
        // an empty selection, which is all that can be said.
        match target {
            SelectionTarget::Clipboard => {
                drop(request_data_device_client_selection(
                    &self.seat, mime_type, fd,
                ));
            }
            SelectionTarget::Primary => {
                drop(request_primary_client_selection(&self.seat, mime_type, fd));
            }
        }
    }
}

impl SelectionHandler for State {
    type SelectionUserData = ();

    /// A Wayland client copied: offers its formats to the X11 programs.
    fn new_selection(
        &mut self,
        target: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        let Some(xwm) = self.xwm.as_mut() else {
            return;
        };
        let mime_types = source.map(|source| source.mime_types());
        // An error means the X connection is gone; Xwayland reports that on its own, and
        // the selection is of no use to a server that is not there.
        let _ = xwm.new_selection(target, mime_types);
    }

    /// A Wayland client pastes what an X11 program copied: asks that program for it.
    fn send_selection(
        &mut self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        (): &(),
    ) {
        let handle = self.loop_handle.clone();
        let Some(xwm) = self.xwm.as_mut() else {
            return;
        };
        // On failure `fd` is dropped with the error, so the pasting client reads nothing
        // instead of waiting for data that will not come.
        let _ = xwm.send_selection(target, mime_type, fd, handle);
    }
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device
    }
}

// Drag and drop between clients needs nothing from the window manager.
impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {}

impl PrimarySelectionHandler for State {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection
    }
}

delegate_data_device!(State);
delegate_primary_selection!(State);
