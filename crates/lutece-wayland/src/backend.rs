//! The backend a window manager drives: it serves Wayland clients, turns what they do
//! into core events, and turns scenes and effects back into protocol messages.

use crate::apply::{Flow, apply_changes, apply_effects};
use crate::compositor::{ClientState, State};
use crate::dmabuf::DmabufSupport;
use crate::error::{Error, Result};
use crate::render::SceneElement;
use crate::scale::Scale;
use lutece::{
    Effect, Event, KeyPress, Modifiers, Output, OutputId, Point as CorePoint, Rect, Scene, Size,
    diff,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use smithay::backend::input::{KeyState, Keycode};
use smithay::backend::renderer::{ImportAll, Renderer};
use smithay::input::pointer::CursorImageStatus;
use smithay::output::{
    Mode, Output as WlOutput, PhysicalProperties, Scale as OutputScale, Subpixel,
};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::{Display, ListeningSocket};
use smithay::utils::Point;
use std::ffi::OsStr;
use std::io;
use std::sync::Arc;
use std::time::Duration;

/// The socket names tried are `lutece-0` up to `lutece-31`, so a nested compositor never
/// takes the name of the session it runs in.
const SOCKET_BASE: &str = "lutece";
const SOCKET_TRIES: std::ops::Range<usize> = 0..32;
const OUTPUT_ID: OutputId = OutputId::new(0);
/// A virtual output has no real refresh rate; clients only need a plausible one.
const REFRESH_MILLIHERTZ: i32 = 60_000;

/// Whether a key went down or came up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// The key went down.
    Press,
    /// The key came up.
    Release,
}

/// Whether a pointer button went down or came up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    /// The button went down.
    Press,
    /// The button came up.
    Release,
}

/// A Wayland compositor the core drives: any number of virtual outputs and clients.
///
/// The caller owns the loop, as with the X11 backend: collect events with
/// [`next_events`](Self::next_events), feed them to the engine, then apply the effects
/// and the scene it returns.
pub struct WaylandBackend {
    display: Display<State>,
    listener: ListeningSocket,
    state: State,
    /// Runs Xwayland and its window manager. It never blocks: the caller's own loop
    /// waits, and this one is stepped from it.
    event_loop: EventLoop<'static, State>,
    xwayland_enabled: bool,
    scene: Scene,
    /// Invariant: each output id appears once.
    screens: Vec<Screen>,
}

/// An output clients can see, and the global that announces it to them.
struct Screen {
    id: OutputId,
    /// The area in logical pixels.
    rect: Rect,
    output: WlOutput,
    global: GlobalId,
}

impl WaylandBackend {
    /// Starts listening for clients on a free socket, with one output of `size` pixels.
    /// The first call to [`next_events`](Self::next_events) reports that output.
    ///
    /// # Errors
    ///
    /// Returns an error if no socket can be bound (for example `XDG_RUNTIME_DIR` is
    /// unset) or the keyboard layout cannot be loaded.
    pub fn bind(size: Size) -> Result<Self> {
        let display = Display::<State>::new()?;
        let handle = display.handle();
        let event_loop = EventLoop::try_new().map_err(|error| io::Error::other(error.to_string()))?;
        let state = State::new(&handle, event_loop.handle())?;
        let listener = ListeningSocket::bind_auto(SOCKET_BASE, SOCKET_TRIES)?;
        let mut backend = Self {
            display,
            listener,
            state,
            event_loop,
            xwayland_enabled: false,
            scene: Scene::empty(),
            screens: Vec::new(),
        };
        backend.set_output_size(size)?;
        Ok(backend)
    }

    /// The name clients put in `WAYLAND_DISPLAY` to reach this compositor.
    #[must_use]
    pub fn socket_name(&self) -> &OsStr {
        self.listener
            .socket_name()
            .expect("a socket bound by name always has a name")
    }

