//! The compositor as the session of a login: it takes a screen, the keyboard and the pointer
//! from the machine and serves clients on them.
//!
//! It is the hardware twin of the nested backend: the same clients, the same scene, the same
//! calls, with a real screen and real devices in place of a window. The screen is one
//! connected monitor at its best mode; the seat and the devices come from `libseat` and
//! `libinput`, so it must be started from a virtual terminal, not from inside another
//! session.
//!
//! What is not done: a second monitor, a monitor plugged or unplugged while it runs, and the
//! outputs' own scale.
//!
//! A frame is drawn only when something may have changed (see [`Redraw`]), and at least once a
//! second whatever the signals say, so a change nobody reported shows up late and is never
//! lost. Settings for the keyboard's repeat and for touchpads and mice come from
//! [`InputSettings`](crate::InputSettings).
//!
//! Getting out is never left to the window manager: see [`super::keys`].
//!
//! Started by hand, it has never been run on a screen; see `docs/live-check.md` in the window
//! manager for what to look at first.

use super::cursor;
use super::keys::{Chord, Chords, Verdict};
use super::pattern::{COLOR_FORMATS, Target, find_target, hardware, make_renderer};
use crate::apply::Flow;
use crate::backend::WaylandBackend;
use crate::dmabuf::DmabufSupport;
use crate::error::{Error, Result};
use crate::input::{self, Input};
use crate::look::Shaders;
use crate::outline::{self, Outline};
use crate::render::SceneElement;
use crate::scale::Scale;
use crate::settings::InputSettings;
use crate::title::TitleCache;
use lutece::{Effect, Event, KeyPress, Modifiers, Output, OutputId, Rect, Scene, Size};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, GbmBufferedSurface};
use smithay::backend::input::{InputEvent, KeyState};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::memory::{
    MemoryRenderBuffer, MemoryRenderBufferRenderElement,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{Bind, Color32F, Frame, ImportDma, Renderer};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::primary_gpu;
use smithay::input::pointer::CursorImageStatus;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::input::{Device, Libinput};
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Physical, Rectangle, Transform};
use std::ffi::OsStr;
use std::time::{Duration, Instant};

const BACKGROUND: Color32F = Color32F::new(0.10, 0.12, 0.16, 1.0);
/// Devices and clients are watched in turns, so waiting on the clients for long would leave
/// the keyboard deaf. This bounds the latency of a key press.
const WAIT_SLICE: Duration = Duration::from_millis(8);
/// The longest the screen goes without a new frame in [`Redraw::OnChange`]. It bounds how long
/// a change that nothing reported stays unseen.
const BACKSTOP: Duration = Duration::from_secs(1);

/// When the screen is drawn again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Redraw {
    /// Only after something may have changed: a client committed, the scene changed, a window
    /// is moving, the pointer or a key was used. Never less than once a second.
    #[default]
    OnChange,
    /// Every refresh, whether anything changed or not.
    Always,
}

/// What the event sources leave for the backend to act on once they have been drained.
struct Shared {
    /// What the keyboard and the pointer did, not yet applied.
    inputs: Vec<Input>,
    /// Pauses and resumes of the session: a switch away from this virtual terminal and back.
    session: Vec<SessionEvent>,
    /// Page flips that landed.
    flips: u32,
    /// The first thing that went wrong in a source, to be reported by the next call.
    failure: Option<Error>,
    /// The area a pointer position is spread over, in logical pixels.
    bounds: (i32, i32),
    /// The settings new devices get, and the ones that are set again when they change.
    settings: InputSettings,
    /// The devices the seat has now, to set again when the settings change.
    devices: Vec<Device>,
}

/// A [`WaylandBackend`] shown on a real screen, with the machine's own keyboard and pointer.
///
/// Drive it like the nested backend: collect events with [`next_events`](Self::next_events),
/// show the scene, carry out the effects, and draw with [`render`](Self::render). Stop when
/// [`is_closed`](Self::is_closed) is true.
///
/// Fields are dropped in order, so the seat, which the devices were opened through, goes
/// last.
pub struct DrmBackend {
    wayland: WaylandBackend,
    shaders: Shaders,
    titles: TitleCache,
    /// The pointer's arrow and its size in pixels.
    pointer: (MemoryRenderBuffer, (i32, i32)),
    renderer: GlesRenderer,
    surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    device: DrmDevice,
    libinput: Libinput,
    event_loop: EventLoop<'static, Shared>,
    shared: Shared,
    session: LibSeatSession,
    size: smithay::utils::Size<i32, Physical>,
    scale: Scale,
    started: Instant,
    closed: bool,
    /// Whether this virtual terminal is the one on the screen. Nothing is drawn or read when
    /// it is not.
    active: bool,
    /// A frame was queued and its flip has not landed; the next frame waits for it.
    flip_pending: bool,
    redraw: Redraw,
    /// When the last frame was queued, for the backstop.
    last_drawn: Instant,
    chords: Chords,
    outlines: Vec<Outline>,
    read_only: bool,
    /// Where the connected screen is, for the one line that says what was opened.
    description: String,
}

