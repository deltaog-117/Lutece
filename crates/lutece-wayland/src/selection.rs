//! Copy and paste: the clipboard (data device) and the primary selection.
//!
//! The compositor never reads a selection itself. It only keeps the selection focus on
//! the client that has keyboard focus, which is what lets that client paste what
//! another client copied.

use crate::compositor::State;
use smithay::input::{Seat, SeatHandler};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
    set_data_device_focus,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState, set_primary_focus,
};
use smithay::{delegate_data_device, delegate_primary_selection};

impl State {
    /// Points both selections at the client that owns `focused`, or at nobody.
    pub(crate) fn follow_selection_focus(
        &self,
        seat: &Seat<Self>,
        focused: Option<&<Self as SeatHandler>::KeyboardFocus>,
    ) {
        let client =
            focused.and_then(|surface: &WlSurface| self.display.get_client(surface.id()).ok());
        set_data_device_focus(&self.display, seat, client.clone());
        set_primary_focus(&self.display, seat, client);
    }
}

impl SelectionHandler for State {
    type SelectionUserData = ();
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