    /// Changes the size of the only output, for example when the window it is shown in is
    /// resized, and reports the new screen to the core on the next
    /// [`next_events`](Self::next_events). Any other output is removed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Core`] if the size does not fit the core's geometry types.
    pub fn set_output_size(&mut self, size: Size) -> Result<()> {
        let rect = Rect::from_parts(CorePoint { x: 0, y: 0 }, size);
        self.set_outputs(&[Output {
            id: OUTPUT_ID,
            rect,
        }]);
        Ok(())
    }

    /// Lets X11 programs run: starts Xwayland, an X server that shows each X11 window as
    /// a window of this compositor. Those windows reach the core as any other and are
    /// placed by its scenes; their menus and tooltips are drawn where their program asks.
    /// Programs started by a spawn effect find it through `DISPLAY` once it is up.
    ///
    /// Xwayland takes a moment to start; [`xwayland_display`](Self::xwayland_display)
    /// tells when it is ready. Calling this again does nothing. Copy and paste work between
    /// X11 and Wayland programs, for the clipboard and the primary selection.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the `Xwayland` program cannot be started.
    pub fn enable_xwayland(&mut self) -> Result<()> {
        if self.xwayland_enabled {
            return Ok(());
        }
        crate::xwayland::start(&self.event_loop.handle(), &self.display.handle())?;
        self.xwayland_enabled = true;
        Ok(())
    }

    /// The X11 display number (`:N`) of Xwayland, once it is ready to serve X11 programs.
    #[must_use]
    pub fn xwayland_display(&self) -> Option<u32> {
        self.state.x_display
    }

    /// Sets how many physical pixels one logical pixel is on the screen the compositor
    /// is shown on. The core's rectangles stay logical, so nothing there changes; clients
    /// are told to draw that much finer: through `wp_fractional_scale` with the exact
    /// value, and through `wl_output` with it rounded up. Outputs keep their logical area
    /// and report the physical size that goes with it.
    ///
    /// Clients draw at the scale only if they ask for it by binding `wp_fractional_scale`;
    /// others stay at 1 and are enlarged when drawn.
    pub fn set_scale(&mut self, scale: Scale) {
        self.state.set_scale(scale);
        for screen in &self.screens {
            show_area(&screen.output, screen.rect, scale);
        }
        self.state.refresh_all_presence();
    }

    /// Offers clients GPU buffers (`linux-dmabuf`) on the GPU `support` describes, which
    /// is the one the host draws with. A client that draws with the GPU then hands its
    /// frames over without copying them. Calling it again replaces the offer.
    ///
    /// A buffer is accepted when its format was offered; whether the driver can use it is
    /// only known when it is drawn, and one it rejects is left out of the frame.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the list of formats cannot be shared with clients.
    pub fn enable_dmabuf(&mut self, support: &DmabufSupport) -> Result<()> {
        self.state.enable_dmabuf(support)?;
        Ok(())
    }

    /// Makes `outputs` the complete set of outputs. One whose id is new appears to
    /// clients, one whose id is missing disappears (its windows are the core's to move),
    /// and one that stays takes the new area. The core hears of the new set on the next
    /// [`next_events`](Self::next_events). The ids must differ from each other.
    pub fn set_outputs(&mut self, outputs: &[Output]) {
        let handle = self.display.handle();
        let mut gone = Vec::new();
        self.screens.retain(|screen| {
            let kept = outputs.iter().any(|output| output.id == screen.id);
            if !kept {
                handle.remove_global::<State>(screen.global.clone());
                gone.push(screen.output.clone());
            }
            kept
        });
        self.state.leave_outputs(&gone);
        let scale = self.state.scale;
        for output in outputs {
            match self
                .screens
                .iter_mut()
                .find(|screen| screen.id == output.id)
            {
                Some(screen) => {
                    screen.rect = output.rect;
                    show_area(&screen.output, output.rect, scale);
                }
                None => {
                    let wl_output = WlOutput::new(
                        format!("{SOCKET_BASE}-{}", output.id.get()),
                        PhysicalProperties {
                            size: (0, 0).into(),
                            subpixel: Subpixel::Unknown,
                            make: "lutece".to_owned(),
                            model: "virtual".to_owned(),
                        },
                    );
                    show_area(&wl_output, output.rect, scale);
                    let global = wl_output.create_global::<State>(&handle);
                    self.screens.push(Screen {
                        id: output.id,
                        rect: output.rect,
                        output: wl_output,
                        global,
                    });
                }
            }
        }
        self.state.wl_outputs = outputs
            .iter()
            .filter_map(|output| {
                let screen = self.screens.iter().find(|screen| screen.id == output.id)?;
                Some((output.rect, screen.output.clone()))
            })
            .collect();
        self.state.refresh_all_presence();
        self.state.report_outputs(outputs.to_vec());
    }

