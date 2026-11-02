//! The Wayland protocol side: which globals clients see, and what the backend does when
//! a client creates, commits or destroys a window.
//!
//! Nothing here draws. A toplevel is announced to the core on its first commit, and
//! every later size or focus decision arrives from the core as a scene.

use crate::pointer::PointerState;
use crate::translate;
use crate::windows::Windows;
use lutece::{Event, KeyPress, Output as CoreOutput, Rect, WindowInfo};
use smithay::backend::input::{KeyState, Keycode};
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::desktop::{PopupKind, PopupManager};
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_seat, wl_surface::WlSurface};
use smithay::reexports::wayland_server::{
    Client, DisplayHandle,
    backend::{ClientData, ClientId, DisconnectReason},
};
use smithay::utils::{SERIAL_COUNTER, Serial};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, with_states,
};
use smithay::wayland::output::{OutputHandler, OutputManagerState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::shell::xdg::decoration::{XdgDecorationHandler, XdgDecorationState};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
    XdgToplevelSurfaceData,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_output, delegate_seat, delegate_shm, delegate_xdg_decoration,
    delegate_xdg_shell,
};
use std::collections::HashSet;

const SEAT_NAME: &str = "lutece";
/// Key repeat as most desktops default to it: wait 600 ms, then 25 keys per second.
const REPEAT_RATE: i32 = 25;
const REPEAT_DELAY: i32 = 600;

/// Everything the protocol handlers read and change.
pub(crate) struct State {
    pub(crate) display: DisplayHandle,
    compositor: CompositorState,
    xdg_shell: XdgShellState,
    shm: ShmState,
    pub(crate) data_device: DataDeviceState,
    pub(crate) primary_selection: PrimarySelectionState,
    seat_state: SeatState<Self>,
    pub(crate) seat: Seat<Self>,
    pub(crate) windows: Windows,
    /// The area of each output, to keep popups on one.
    pub(crate) screens: Vec<Rect>,
    pub(crate) popups: PopupManager,
    pub(crate) pointer: PointerState,
    /// Events for the core, waiting for the caller to collect them.
    pub(crate) events: Vec<Event>,
    /// Key combinations the core wants for itself; clients never see them.
    pub(crate) grabs: HashSet<KeyPress>,
    /// Keys whose press was taken, so their release is taken too.
    suppressed: HashSet<Keycode>,
}

/// Per-client data the protocol implementations need.
#[derive(Default)]
pub(crate) struct ClientState {
    compositor: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client: ClientId) {}
    fn disconnected(&self, _client: ClientId, _reason: DisconnectReason) {}
}

impl State {
    /// Creates the protocol globals on `display`.
    pub(crate) fn new(display: &DisplayHandle) -> Result<Self, smithay::input::keyboard::Error> {
        let mut seat_state = SeatState::new();
        let mut seat = seat_state.new_wl_seat(display, SEAT_NAME);
        seat.add_keyboard(Default::default(), REPEAT_DELAY, REPEAT_RATE)?;
        seat.add_pointer();
        // Clients need the manager to bind `wl_output`; the output itself is made by the
        // backend, which owns its size.
        let _ = OutputManagerState::new_with_xdg_output::<Self>(display);
        // Only the global matters: it stays up for as long as the display does.
        let _ = XdgDecorationState::new::<Self>(display);
        Ok(Self {
            display: display.clone(),
            compositor: CompositorState::new::<Self>(display),
            xdg_shell: XdgShellState::new::<Self>(display),
            shm: ShmState::new::<Self>(display, []),
            data_device: DataDeviceState::new::<Self>(display),
            primary_selection: PrimarySelectionState::new::<Self>(display),
            seat_state,
            seat,
            windows: Windows::default(),
            screens: Vec::new(),
            popups: PopupManager::default(),
            pointer: PointerState::new(),
            events: Vec::new(),
            grabs: HashSet::new(),
            suppressed: HashSet::new(),
        })
    }

    /// Feeds one key event through the grabs. Returns the combination when the core
    /// took it; otherwise the focused client gets the key.
    pub(crate) fn press_key(
        &mut self,
        keycode: Keycode,
        key_state: KeyState,
        time: u32,
    ) -> Option<KeyPress> {
        let keyboard = self.seat.get_keyboard()?;
        keyboard
            .input(
                self,
                keycode,
                key_state,
                SERIAL_COUNTER.next_serial(),
                time,
                |state, modifiers, handle| match key_state {
                    KeyState::Pressed => translate::key_press(modifiers, &handle)
                        .filter(|press| state.grabs.contains(press))
                        .map_or(FilterResult::Forward, |press| {
                            state.suppressed.insert(keycode);
                            FilterResult::Intercept(Some(press))
                        }),
                    KeyState::Released if state.suppressed.remove(&keycode) => {
                        FilterResult::Intercept(None)
                    }
                    KeyState::Released => FilterResult::Forward,
                },
            )
            .flatten()
    }

