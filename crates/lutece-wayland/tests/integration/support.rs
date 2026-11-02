//! A minimal Wayland client, and a way to step it and the backend in turn.
//!
//! Both sides run on the test thread. Neither blocks: each pass lets the backend serve
//! its clients, then lets the client read what the backend sent.

use lutece::{Engine, Event, Layout, Size, StackSet, Tag, WinId};
use lutece_wayland::WaylandBackend;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use wayland_client::protocol::wl_buffer::{self, WlBuffer};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::WlDataDeviceManager;
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::{wl_compositor::WlCompositor, wl_surface::WlSurface};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop, event_created_child,
};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::{
    self, Mode, ZxdgToplevelDecorationV1,
};
use wayland_protocols::xdg::shell::client::xdg_popup::{self, XdgPopup};
use wayland_protocols::xdg::shell::client::xdg_positioner::{self, XdgPositioner};
use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::{self, XdgToplevel};
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};

/// How many back-and-forth passes settle one exchange. A request, its reply and the
/// reaction to the reply need three; the rest is margin.
const PASSES: usize = 6;
const PASS_TIMEOUT: Duration = Duration::from_millis(5);
/// The value of `xdg_toplevel.state.activated` in the protocol.
const ACTIVATED: u32 = 4;

/// What the client saw happen to one of its windows.
#[derive(Debug, Default)]
pub struct Window {
    pub configures: Vec<(i32, i32)>,
    pub activated: bool,
    pub closed: bool,
    /// The decoration mode the compositor last announced, if it announced one.
    pub decoration_mode: Option<Mode>,
    xdg: Option<XdgSurface>,
    toplevel: Option<XdgToplevel>,
    surface: Option<WlSurface>,
    decoration: Option<ZxdgToplevelDecorationV1>,
}

/// What the client saw happen to one of its popups.
#[derive(Debug)]
pub struct Popup {
    /// Each configure as `(x, y, width, height)`.
    pub configures: Vec<(i32, i32, i32, i32)>,
    pub done: bool,
    surface: WlSurface,
    _xdg: XdgSurface,
    _popup: XdgPopup,
}

/// Marks an `xdg_surface` as belonging to a popup, not a window.
pub struct PopupSlot;

#[derive(Default)]
struct Observed {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    seat: Option<WlSeat>,
    shm: Option<WlShm>,
    data_manager: Option<WlDataDeviceManager>,
    decoration_manager: Option<ZxdgDecorationManagerV1>,
    /// The registry names of the `wl_output` globals that are currently offered.
    output_globals: Vec<u32>,
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    data_device: Option<WlDataDevice>,
    windows: Vec<Window>,
    popups: Vec<Popup>,
    keyboard_enters: usize,
    keyboard_serial: u32,
    keys: usize,
    pointer_enters: usize,
    pointer_entered_popup: bool,
    pointer_position: Option<(f64, f64)>,
    pointer_buttons: usize,
    pointer_axes: usize,
    /// Mime types announced for the offer that is being described.
    pending_mimes: Vec<String>,
    selection: Option<(WlDataOffer, Vec<String>)>,
    source: Option<WlDataSource>,
    payload: Vec<u8>,
    buffers: Vec<WlBuffer>,
}

/// A client connected to a backend under test.
pub struct TestClient {
    connection: Connection,
    queue: EventQueue<Observed>,
    observed: Observed,
}

impl TestClient {
    /// Connects to `backend` and binds the globals the tests use.
    pub fn connect(backend: &mut WaylandBackend) -> (Self, Vec<Event>) {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").expect("tests need XDG_RUNTIME_DIR");
        let path = PathBuf::from(runtime).join(backend.socket_name());
        let stream = UnixStream::connect(path).expect("the backend is listening");
        let connection = Connection::from_socket(stream).expect("a Wayland connection");
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        let mut client = Self {
            connection,
            queue,
            observed: Observed::default(),
        };
        let mut events = client.settle(backend);
        client.create_data_device();
        events.extend(client.settle(backend));
        (client, events)
    }

    fn create_data_device(&mut self) {
        let qh = self.queue.handle();
        let (Some(manager), Some(seat)) = (&self.observed.data_manager, &self.observed.seat) else {
            return;
        };
        self.observed.data_device = Some(manager.get_data_device(seat, &qh, ()));
    }

    /// Creates a toplevel with a title and an app id, and commits it without a buffer.
    pub fn open_window(&mut self, title: &str, app_id: &str) -> usize {
        let qh = self.queue.handle();
        let index = self.observed.windows.len();
        let compositor = self.observed.compositor.as_ref().expect("compositor bound");
        let wm_base = self.observed.wm_base.as_ref().expect("xdg_wm_base bound");
        let surface = compositor.create_surface(&qh, ());
        let xdg = wm_base.get_xdg_surface(&surface, &qh, index);
        let toplevel = xdg.get_toplevel(&qh, index);
        toplevel.set_title(title.to_owned());
        toplevel.set_app_id(app_id.to_owned());
        surface.commit();
        self.observed.windows.push(Window {
            xdg: Some(xdg),
            toplevel: Some(toplevel),
            surface: Some(surface),
            ..Window::default()
        });
        index
    }

