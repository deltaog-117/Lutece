//! The compositor as the session of a login: it takes a screen, the keyboard and the pointer
//! from the machine and serves clients on them.
//!
//! It is the hardware twin of the nested backend: the same clients, the same scene, the same
//! calls, with real screens and real devices in place of a window. Every connected monitor
//! runs at its best mode and is placed left to right, each its own output with its own
//! scale (see [`ScreenSettings`]); the seat and the devices come from `libseat` and
//! `libinput`, so it must be started from a virtual terminal, not from inside another
//! session.
//!
//! A monitor plugged or unplugged while it runs is noticed: the device reports a change, and
//! once it has been quiet for a moment the connected screens are read again (see
//! [`DrmBackend::next_events`]). A screen that stays keeps its output number, one that is gone
//! is a missing output the core moves its windows off, and one that returns gets its old
//! number back, so it shows the workspace it showed.
//!
//! What is not done: a screen turned or placed anywhere but in a row, a change of a screen's
//! mode while it stays connected, and a sharp picture for the X11 programs of Xwayland, which
//! live in logical pixels and are stretched to the screen (see the X11 tests).
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

use super::arrange::{ScreenSettings, arrange, order, plan_screens, remember};
use super::cursor;
use super::keys::{Chord, Chords, Verdict};
use super::pattern::{COLOR_FORMATS, Target, find_targets, hardware, make_renderer, read_targets};
use crate::apply::Flow;
use crate::backend::WaylandBackend;
use crate::dmabuf::DmabufSupport;
use crate::error::{Error, Result};
use crate::input::{self, Area, Input};
use crate::look::Shaders;
use crate::outline::{self, Outline};
use crate::render::{SceneElement, View};
use crate::scale::Scale;
use crate::settings::InputSettings;
use crate::title::TitleCache;
use lutece::{Effect, Event, KeyPress, Modifiers, Output, OutputId, Rect, Scene};
use smithay::backend::allocator::Format as DrmFormat;
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
use smithay::backend::udev::{UdevBackend, UdevEvent, primary_gpu};
use smithay::input::pointer::CursorImageStatus;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::drm::control::crtc;
use smithay::reexports::input::{Device, Libinput};
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Physical, Point, Rectangle, Transform};
use std::ffi::OsStr;
use std::time::{Duration, Instant};

const BACKGROUND: Color32F = Color32F::new(0.10, 0.12, 0.16, 1.0);
/// Devices and clients are watched in turns, so waiting on the clients for long would leave
/// the keyboard deaf. This bounds the latency of a key press.
const WAIT_SLICE: Duration = Duration::from_millis(8);
/// The longest the screen goes without a new frame in [`Redraw::OnChange`]. It bounds how long
/// a change that nothing reported stays unseen.
const BACKSTOP: Duration = Duration::from_secs(1);
/// How long the device is left quiet after it reports a change before the screens are read
/// again. Plugging a monitor sends several reports in a row, and its modes are not all ready
/// at the first.
const RESCAN_DELAY: Duration = Duration::from_millis(300);

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
    /// The screens whose page flips landed, one entry for each flip.
    flips: Vec<crtc::Handle>,
    /// The first thing that went wrong in a source, to be reported by the next call.
    failure: Option<Error>,
    /// The box around every screen, which an absolute pointer position is spread over, in
    /// logical pixels.
    bounds: (i32, i32),
    /// The settings new devices get, and the ones that are set again when they change.
    settings: InputSettings,
    /// The devices the seat has now, to set again when the settings change.
    devices: Vec<Device>,
    /// When to read the connected screens again, after the device reported a change.
    rescan_at: Option<Instant>,
}

/// One connected screen: the surface its pictures go to and what it shows of the space.
struct Head {
    /// The connector, for example `HDMIA-1`.
    name: String,
    /// The number of its output, which it keeps for as long as it is connected and gets
    /// back when it returns.
    id: OutputId,
    /// What was opened, for the line that says so.
    line: String,
    crtc: crtc::Handle,
    surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    /// The picture's size in pixels.
    size: smithay::utils::Size<i32, Physical>,
    scale: Scale,
    /// The part of the space it shows, in logical pixels.
    rect: Rect,
    /// A frame was queued and its flip has not landed; the next frame waits for it.
    flip_pending: bool,
    /// The scene changed since this screen was last drawn.
    stale: bool,
}