impl DrmBackend {
    /// Takes the seat, the primary GPU, the first connected screen and the input devices,
    /// and starts listening for clients.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Drm`], saying which step failed, when there is no seat (this was not
    /// started from a virtual terminal), no GPU, no connected screen with a mode, or the
    /// buffers or the input devices cannot be had; or another error if the compositor cannot
    /// start.
    pub fn open() -> Result<Self> {
        let event_loop: EventLoop<'static, Shared> =
            EventLoop::try_new().map_err(hardware("event loop"))?;
        let handle = event_loop.handle();
        let (mut session, session_notifier) =
            LibSeatSession::new().map_err(hardware("seat (run this from a virtual terminal)"))?;
        handle
            .insert_source(session_notifier, |event, (), shared: &mut Shared| {
                shared.session.push(event);
            })
            .map_err(hardware("seat events"))?;

        let path = primary_gpu(session.seat())
            .map_err(hardware("graphics device lookup"))?
            .ok_or_else(|| Error::Drm("no graphics device on this seat".to_owned()))?;
        let fd = session
            .open(
                &path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .map_err(hardware("opening the graphics device"))?;
        let fd = DrmDeviceFd::new(DeviceFd::from(fd));
        let (mut device, device_notifier) =
            DrmDevice::new(fd.clone(), true).map_err(hardware("graphics device"))?;
        handle
            .insert_source(
                device_notifier,
                |event, _, shared: &mut Shared| match event {
                    DrmEvent::VBlank(_) => shared.flips += 1,
                    DrmEvent::Error(error) => {
                        shared.failure.get_or_insert(hardware("page flip")(error));
                    }
                },
            )
            .map_err(hardware("graphics events"))?;

        let target = find_target(&device)?;
        let drm_surface = device
            .create_surface(target.crtc, target.mode, &[target.connector])
            .map_err(hardware("screen surface"))?;
        let gbm = GbmDevice::new(fd).map_err(hardware("buffer allocator"))?;
        let mut renderer = make_renderer(gbm.clone())?;
        let formats = renderer
            .dmabuf_formats()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
        let surface = GbmBufferedSurface::new(drm_surface, allocator, &COLOR_FORMATS, formats)
            .map_err(hardware("screen buffers"))?;

        let libinput = open_input(&session, &handle)?;

        let Target {
            name,
            size: [width, height],
            refresh_hz,
            ..
        } = target;
        let scale = Scale::default();
        let logical = Size::new(u32::from(width), u32::from(height))?;
        let mut wayland = WaylandBackend::bind(logical)?;
        wayland.set_scale(scale);
        wayland.set_outputs(&[Output {
            id: OutputId::new(0),
            rect: Rect::new(0, 0, logical.width(), logical.height())?,
        }]);
        let shaders = Shaders::compile(&mut renderer).map_err(hardware("shaders"))?;
        if let Some(support) = DmabufSupport::of(&renderer) {
            wayland.enable_dmabuf(&support)?;
        }

        Ok(Self {
            wayland,
            shaders,
            titles: TitleCache::new(),
            pointer: cursor::sprite(),
            renderer,
            surface,
            device,
            libinput,
            event_loop,
            shared: Shared {
                inputs: Vec::new(),
                session: Vec::new(),
                flips: 0,
                failure: None,
                bounds: (i32::from(width), i32::from(height)),
                settings: InputSettings::default(),
                devices: Vec::new(),
            },
            session,
            size: smithay::utils::Size::from((i32::from(width), i32::from(height))),
            scale,
            started: Instant::now(),
            closed: false,
            active: true,
            flip_pending: false,
            redraw: Redraw::default(),
            last_drawn: Instant::now(),
            chords: Chords::default(),
            outlines: Vec::new(),
            read_only: false,
            description: format!("{name} at {width}x{height} {refresh_hz} Hz"),
        })
    }

    /// The screen that was opened, for example `HDMIA-1 at 1920x1080 60 Hz`.
    #[must_use]
    pub fn describe(&self) -> &str {
        &self.description
    }

    /// Sets the outlines drawn over the windows, replacing the ones before; an empty list takes
    /// them away. They are only drawn: no window moves and no client is told.
    pub fn set_outlines(&mut self, outlines: Vec<Outline>) {
        self.outlines = outlines;
        self.wayland.mark_damage();
    }

    /// Sets when the screen is drawn again; see [`Redraw`].
    pub fn set_redraw(&mut self, redraw: Redraw) {
        self.redraw = redraw;
        self.wayland.mark_damage();
    }

    /// Sets how the keyboard repeats and how the touchpads and mice behave, on the devices
    /// there are and on the ones plugged in later. A setting a device does not have, such as
    /// tap to click on a mouse, leaves that device as it is.
    pub fn set_input_settings(&mut self, settings: InputSettings) {
        self.wayland.set_repeat(&settings);
        self.shared.settings = settings;
        for device in &mut self.shared.devices {
            apply_device(device, &settings);
        }
    }

    /// Makes the keyboard and the pointer do nothing, or take them again. While read-only, no
    /// key reaches the core or a client and the pointer stays where it is; only the release of
    /// a key or button already down gets through. The chords that end the session or switch
    /// terminal still work, so a view that holds the input can always be left.
    pub const fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// Whether the input is held back.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// The name clients put in `WAYLAND_DISPLAY` to reach this compositor.
    #[must_use]
    pub fn socket_name(&self) -> &OsStr {
        self.wayland.socket_name()
    }

    /// Whether the session was ended, by Ctrl+Alt+Backspace. The loop should end.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Sets the key combinations the core wants for itself.
    pub fn grab_keys(&mut self, presses: impl IntoIterator<Item = KeyPress>) {
        self.wayland.grab_keys(presses);
    }

    /// Sets the modifier that turns a left or right button press into a drag of the window
    /// under the pointer; see [`WaylandBackend::set_drag_modifier`].
    pub fn set_drag_modifier(&mut self, modifier: Modifiers) {
        self.wayland.set_drag_modifier(modifier);
    }

    /// Lets X11 programs run; see [`WaylandBackend::enable_xwayland`].
    ///
    /// # Errors
    ///
    /// Returns an error if the `Xwayland` program cannot be started.
    pub fn enable_xwayland(&mut self) -> Result<()> {
        self.wayland.enable_xwayland()
    }

    /// The X11 display number of Xwayland, once it is ready.
    #[must_use]
    pub fn xwayland_display(&self) -> Option<u32> {
        self.wayland.xwayland_display()
    }

    /// Collects what the devices and the clients did. A grabbed key press arrives as
    /// [`Event::KeyPressed`]; every other key goes to the focused client.
    ///
    /// # Errors
    ///
    /// Returns an error if the screen or a device reports a failure, or talking to the
    /// clients fails.
    pub fn next_events(&mut self, timeout: Duration) -> Result<Vec<Event>> {
        self.event_loop
            .dispatch(Some(Duration::ZERO), &mut self.shared)
            .map_err(hardware("waiting for the devices"))?;
        if let Some(failure) = self.shared.failure.take() {
            return Err(failure);
        }
        self.follow_session()?;
        self.follow_flips()?;

        let bounds = (
            f64::from(self.shared.bounds.0),
            f64::from(self.shared.bounds.1),
        );
        let mut events = Vec::new();
        for input in std::mem::take(&mut self.shared.inputs) {
            if !self.active {
                continue;
            }
            let Some(input) = self.screen_chords(input) else {
                continue;
            };
            if self.read_only {
                input::apply_read_only(&mut self.wayland, input);
            } else {
                // The pointer's arrow, or a client's answer to a key, may now look different.
                self.wayland.mark_damage();
                if let Some(event) = input::apply(&mut self.wayland, input, bounds) {
                    events.push(event);
                }
            }
        }
        events.extend(self.wayland.next_events(timeout.min(WAIT_SLICE))?);
        if !events.is_empty() {
            self.wayland.mark_damage();
        }
        Ok(events)
    }

    /// Makes the clients match `scene`.
    ///
    /// # Errors
    ///
    /// Returns an error if a client cannot be written to.
    pub fn apply_scene(&mut self, scene: &Scene) -> Result<()> {
        self.wayland.apply_scene(scene)
    }

    /// Carries out effects: closing windows, starting programs, releasing windows.
    ///
    /// # Errors
    ///
    /// Returns an error if a program cannot be started or a client cannot be written to.
    pub fn apply_effects(&mut self, effects: &[Effect]) -> Result<Flow> {
        self.wayland.apply_effects(effects)
    }

    /// Draws a frame on the screen, unless the last one has not been shown yet or this
    /// terminal is not the one on the screen; then it does nothing and the next call tries
    /// again.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Render`] or [`Error::Drm`] if the frame cannot be drawn or queued.
    pub fn render(&mut self) -> Result<()> {
        if !self.active || self.flip_pending {
            return Ok(());
        }
        let now = Instant::now();
        if !self.frame_due(now) {
            return Ok(());
        }
        let size = self.size;
        let whole = [Rectangle::from_size(size)];

        // Front to back: outlines, the pointer, a client's own pointer image, then windows.
        let mut elements = outline::elements(&self.outlines, &self.shaders, self.scale);
        elements.extend(self.pointer_elements());
        elements.extend(self.wayland.cursor_elements(&mut self.renderer));
        // The buffers are scanned out with the first row at the top, which is how the
        // elements are laid out, so nothing is drawn flipped.
        elements.extend(self.wayland.surface_elements(
            &mut self.renderer,
            &self.shaders,
            &mut self.titles,
            size,
            false,
        ));

        let (mut buffer, _age) = self
            .surface
            .next_buffer()
            .map_err(hardware("next buffer"))?;
        let sync = {
            let mut target = self
                .renderer
                .bind(&mut buffer)
                .map_err(hardware("binding the buffer"))?;
            let mut frame = self
                .renderer
                .render(&mut target, size, Transform::Normal)
                .map_err(hardware("starting a frame"))?;
            frame
                .clear(BACKGROUND, &whole)
                .map_err(hardware("clearing the frame"))?;
            draw_render_elements::<GlesRenderer, _, _>(
                &mut frame,
                self.scale.factor(),
                &elements,
                &whole,
            )
            .map_err(hardware("drawing the frame"))?;
            frame.finish().map_err(hardware("finishing the frame"))?
        };
        self.surface
            .queue_buffer(Some(sync), None, ())
            .map_err(hardware("queueing the frame"))?;
        self.flip_pending = true;
        self.wayland.clear_damage();
        self.last_drawn = now;

        // Milliseconds wrap after about 49 days, which the protocol allows.
        let time_ms = u32::try_from(self.started.elapsed().as_millis() % u128::from(u32::MAX))
            .unwrap_or_default();
        self.wayland.send_frame_callbacks(time_ms)
    }
}

impl DrmBackend {
    /// Whether a frame should be drawn at `now`; see [`frame_due`].
    fn frame_due(&self, now: Instant) -> bool {
        frame_due(
            self.redraw,
            self.wayland.needs_frame(now),
            now.duration_since(self.last_drawn),
        )
    }