    /// Asks for the window to be decorated by the compositor, as a client that draws no
    /// frame of its own would.
    pub fn request_decoration(&mut self, index: usize) {
        let qh = self.queue.handle();
        let manager = self
            .observed
            .decoration_manager
            .as_ref()
            .expect("the compositor offers xdg-decoration");
        let window = &mut self.observed.windows[index];
        let toplevel = window.toplevel.as_ref().expect("the window is alive");
        window.decoration = Some(manager.get_toplevel_decoration(toplevel, &qh, index));
    }

    /// Says the client would rather draw its own frame.
    pub fn prefer_client_side_decoration(&mut self, index: usize) {
        if let Some(decoration) = &self.observed.windows[index].decoration {
            decoration.set_mode(Mode::ClientSide);
        }
    }

    /// Declares the part of the window's surface that is the window proper, as a client
    /// does that draws a shadow around it, and commits.
    pub fn set_window_geometry(&mut self, index: usize, (x, y): (i32, i32), (w, h): (i32, i32)) {
        let window = &self.observed.windows[index];
        window
            .xdg
            .as_ref()
            .expect("the window is alive")
            .set_window_geometry(x, y, w, h);
        window.surface.as_ref().expect("alive").commit();
    }

    /// How many `wl_output` globals the compositor offers now.
    pub fn output_count(&self) -> usize {
        self.observed.output_globals.len()
    }

    /// Gives a window a buffer of `width` x `height` pixels, so it has a size on screen
    /// and can be found under the pointer.
    pub fn map_window(&mut self, index: usize, width: i32, height: i32) {
        let buffer = self.buffer(width, height);
        let surface = self.observed.windows[index]
            .surface
            .as_ref()
            .expect("the window is alive");
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        self.observed.buffers.push(buffer);
    }

    /// Opens a popup of `width` x `height` whose top-left corner sits at `(x, y)` in the
    /// parent window's surface, and gives it a buffer of that size.
    pub fn open_popup(
        &mut self,
        parent: usize,
        (x, y): (i32, i32),
        (width, height): (i32, i32),
        grab: bool,
    ) -> usize {
        let qh = self.queue.handle();
        let slot = self.observed.popups.len();
        let positioner = self
            .observed
            .wm_base
            .as_ref()
            .expect("xdg_wm_base bound")
            .create_positioner(&qh, ());
        positioner.set_size(width, height);
        positioner.set_anchor_rect(x, y, 1, 1);
        positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
        positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        // As menus ask, so a popup near an edge slides back instead of leaving the screen.
        positioner.set_constraint_adjustment(
            xdg_positioner::ConstraintAdjustment::SlideX
                | xdg_positioner::ConstraintAdjustment::SlideY,
        );
        let parent_xdg = self.observed.windows[parent].xdg.as_ref().expect("alive");
        let surface = self
            .observed
            .compositor
            .as_ref()
            .expect("compositor bound")
            .create_surface(&qh, ());
        let xdg = self
            .observed
            .wm_base
            .as_ref()
            .expect("xdg_wm_base bound")
            .get_xdg_surface(&surface, &qh, PopupSlot);
        let popup = xdg.get_popup(Some(parent_xdg), &positioner, &qh, slot);
        if grab {
            let seat = self.observed.seat.as_ref().expect("wl_seat bound");
            popup.grab(seat, self.observed.keyboard_serial);
        }
        surface.commit();
        self.observed.popups.push(Popup {
            configures: Vec::new(),
            done: false,
            surface,
            _xdg: xdg,
            _popup: popup,
        });
        positioner.destroy();
        slot
    }

    /// Attaches a buffer to a popup so it has a size on screen.
    pub fn map_popup(&mut self, slot: usize, width: i32, height: i32) {
        let buffer = self.buffer(width, height);
        let surface = &self.observed.popups[slot].surface;
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        self.observed.buffers.push(buffer);
    }