impl Head {
    /// What this screen shows of the space.
    fn view(&self) -> View {
        View {
            origin: Point::from((self.rect.x(), self.rect.y())),
            scale: self.scale,
        }
    }
}

/// A [`WaylandBackend`] shown on the machine's real screens, with its own keyboard and pointer.
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
    /// Every connected screen, in the order they are laid out. Never empty.
    heads: Vec<Head>,
    /// What the user said about the order and the scale of the screens, kept to apply to
    /// screens that come and go.
    settings: ScreenSettings,
    /// The output number of every screen seen so far, so one that returns gets its own back.
    known: Vec<(String, OutputId)>,
    /// What screens are made on, kept for the ones that are plugged in later.
    gbm: GbmDevice<DrmDeviceFd>,
    formats: Vec<DrmFormat>,
    /// The same screens as the pointer sees them.
    areas: Vec<Area>,
    device: DrmDevice,
    libinput: Libinput,
    event_loop: EventLoop<'static, Shared>,
    shared: Shared,
    session: LibSeatSession,
    started: Instant,
    closed: bool,
    /// Whether this virtual terminal is the one on the screen. Nothing is drawn or read when
    /// it is not.
    active: bool,
    redraw: Redraw,
    /// When the last frame was queued, for the backstop.
    last_drawn: Instant,
    chords: Chords,
    outlines: Vec<Outline>,
    read_only: bool,
    /// What was opened, for the one line that says so.
    description: String,
}

impl DrmBackend {
    /// Takes the seat, the primary GPU, every connected screen and the input devices, and
    /// starts listening for clients. The screens are in the machine's order and at a scale of
    /// one; [`open_with`](Self::open_with) says otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Drm`], saying which step failed, when there is no seat (this was not
    /// started from a virtual terminal), no GPU, no connected screen with a mode, or the
    /// buffers or the input devices cannot be had; or another error if the compositor cannot
    /// start.
    pub fn open() -> Result<Self> {
        Self::open_with(&ScreenSettings::default())
    }