    /// The arrow, where the pointer is, when the focused client wants a named cursor. A client
    /// that draws its own is drawn with the windows, and one that wants none gets none.
    fn pointer_elements(&mut self) -> Option<SceneElement> {
        if !matches!(self.wayland.cursor(), CursorImageStatus::Named(_)) {
            return None;
        }
        let (x, y) = self.wayland.pointer_location();
        let (buffer, size) = &self.pointer;
        // A pointer sprite that cannot be uploaded leaves no pointer; it must not cost the
        // frame.
        MemoryRenderBufferRenderElement::from_buffer(
            &mut self.renderer,
            (x * self.scale.factor(), y * self.scale.factor()),
            buffer,
            None,
            None,
            Some((*size).into()),
            Kind::Cursor,
        )
        .ok()
        .map(SceneElement::Picture)
    }

    /// Handles the chords the session keeps for itself, and returns the input if it is not one.
    fn screen_chords(&mut self, input: Input) -> Option<Input> {
        let Input::Key { code, state, .. } = &input else {
            return Some(input);
        };
        match self.chords.key(*code, *state == KeyState::Pressed) {
            Verdict::Pass => Some(input),
            Verdict::Swallow => None,
            Verdict::Chord(Chord::Quit) => {
                self.closed = true;
                None
            }
            Verdict::Chord(Chord::SwitchVt(vt)) => {
                // A terminal that does not exist is refused by the seat; the screen stays.
                let _ = self.session.change_vt(vt);
                None
            }
        }
    }