    fn buffer(&self, width: i32, height: i32) -> WlBuffer {
        let qh = self.queue.handle();
        let stride = width * 4;
        let size = stride * height;
        let file = tempfile::tempfile().expect("an anonymous file");
        file.set_len(u64::try_from(size).expect("a positive size"))
            .expect("a sized file");
        let pool = self
            .observed
            .shm
            .as_ref()
            .expect("wl_shm bound")
            .create_pool(file.as_fd(), size, &qh, ());
        let buffer =
            pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, &qh, ());
        pool.destroy();
        buffer
    }

    /// Destroys a window the way a client closing it would.
    pub fn destroy_window(&mut self, index: usize) {
        let window = &mut self.observed.windows[index];
        if let Some(toplevel) = window.toplevel.take() {
            toplevel.destroy();
        }
        if let Some(xdg) = window.xdg.take() {
            xdg.destroy();
        }
        if let Some(surface) = window.surface.take() {
            surface.destroy();
        }
    }

    /// Offers `payload` as the clipboard, as `text/plain`.
    pub fn copy(&mut self, payload: &[u8]) {
        let qh = self.queue.handle();
        let manager = self.observed.data_manager.as_ref().expect("manager bound");
        let source = manager.create_data_source(&qh, ());
        source.offer("text/plain".to_owned());
        self.observed
            .data_device
            .as_ref()
            .expect("a data device")
            .set_selection(Some(&source), self.observed.keyboard_serial);
        self.observed.source = Some(source);
        self.observed.payload = payload.to_vec();
    }

    /// Asks the current clipboard offer for `text/plain`. Returns the pipe to read the
    /// answer from, once the sender has been given time to write it.
    pub fn paste(&mut self) -> std::io::PipeReader {
        let (reader, writer) = std::io::pipe().expect("a pipe");
        let (offer, _) = self.observed.selection.as_ref().expect("an offer to paste");
        offer.receive("text/plain".to_owned(), writer.as_fd());
        reader
    }

    /// The mime types of the clipboard offer this client currently sees.
    pub fn offered(&self) -> Option<&[String]> {
        self.observed
            .selection
            .as_ref()
            .map(|(_, mimes)| mimes.as_slice())
    }

    pub fn window(&self, index: usize) -> &Window {
        &self.observed.windows[index]
    }

    pub fn popup(&self, slot: usize) -> &Popup {
        &self.observed.popups[slot]
    }

    pub fn keyboard_enters(&self) -> usize {
        self.observed.keyboard_enters
    }

    /// Keys the client has been sent.
    pub fn keys(&self) -> usize {
        self.observed.keys
    }

    pub fn pointer_enters(&self) -> usize {
        self.observed.pointer_enters
    }

    /// Whether the pointer ever entered one of this client's popups.
    pub fn pointer_entered_popup(&self) -> bool {
        self.observed.pointer_entered_popup
    }

    /// Where the pointer last was, in the coordinates of the surface it is over.
    pub fn pointer_position(&self) -> Option<(f64, f64)> {
        self.observed.pointer_position
    }

    pub fn pointer_buttons(&self) -> usize {
        self.observed.pointer_buttons
    }

    pub fn pointer_axes(&self) -> usize {
        self.observed.pointer_axes
    }

    /// Lets both sides react until nothing more is in flight. Returns the events the
    /// backend produced meanwhile.
    pub fn settle(&mut self, backend: &mut WaylandBackend) -> Vec<Event> {
        settle_all(backend, &mut [self])
    }
}

/// Like [`TestClient::settle`] for several clients at once, so none starves.
pub fn settle_all(backend: &mut WaylandBackend, clients: &mut [&mut TestClient]) -> Vec<Event> {
    let mut events = Vec::new();
    for _ in 0..PASSES {
        for client in clients.iter_mut() {
            client.connection.flush().expect("flush to the backend");
        }
        events.extend(backend.next_events(PASS_TIMEOUT).expect("backend serves"));
        for client in clients.iter_mut() {
            if let Some(guard) = client.queue.prepare_read() {
                // Nothing to read is not an error here: the backend may have said nothing.
                let _ = guard.read();
            }
            client
                .queue
                .dispatch_pending(&mut client.observed)
                .expect("the client handles every event");
        }
    }
    events
}

/// A backend with a 1000x600 output.
pub fn backend() -> WaylandBackend {
    WaylandBackend::bind(Size::new(1000, 600).expect("non-zero size")).expect("backend binds")
}

impl Dispatch<WlRegistry, ()> for Observed {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let (name, interface, version) = match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => (name, interface, version),
            wl_registry::Event::GlobalRemove { name } => {
                state.output_globals.retain(|offered| *offered != name);
                return;
            }
            _ => return,
        };
        match interface.as_str() {
            "wl_compositor" => {
                state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(2), qh, ())),
            "wl_seat" => state.seat = Some(registry.bind(name, version.min(5), qh, ())),
            "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
            "wl_data_device_manager" => {
                state.data_manager = Some(registry.bind(name, version.min(3), qh, ()));
            }
            "zxdg_decoration_manager_v1" => {
                state.decoration_manager = Some(registry.bind(name, 1, qh, ()));
            }
            "wl_output" => state.output_globals.push(name),
            _ => {}
        }
    }
}