    /// Like [`open`](Self::open), with `settings` for the order and the scale of the screens.
    /// The screens are read once, here: one plugged or unplugged later is not noticed.
    ///
    /// Each screen is an output of its own, numbered from 0 in the order laid out, so the
    /// first is the one the core shows its first workspace on. Windows are told the scale of
    /// the screen they are mostly on, and each screen draws at its own.
    ///
    /// # Errors
    ///
    /// The same as [`open`](Self::open).
    pub fn open_with(settings: &ScreenSettings) -> Result<Self> {
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
                    DrmEvent::VBlank(crtc) => shared.flips.push(crtc),
                    DrmEvent::Error(error) => {
                        shared.failure.get_or_insert(hardware("page flip")(error));
                    }
                },
            )
            .map_err(hardware("graphics events"))?;

        let device_id = device.device_id();
        let udev = UdevBackend::new(session.seat()).map_err(hardware("device watch"))?;
        handle
            .insert_source(udev, move |event, (), shared: &mut Shared| {
                // A monitor plugged or unplugged is a change of the graphics device itself.
                if let UdevEvent::Changed { device_id: changed } = event
                    && changed == device_id
                {
                    shared.rescan_at = Some(Instant::now() + RESCAN_DELAY);
                }
            })
            .map_err(hardware("device events"))?;

        let mut targets = find_targets(&device)?;
        let sequence = {
            let names: Vec<&str> = targets.iter().map(|target| target.name.as_str()).collect();
            order(&names, settings)
        };
        let mut listed: Vec<Option<Target>> = targets.drain(..).map(Some).collect();
        let targets: Vec<Target> = sequence
            .into_iter()
            .filter_map(|index| listed[index].take())
            .collect();
        let scales: Vec<Scale> = targets
            .iter()
            .map(|target| settings.scale_of(&target.name).unwrap_or_default())
            .collect();
        let layout = arrange(
            &targets
                .iter()
                .zip(&scales)
                .map(|(target, scale)| (target.size, *scale))
                .collect::<Vec<_>>(),
        )?;

        let gbm = GbmDevice::new(fd).map_err(hardware("buffer allocator"))?;
        let mut renderer = make_renderer(gbm.clone())?;
        let formats = renderer
            .dmabuf_formats()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let mut heads = Vec::with_capacity(targets.len());
        for (((target, scale), rect), number) in
            targets.into_iter().zip(scales).zip(layout).zip(0u32..)
        {
            heads.push(open_head(
                &mut device,
                &gbm,
                &formats,
                target,
                (OutputId::new(number), scale, rect),
            )?);
        }
        // A name the user gave that no connected screen has: said, not an error, so one
        // configuration serves a laptop on and off its dock.
        let missing: Vec<&str> = settings
            .names()
            .filter(|name| heads.iter().all(|head| head.name != *name))
            .collect();
        let mut description = describe_heads(&heads);
        if !missing.is_empty() {
            description.push_str(&format!(
                "; no connected screen is named {}",
                missing.join(", ")
            ));
        }

        let libinput = open_input(&session, &handle)?;

        let (outputs, areas, bounds) = geometry(&heads);

        // `bind` makes the one output the compositor starts with; the real set replaces it.
        let mut wayland = WaylandBackend::bind(heads[0].rect.size())?;
        for head in &heads {
            wayland.set_output_scale(head.id, Some(head.scale));
        }
        wayland.set_outputs(&outputs);
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
            known: heads
                .iter()
                .map(|head| (head.name.clone(), head.id))
                .collect(),
            heads,
            settings: settings.clone(),
            gbm,
            formats,
            areas,
            device,
            libinput,
            event_loop,
            shared: Shared {
                inputs: Vec::new(),
                session: Vec::new(),
                flips: Vec::new(),
                failure: None,
                bounds,
                settings: InputSettings::default(),
                devices: Vec::new(),
                rescan_at: None,
            },
            session,
            started: Instant::now(),
            closed: false,
            active: true,
            redraw: Redraw::default(),
            last_drawn: Instant::now(),
            chords: Chords::default(),
            outlines: Vec::new(),
            read_only: false,
            description,
        })
    }

    /// The screens that were opened, for example `HDMIA-1 at 1920x1080 60 Hz, eDP-1 at
    /// 2560x1600 60 Hz scale 1.5`, in the order they are laid out, left to right. When the
    /// settings named a screen that is not connected, it is said after a semicolon.
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
        self.follow_screens()?;

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
                if let Some(event) = input::apply(&mut self.wayland, input, &self.areas) {
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

    /// Draws a frame on every screen whose last one has been shown, unless this terminal is
    /// not the one on the screen; then it does nothing and the next call tries again.
    ///
    /// A screen still waiting for its last flip is left alone and drawn on a later call, so a
    /// slow or switched-off monitor does not hold the others back.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Render`] or [`Error::Drm`] if a frame cannot be drawn or queued.
    pub fn render(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        let now = Instant::now();
        if self.frame_due(now) {
            // The change is handed to the screens: each draws it when its turn comes, and
            // the damage is spent.
            for head in &mut self.heads {
                head.stale = true;
            }
            self.wayland.clear_damage();
            self.last_drawn = now;
        }
        let mut drew = false;
        for index in 0..self.heads.len() {
            if self.heads[index].stale && !self.heads[index].flip_pending {
                self.draw(index)?;
                drew = true;
            }
        }
        if !drew {
            return Ok(());
        }

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

    /// Draws one screen: the outlines, the pointer and the windows as that screen shows them,
    /// and queues the picture for its next flip.
    fn draw(&mut self, index: usize) -> Result<()> {
        let view = self.heads[index].view();
        let size = self.heads[index].size;
        let whole = [Rectangle::from_size(size)];

        // Front to back: outlines, the pointer, a client's own pointer image, then windows.
        // A screen draws all of them; what lies outside it falls outside the frame.
        let mut elements = outline::elements(&self.outlines, &self.shaders, view);
        elements.extend(self.pointer_elements(view));
        elements.extend(self.wayland.cursor_elements(&mut self.renderer, view));
        // The buffers are scanned out with the first row at the top, which is how the
        // elements are laid out, so nothing is drawn flipped.
        elements.extend(self.wayland.surface_elements(
            &mut self.renderer,
            &self.shaders,
            &mut self.titles,
            size,
            false,
            view,
        ));

        let head = &mut self.heads[index];
        let (mut buffer, _age) = head
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
                view.scale.factor(),
                &elements,
                &whole,
            )
            .map_err(hardware("drawing the frame"))?;
            frame.finish().map_err(hardware("finishing the frame"))?
        };
        head.surface
            .queue_buffer(Some(sync), None, ())
            .map_err(hardware("queueing the frame"))?;
        head.flip_pending = true;
        head.stale = false;
        Ok(())
    }

    /// The arrow, where the pointer is on the screen `view` describes, when the focused
    /// client wants a named cursor. A client that draws its own is drawn with the windows,
    /// and one that wants none gets none.
    fn pointer_elements(&mut self, view: View) -> Option<SceneElement> {
        if !matches!(self.wayland.cursor(), CursorImageStatus::Named(_)) {
            return None;
        }
        let (x, y) = self.wayland.pointer_location();
        let (buffer, size) = &self.pointer;
        let factor = view.scale.factor();
        // A pointer sprite that cannot be uploaded leaves no pointer; it must not cost the
        // frame.
        MemoryRenderBufferRenderElement::from_buffer(
            &mut self.renderer,
            (
                (x - f64::from(view.origin.x)) * factor,
                (y - f64::from(view.origin.y)) * factor,
            ),
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
                    for head in &mut self.heads {
                        head.surface.reset_buffers();
                        head.flip_pending = false;
                        head.stale = true;
                    }
                    self.active = true;
                    // A monitor may have come or gone while this terminal was not the one on
                    // the screen, with nobody here to hear the device say so.
                    self.shared.rescan_at = Some(Instant::now());
                    self.wayland.mark_damage();
                }
            }
        }
        Ok(())
    }

    /// Reads the screens again once the device has been quiet for a moment after reporting a
    /// change. Waits while this terminal is not the one on the screen.
    fn follow_screens(&mut self) -> Result<()> {
        let Some(at) = self.shared.rescan_at else {
            return Ok(());
        };
        if !self.active || Instant::now() < at {
            return Ok(());
        }
        self.shared.rescan_at = None;
        self.rescan()
    }

    /// Brings the screens shown in line with the screens connected: closes the ones that are
    /// gone, opens the ones that appeared, lays all of them out again and tells the core and
    /// the clients.
    ///
    /// A problem in reading the screens, or in a screen that cannot be opened, is said on the
    /// standard error and the session goes on with what it has: a monitor that does not work
    /// must not end the session. Only the end of every screen is an error, since there would
    /// be nothing left to show.
    fn rescan(&mut self) -> Result<()> {
        let pinned: Vec<(String, crtc::Handle)> = self
            .heads
            .iter()
            .map(|head| (head.name.clone(), head.crtc))
            .collect();
        let mut targets = match read_targets(&self.device, &pinned) {
            Ok(targets) => targets,
            Err(error) => {
                eprintln!("lutece: the screens could not be read again: {error}");
                return Ok(());
            }
        };
        if targets.is_empty() {
            // Every monitor gone at once, a laptop with its lid shut among them: the picture
            // stays on the last screens, which is more useful than no output at all.
            eprintln!("lutece: no screen is connected, the ones shown stay");
            return Ok(());
        }
        let shown: Vec<String> = self.heads.iter().map(|head| head.name.clone()).collect();
        let found: Vec<String> = targets.iter().map(|target| target.name.clone()).collect();
        let plan = plan_screens(&self.known, &shown, &found, &self.settings);
        if plan.is_unchanged(&shown) {
            return Ok(());
        }

        // Closed first, so their scanout engines are free for the screens that open.
        let gone: Vec<OutputId> = self
            .heads
            .iter()
            .filter(|head| plan.closed.contains(&head.name))
            .map(|head| head.id)
            .collect();
        let mut kept = std::mem::take(&mut self.heads);
        kept.retain(|head| !plan.closed.contains(&head.name));

        let mut heads = Vec::with_capacity(plan.layout.len());
        for (name, id) in &plan.layout {
            if let Some(position) = kept.iter().position(|head| head.name == *name) {
                let mut head = kept.swap_remove(position);
                head.id = *id;
                head.stale = true;
                heads.push(head);
                continue;
            }
            let Some(position) = targets.iter().position(|target| target.name == *name) else {
                continue;
            };
            let target = targets.swap_remove(position);
            let scale = self.settings.scale_of(name).unwrap_or_default();
            // The rectangle is a stand-in: the whole row is laid out again below.
            let rect = Rect::new(0, 0, 1, 1)?;
            match open_head(
                &mut self.device,
                &self.gbm,
                &self.formats,
                target,
                (*id, scale, rect),
            ) {
                Ok(head) => heads.push(head),
                Err(error) => eprintln!("lutece: the screen {name} could not be opened: {error}"),
            }
        }
        if heads.is_empty() {
            return Err(Error::Drm(
                "no screen could be opened after the screens changed".to_owned(),
            ));
        }

        let row: Vec<([u16; 2], Scale)> = heads
            .iter()
            .map(|head| {
                let side = |pixels: i32| u16::try_from(pixels).unwrap_or(u16::MAX);
                ([side(head.size.w), side(head.size.h)], head.scale)
            })
            .collect();
        for (head, rect) in heads.iter_mut().zip(arrange(&row)?) {
            head.rect = rect;
        }
        let layout: Vec<(String, OutputId)> = heads
            .iter()
            .map(|head| (head.name.clone(), head.id))
            .collect();
        self.known = remember(&self.known, &layout);
        self.heads = heads;

        let (outputs, areas, bounds) = geometry(&self.heads);
        for id in gone {
            self.wayland.set_output_scale(id, None);
        }
        for head in &self.heads {
            self.wayland.set_output_scale(head.id, Some(head.scale));
        }
        self.wayland.set_outputs(&outputs);
        self.areas = areas;
        self.shared.bounds = bounds;
        self.description = describe_heads(&self.heads);
        self.wayland.mark_damage();
        eprintln!("lutece: the screens are now {}", self.description);
        Ok(())
    }

    /// Notes the flips that landed, so the next frame can be drawn.
    fn follow_flips(&mut self) -> Result<()> {
        for crtc in std::mem::take(&mut self.shared.flips) {
            let Some(head) = self.heads.iter_mut().find(|head| head.crtc == crtc) else {
                continue;
            };
            // `None` is the flip of the mode setting itself, which no frame of ours is waiting on.
            if head
                .surface
                .frame_submitted()
                .map_err(hardware("finishing a flip"))?
                .is_some()
            {
                head.flip_pending = false;
            }
        }
        Ok(())
    }
}