    /// Acts on a switch away from this virtual terminal and back.
    fn follow_session(&mut self) -> Result<()> {
        for event in std::mem::take(&mut self.shared.session) {
            match event {
                SessionEvent::PauseSession => {
                    self.libinput.suspend();
                    self.device.pause();
                    self.chords.reset();
                    self.active = false;
                }
                SessionEvent::ActivateSession => {
                    self.libinput
                        .resume()
                        .map_err(|()| Error::Drm("input devices: cannot resume".to_owned()))?;
                    self.device
                        .activate(true)
                        .map_err(hardware("taking the screen back"))?;
                    // Whatever was queued before the switch will never land.
                    self.surface.reset_buffers();
                    self.flip_pending = false;
                    self.active = true;
                    self.wayland.mark_damage();
                }
            }
        }
        Ok(())
    }

    /// Notes the flips that landed, so the next frame can be drawn.
    fn follow_flips(&mut self) -> Result<()> {
        for _ in 0..std::mem::take(&mut self.shared.flips) {
            // `None` is the flip of the mode setting itself, which no frame of ours is waiting on.
            if self
                .surface
                .frame_submitted()
                .map_err(hardware("finishing a flip"))?
                .is_some()
            {
                self.flip_pending = false;
            }
        }
        Ok(())
    }
}

/// Opens the keyboard, pointer and other devices of the seat and has the loop read them.
fn open_input(
    session: &LibSeatSession,
    handle: &smithay::reexports::calloop::LoopHandle<'static, Shared>,
) -> Result<Libinput> {
    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    libinput
        .udev_assign_seat(&session.seat())
        .map_err(|()| Error::Drm("input devices: cannot take the seat's devices".to_owned()))?;
    handle
        .insert_source(
            LibinputInputBackend::new(libinput.clone()),
            |event, (), shared: &mut Shared| {
                match &event {
                    InputEvent::DeviceAdded { device } => {
                        let mut device = device.clone();
                        apply_device(&mut device, &shared.settings);
                        shared.devices.push(device);
                    }
                    InputEvent::DeviceRemoved { device } => {
                        shared.devices.retain(|known| known != device);
                    }
                    _ => {}
                }
                let (width, height) = shared.bounds;
                if let Some(input) = input::collect(event, width, height) {
                    shared.inputs.push(input);
                }
            },
        )
        .map_err(hardware("input events"))?;
    Ok(libinput)
}

/// Whether a frame is due: always in [`Redraw::Always`]; otherwise when `changed` (something
/// may look different, or a window is moving) or when `idle`, the time since the last frame,
/// has reached the backstop. Pure, so the rule that decides what the screen shows can be
/// tested without a screen.
fn frame_due(redraw: Redraw, changed: bool, idle: Duration) -> bool {
    match redraw {
        Redraw::Always => true,
        Redraw::OnChange => changed || idle >= BACKSTOP,
    }
}

/// Applies the device settings that `device` has. A setting the device lacks is skipped, and
/// one libinput refuses leaves the device as it was: there is no better answer for a mouse
/// that cannot tap, and nothing here can tell the user of a refusal.
fn apply_device(device: &mut Device, settings: &InputSettings) {
    if let Some(on) = settings.tap_to_click()
        && device.config_tap_finger_count() > 0
    {
        let _ = device.config_tap_set_enabled(on);
    }
    if let Some(on) = settings.natural_scroll()
        && device.config_scroll_has_natural_scroll()
    {
        let _ = device.config_scroll_set_natural_scroll_enabled(on);
    }
    if let Some(speed) = settings.pointer_speed()
        && device.config_accel_is_available()
    {
        let _ = device.config_accel_set_speed(speed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn always_draws_whatever_the_signals_say(changed: bool, idle_ms in 0u64..10_000) {
            prop_assert!(frame_due(Redraw::Always, changed, Duration::from_millis(idle_ms)));
        }

        #[test]
        fn a_change_is_drawn_at_once(idle_ms in 0u64..10_000) {
            prop_assert!(frame_due(Redraw::OnChange, true, Duration::from_millis(idle_ms)));
        }

        #[test]
        fn nothing_waits_longer_than_the_backstop_for_a_frame(changed: bool) {
            prop_assert!(frame_due(Redraw::OnChange, changed, BACKSTOP));
        }

        #[test]
        fn a_quiet_screen_is_left_alone_before_the_backstop(idle_ms in 0u64..1000) {
            prop_assert!(!frame_due(Redraw::OnChange, false, Duration::from_millis(idle_ms)));
        }
    }
}