    /// Sets the key combinations the core wants for itself. Clients never see them.
    pub fn grab_keys(&mut self, presses: impl IntoIterator<Item = KeyPress>) {
        self.state.grabs = presses.into_iter().collect();
    }

    /// Waits up to `timeout` for something to happen, then returns the events for the
    /// core. Returns at once if events are already waiting.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting on, or talking to, the clients fails.
    pub fn next_events(&mut self, timeout: Duration) -> Result<Vec<Event>> {
        if self.state.events.is_empty() {
            self.wait(timeout)?;
        }
        self.pump()?;
        if let Some(why) = self.state.xwayland_failure.take() {
            return Err(Error::XWayland(why));
        }
        Ok(std::mem::take(&mut self.state.events))
    }

    /// Feeds one key event from a real keyboard. `keycode` is the evdev code plus 8, as
    /// Wayland keyboards number keys. Returns the combination when the core took it, in
    /// which case the caller turns it into [`Event::KeyPressed`]; otherwise the focused
    /// client has already received the key.
    pub fn key_event(&mut self, keycode: u32, action: KeyAction, time: u32) -> Option<KeyPress> {
        let key_state = match action {
            KeyAction::Press => KeyState::Pressed,
            KeyAction::Release => KeyState::Released,
        };
        self.state
            .press_key(Keycode::from(keycode), key_state, time)
    }

    /// The pointer moved to `x`, `y` on the output. Entering a window asks the core to
    /// focus it, reported as [`Event::FocusRequested`] by the next
    /// [`next_events`](Self::next_events).
    pub fn pointer_motion(&mut self, x: f64, y: f64, time: u32) {
        self.state.pointer_motion(Point::from((x, y)), time);
    }

    /// Where the pointer is, in the coordinates of the output.
    pub(crate) fn pointer_location(&self) -> (f64, f64) {
        let location = self.state.pointer.location;
        (location.x, location.y)
    }

    /// A pointer button changed. `button` is a Linux input code, `0x110` for the left
    /// button. While the drag modifier is held, the left button starts a move and the
    /// right button a resize of the window under the pointer, reported as
    /// [`Event::FloatAdjusted`]; clients do not see those clicks.
    pub fn pointer_button(&mut self, button: u32, action: ButtonAction, time: u32) {
        self.state
            .pointer_button(button, action == ButtonAction::Press, time);
    }

    /// The wheel turned by `horizontal` and `vertical`; positive scrolls right and down.
    pub fn pointer_axis(&mut self, horizontal: f64, vertical: f64, time: u32) {
        self.state.pointer_axis(horizontal, vertical, time);
    }

    /// Sets the modifier that turns a left or right button press into a drag. It is
    /// [`Modifiers::ALT`] until set; [`Modifiers::NONE`] turns drags off.
    pub fn set_drag_modifier(&mut self, modifier: Modifiers) {
        self.state.pointer.drag_modifier = modifier;
    }