/// Puts a picture on the screen `target` names: its surface and buffers, at `scale`, showing
/// `rect` of the space as output `id`.
fn open_head(
    device: &mut DrmDevice,
    gbm: &GbmDevice<DrmDeviceFd>,
    formats: &[DrmFormat],
    target: Target,
    (id, scale, rect): (OutputId, Scale, Rect),
) -> Result<Head> {
    let drm_surface = device
        .create_surface(target.crtc, target.mode, &[target.connector])
        .map_err(hardware("screen surface"))?;
    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let surface = GbmBufferedSurface::new(drm_surface, allocator, &COLOR_FORMATS, formats.to_vec())
        .map_err(hardware("screen buffers"))?;
    let [width, height] = target.size;
    let mut line = format!(
        "{} at {width}x{height} {} Hz",
        target.name, target.refresh_hz
    );
    if scale != Scale::ONE {
        line.push_str(&format!(" scale {}", scale.factor()));
    }
    Ok(Head {
        name: target.name,
        id,
        line,
        crtc: target.crtc,
        surface,
        size: smithay::utils::Size::from((i32::from(width), i32::from(height))),
        scale,
        rect,
        flip_pending: false,
        stale: true,
    })
}

/// The screens as the one line [`DrmBackend::describe`] gives.
fn describe_heads(heads: &[Head]) -> String {
    heads
        .iter()
        .map(|head| head.line.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// What the screens look like to the core (its outputs), to the pointer (its areas) and to
/// an absolute pointer device (the far corner of the box around all of them).
fn geometry(heads: &[Head]) -> (Vec<Output>, Vec<Area>, (i32, i32)) {
    let outputs: Vec<Output> = heads
        .iter()
        .map(|head| Output {
            id: head.id,
            rect: head.rect,
        })
        .collect();
    let areas = heads
        .iter()
        .map(|head| {
            (
                f64::from(head.rect.x()),
                f64::from(head.rect.y()),
                f64::from(head.rect.width()),
                f64::from(head.rect.height()),
            )
        })
        .collect();
    let edge = |far: fn(&Output) -> i32| outputs.iter().map(far).max().unwrap_or(0);
    let bounds = (
        edge(|output| far_edge(output.rect.x(), output.rect.width())),
        edge(|output| far_edge(output.rect.y(), output.rect.height())),
    );
    (outputs, areas, bounds)
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

/// Where an edge that starts at `start` and is `length` long ends, kept inside `i32`.
fn far_edge(start: i32, length: u32) -> i32 {
    i32::try_from(i64::from(start) + i64::from(length)).unwrap_or(i32::MAX)
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