impl Dispatch<XdgWmBase, ()> for Observed {
    fn event(
        _: &mut Self,
        wm_base: &XdgWmBase,
        event: xdg_wm_base::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, usize> for Observed {
    fn event(
        _: &mut Self,
        xdg: &XdgSurface,
        event: xdg_surface::Event,
        _: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg.ack_configure(serial);
        }
    }
}

impl Dispatch<XdgSurface, PopupSlot> for Observed {
    fn event(
        _: &mut Self,
        xdg: &XdgSurface,
        event: xdg_surface::Event,
        _: &PopupSlot,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg.ack_configure(serial);
        }
    }
}

impl Dispatch<XdgToplevel, usize> for Observed {
    fn event(
        state: &mut Self,
        _: &XdgToplevel,
        event: xdg_toplevel::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let window = &mut state.windows[*index];
        match event {
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                window.configures.push((width, height));
                window.activated = states
                    .chunks_exact(4)
                    .any(|raw| u32::from_ne_bytes([raw[0], raw[1], raw[2], raw[3]]) == ACTIVATED);
            }
            xdg_toplevel::Event::Close => window.closed = true,
            _ => {}
        }
    }
}

impl Dispatch<XdgPopup, usize> for Observed {
    fn event(
        state: &mut Self,
        _: &XdgPopup,
        event: xdg_popup::Event,
        slot: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let popup = &mut state.popups[*slot];
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => popup.configures.push((x, y, width, height)),
            xdg_popup::Event::PopupDone => popup.done = true,
            _ => {}
        }
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, usize> for Observed {
    fn event(
        state: &mut Self,
        _: &ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_toplevel_decoration_v1::Event::Configure {
            mode: WEnum::Value(mode),
        } = event
        {
            state.windows[*index].decoration_mode = Some(mode);
        }
    }
}

impl Dispatch<WlSeat, ()> for Observed {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        else {
            return;
        };
        if capabilities.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
            state.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
            state.pointer = Some(seat.get_pointer(qh, ()));
        }
    }
}

impl Dispatch<WlKeyboard, ()> for Observed {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { serial, .. } => {
                state.keyboard_enters += 1;
                state.keyboard_serial = serial;
            }
            wl_keyboard::Event::Key { .. } => state.keys += 1,
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, ()> for Observed {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_enters += 1;
                state.pointer_position = Some((surface_x, surface_y));
                if state.popups.iter().any(|popup| popup.surface == surface) {
                    state.pointer_entered_popup = true;
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => state.pointer_position = Some((surface_x, surface_y)),
            wl_pointer::Event::Button { .. } => state.pointer_buttons += 1,
            wl_pointer::Event::Axis { .. } => state.pointer_axes += 1,
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for Observed {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_device::Event::Selection { id } = event {
            let mimes = std::mem::take(&mut state.pending_mimes);
            state.selection = id.map(|offer| (offer, mimes));
        }
    }

    event_created_child!(Observed, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for Observed {
    fn event(
        state: &mut Self,
        _: &WlDataOffer,
        event: wl_data_offer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            state.pending_mimes.push(mime_type);
        }
    }
}

impl Dispatch<WlDataSource, ()> for Observed {
    fn event(
        state: &mut Self,
        _: &WlDataSource,
        event: wl_data_source::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_source::Event::Send { fd, .. } = event {
            let mut pipe = std::fs::File::from(fd);
            pipe.write_all(&state.payload)
                .expect("the reader is waiting");
        }
    }
}

impl Dispatch<WlBuffer, ()> for Observed {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: wl_buffer::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Observed: ignore WlCompositor);
delegate_noop!(Observed: ignore WlSurface);
delegate_noop!(Observed: ignore WlShm);
delegate_noop!(Observed: ignore WlShmPool);
delegate_noop!(Observed: ignore WlDataDeviceManager);
delegate_noop!(Observed: ignore XdgPositioner);
delegate_noop!(Observed: ignore ZxdgDecorationManagerV1);

/// An engine with one workspace.
pub fn engine() -> Engine {
    let stacks = StackSet::new([Tag::try_new("1").expect("a valid tag")]).expect("one workspace");
    Engine::new(stacks, Layout::default())
}

/// Runs events through the engine and applies the resulting effects and scene. Returns
/// the windows that were opened.
pub fn drive(engine: &mut Engine, backend: &mut WaylandBackend, events: Vec<Event>) -> Vec<WinId> {
    let mut opened = Vec::new();
    for event in events {
        if let Event::WindowOpened { id, .. } = &event {
            opened.push(*id);
        }
        backend
            .apply_effects(&engine.handle(event))
            .expect("effects apply");
    }
    backend
        .apply_scene(&engine.scene().expect("a valid scene"))
        .expect("scene applies");
    opened
}