    /// Makes the clients match `scene`: sizes the windows it lists, focuses the focused
    /// one and records the stacking order.
    ///
    /// # Errors
    ///
    /// Returns an error if a client cannot be written to.
    pub fn apply_scene(&mut self, scene: &Scene) -> Result<()> {
        let changes = diff(&self.scene, scene);
        apply_changes(&mut self.state, &changes);
        self.scene = scene.clone();
        self.display.flush_clients()?;
        Ok(())
    }

    /// Carries out effects: closing windows, starting programs, releasing windows.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Spawn`] if a program cannot be started, or an I/O error if a
    /// client cannot be written to.
    pub fn apply_effects(&mut self, effects: &[Effect]) -> Result<Flow> {
        let socket = self.socket_name().to_owned();
        let flow = apply_effects(&mut self.state, &socket, effects)?;
        self.display.flush_clients()?;
        Ok(flow)
    }

    /// What to draw for the managed windows, front to back.
    pub(crate) fn surface_elements<R>(&self, renderer: &mut R) -> Vec<SceneElement<R>>
    where
        R: Renderer + ImportAll,
        R::TextureId: Clone + 'static,
    {
        crate::render::surface_elements(&self.state.windows, renderer, self.state.scale)
    }

    /// What to draw for a cursor image a client supplied, if it did.
    pub(crate) fn cursor_elements<R>(&self, renderer: &mut R) -> Vec<SceneElement<R>>
    where
        R: Renderer + ImportAll,
        R::TextureId: Clone + 'static,
    {
        crate::render::cursor_elements(
            &self.state.pointer.cursor,
            self.state.pointer.location,
            renderer,
            self.state.scale,
        )
    }

    /// The cursor image the focused client asked for.
    pub(crate) fn cursor(&self) -> &CursorImageStatus {
        &self.state.pointer.cursor
    }

    /// Tells clients a frame was shown, so they draw the next one.
    pub(crate) fn send_frame_callbacks(&mut self, time_ms: u32) -> Result<()> {
        crate::render::send_frame_callbacks(&self.state.windows, time_ms);
        self.display.flush_clients()?;
        Ok(())
    }

    fn wait(&mut self, timeout: Duration) -> Result<()> {
        let timeout = Timespec {
            tv_sec: i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX),
            tv_nsec: i64::from(timeout.subsec_nanos()),
        };
        let clients = self.display.backend().poll_fd().try_clone_to_owned()?;
        let mut fds = [
            PollFd::new(&self.listener, PollFlags::IN),
            PollFd::new(&clients, PollFlags::IN),
            PollFd::new(&self.event_loop, PollFlags::IN),
        ];
        poll(&mut fds, Some(&timeout)).map_err(io::Error::from)?;
        Ok(())
    }

    fn pump(&mut self) -> Result<()> {
        while let Some(stream) = self.listener.accept()? {
            self.display
                .handle()
                .insert_client(stream, Arc::new(ClientState::default()))?;
        }
        self.event_loop
            .dispatch(Some(Duration::ZERO), &mut self.state)
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.display.dispatch_clients(&mut self.state)?;
        self.state.popups.cleanup();
        self.display.flush_clients()?;
        Ok(())
    }
}

/// Gives an output the place of `rect` (logical pixels) and the mode that covers it at
/// `scale`, which is how many physical pixels the screen has for it.
fn show_area(output: &WlOutput, rect: Rect, scale: Scale) {
    let mode = Mode {
        size: (
            scale.to_physical(to_protocol(rect.width())),
            scale.to_physical(to_protocol(rect.height())),
        )
            .into(),
        refresh: REFRESH_MILLIHERTZ,
    };
    output.change_current_state(
        Some(mode),
        None,
        Some(OutputScale::Fractional(scale.factor())),
        Some((rect.x(), rect.y()).into()),
    );
    output.set_preferred(mode);
}

/// Protocol sizes are signed; a size beyond `i32` is clamped, far past any real screen.
fn to_protocol(length: u32) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

impl From<smithay::input::keyboard::Error> for Error {
    fn from(error: smithay::input::keyboard::Error) -> Self {
        Self::Keyboard(error)
    }
}