    /// Reports a toplevel to the core once its first commit has set its title and app id.
    fn announce(&mut self, surface: &WlSurface) {
        let Some((id, toplevel)) = self.windows.announce(surface) else {
            return;
        };
        let (title, class) = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .map(|attributes| {
                    (
                        attributes.title.clone().unwrap_or_default(),
                        attributes.app_id.clone().unwrap_or_default(),
                    )
                })
                .unwrap_or_default()
        });
        let parent = toplevel.parent();
        // The initial configure is what lets the client map; the sized one follows in the
        // scene. A window the core never places still completes its handshake.
        if !toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
        let transient_for = parent.and_then(|parent| self.windows.id_of(&parent));
        self.events.push(Event::WindowOpened {
            id,
            info: WindowInfo {
                title,
                class,
                transient_for,
            },
        });
    }

    /// A popup waits for its first configure before it draws, and that must come after
    /// its first commit, once the parent link and position are known.
    fn complete_popup_handshake(&mut self, surface: &WlSurface) {
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
        {
            // A popup whose parent died cannot be configured; it is dismissed with it.
            let _ = popup.send_configure();
        }
    }

    /// Queues the screen set the core should know about. A set still waiting is replaced
    /// in its place, since each one is the complete new set.
    pub(crate) fn report_outputs(&mut self, outputs: Vec<CoreOutput>) {
        self.screens = outputs.iter().map(|output| output.rect).collect();
        let waiting = self
            .events
            .iter_mut()
            .find(|event| matches!(event, Event::OutputsChanged(_)));
        match waiting {
            Some(event) => *event = Event::OutputsChanged(outputs),
            None => self.events.push(Event::OutputsChanged(outputs)),
        }
    }

    /// Tells the keyboard which window has focus, and which toplevel looks active.
    pub(crate) fn set_keyboard_focus(&mut self, surface: Option<WlSurface>) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
    }
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // Every client is inserted by the backend with a `ClientState`, so this cannot fail.
        &client
            .get_data::<ClientState>()
            .expect("every client is inserted with a ClientState")
            .compositor
    }

    fn commit(&mut self, surface: &WlSurface) {
        // Lets the renderer take hold of the buffer the client just attached.
        on_commit_buffer_handler::<Self>(surface);
        self.popups.commit(surface);
        self.complete_popup_handshake(surface);
        self.announce(surface);
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.windows.track(surface);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.windows.remove(&surface) {
            self.events.push(Event::WindowClosed(id));
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Popups are not managed by the core: they follow their parent window.
        let geometry = self.popup_geometry(&surface, &positioner);
        surface.with_pending_state(|state| state.geometry = geometry);
        // A popup whose parent is already gone cannot be shown, and the client is told so
        // when the surface dies, so there is nothing to do with the error.
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        let geometry = self.popup_geometry(&surface, &positioner);
        surface.with_pending_state(|state| {
            state.geometry = geometry;
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // The popup is dismissed by the first click outside it.
        self.pointer.grabbed_popups.push(surface);
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        self.pointer
            .grabbed_popups
            .retain(|grabbed| grabbed != &surface);
    }

    fn title_changed(&mut self, _surface: ToplevelSurface) {}

    fn app_id_changed(&mut self, _surface: ToplevelSurface) {}
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        self.follow_selection_focus(seat, focused);
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.pointer.cursor = image;
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        server_side(&toplevel);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: Mode) {
        // Whatever the client prefers: the compositor draws no frames, so asking the
        // client to draw none either is what removes the double title bar.
        server_side(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        server_side(&toplevel);
    }
}

/// Tells a toplevel that the compositor decorates it, which for this one means no frame.
fn server_side(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| state.decoration_mode = Some(Mode::ServerSide));
    // Before the first configure the mode rides on that one, sent when the window is
    // announced; a window already configured needs a new one.
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
    }
}

impl OutputHandler for State {}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm
    }
}

/// Marks `toplevel` as the active window, or not, in its next configure.
pub(crate) fn set_activated(toplevel: &ToplevelSurface, active: bool) {
    toplevel.with_pending_state(|state| {
        if active {
            state.states.set(xdg_toplevel::State::Activated);
        } else {
            state.states.unset(xdg_toplevel::State::Activated);
        }
    });
}

delegate_compositor!(State);
delegate_xdg_shell!(State);
delegate_xdg_decoration!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_output!(State);
