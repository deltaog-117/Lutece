//! Opens the seat, the primary GPU and one connected screen, and shows the test pattern.

use super::pick::{ModeInfo, best_mode, pattern_color, pick_crtc};
use crate::error::{Error, Result};
use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent, GbmBufferedSurface};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{Bind, Color32F, Frame, ImportDma, Renderer};
use smithay::backend::session::Session;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::udev::primary_gpu;
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::drm::control::{Device as ControlDevice, ModeTypeFlags, connector};
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Physical, Rectangle, Size, Transform};
use std::fmt::Display;
use std::time::Duration;

/// How long to wait for a page flip before concluding the screen stopped answering.
const FLIP_TIMEOUT: Duration = Duration::from_secs(2);
/// Formats tried in turn for the buffers shown: opaque first, since nothing is behind them.
pub(super) const COLOR_FORMATS: [Fourcc; 2] = [Fourcc::Xrgb8888, Fourcc::Argb8888];

/// What [`show_test_pattern`] put on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternReport {
    /// The connector shown on, for example `HDMIA-1`.
    pub connector: String,
    /// Width of the mode, in pixels.
    pub width: u16,
    /// Height of the mode, in pixels.
    pub height: u16,
    /// Refresh rate of the mode, in hertz.
    pub refresh_hz: u32,
    /// How many frames reached the screen.
    pub frames: u32,
}

/// Shows `frames` frames of a turning colour on the first connected screen.
///
/// Needs the seat of a virtual terminal (seatd or logind), so it fails with
/// [`Error::Drm`] inside a graphical session. Each frame is drawn only after the last one
/// reached the screen, so a stuck page flip ends the run with an error after two seconds
/// instead of hanging.
///
/// # Errors
///
/// [`Error::Drm`] when the seat, the GPU, a connected screen, a mode, the buffers or a
/// page flip cannot be had; the message says which.
pub fn show_test_pattern(frames: u32) -> Result<PatternReport> {
    let mut event_loop: EventLoop<Run> = EventLoop::try_new().map_err(hardware("event loop"))?;
    let (mut session, session_notifier) =
        LibSeatSession::new().map_err(hardware("seat (run this from a virtual terminal)"))?;
    // Pausing and resuming with the terminal is not handled yet: a switch away ends the
    // flips, which the timeout reports.
    event_loop
        .handle()
        .insert_source(session_notifier, |_, (), _| {})
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

    let target = find_target(&device)?;
    let [width, height] = target.size;
    let surface = device
        .create_surface(target.crtc, target.mode, &[target.connector])
        .map_err(hardware("screen surface"))?;

    let gbm = GbmDevice::new(fd).map_err(hardware("buffer allocator"))?;
    let renderer = make_renderer(gbm.clone())?;
    let formats = renderer
        .dmabuf_formats()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    let allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT);
    let surface = GbmBufferedSurface::new(surface, allocator, &COLOR_FORMATS, formats)
        .map_err(hardware("screen buffers"))?;

    let mut run = Run {
        surface,
        renderer,
        size: Size::from((i32::from(width), i32::from(height))),
        shown: 0,
        frames,
        failure: None,
    };
    event_loop
        .handle()
        .insert_source(device_notifier, |event, _, run: &mut Run| match event {
            DrmEvent::VBlank(_) => run.flipped(),
            DrmEvent::Error(error) => run.failure = Some(hardware("page flip")(error)),
        })
        .map_err(hardware("graphics events"))?;

    if frames > 0 {
        run.draw()?;
    }
    while run.shown < frames && run.failure.is_none() {
        let before = run.shown;
        event_loop
            .dispatch(Some(FLIP_TIMEOUT), &mut run)
            .map_err(hardware("waiting for the screen"))?;
        if run.shown == before && run.failure.is_none() {
            return Err(Error::Drm(format!(
                "no page flip within {FLIP_TIMEOUT:?}: the screen stopped answering \
                 (did the terminal switch away?)"
            )));
        }
    }
    if let Some(failure) = run.failure {
        return Err(failure);
    }
    Ok(PatternReport {
        connector: target.name,
        width,
        height,
        refresh_hz: target.refresh_hz,
        frames: run.shown,
    })
}

/// The state the page-flip callback works on: it draws the next frame when one lands.
struct Run {
    surface: GbmBufferedSurface<GbmAllocator<DrmDeviceFd>, ()>,
    renderer: GlesRenderer,
    size: Size<i32, Physical>,
    shown: u32,
    frames: u32,
    failure: Option<Error>,
}

impl Run {
    /// Draws the frame numbered `self.shown` and queues it for the next flip.
    fn draw(&mut self) -> Result<()> {
        let [red, green, blue, alpha] = pattern_color(self.shown, self.frames);
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
                .render(&mut target, self.size, Transform::Normal)
                .map_err(hardware("starting a frame"))?;
            frame
                .clear(
                    Color32F::new(red, green, blue, alpha),
                    &[Rectangle::from_size(self.size)],
                )
                .map_err(hardware("clearing the frame"))?;
            frame.finish().map_err(hardware("finishing the frame"))?
        };
        self.surface
            .queue_buffer(Some(sync), None, ())
            .map_err(hardware("queueing the frame"))
    }

    /// A flip landed: count it and draw the next frame, if any remain.
    fn flipped(&mut self) {
        match self.surface.frame_submitted() {
            // A flip of a frame we did not queue (the mode setting itself): not counted.
            Ok(None) => {}
            Ok(Some(())) => {
                self.shown += 1;
                if self.shown < self.frames
                    && let Err(error) = self.draw()
                {
                    self.failure = Some(error);
                }
            }
            Err(error) => self.failure = Some(hardware("finishing a flip")(error)),
        }
    }
}

/// A connected screen with the mode and engine chosen for it.
pub(super) struct Target {
    pub(super) name: String,
    pub(super) connector: connector::Handle,
    pub(super) crtc: smithay::reexports::drm::control::crtc::Handle,
    pub(super) mode: smithay::reexports::drm::control::Mode,
    pub(super) size: [u16; 2],
    pub(super) refresh_hz: u32,
}

/// The first connected connector that has a mode and a free scanout engine.
pub(super) fn find_target(device: &DrmDevice) -> Result<Target> {
    let mut found = find_targets(device)?;
    // `find_targets` never returns an empty list: it is an error then.
    Ok(found.swap_remove(0))
}

/// Every connected connector that has a mode and a scanout engine no other screen uses, in
/// the order the device lists them. Each gets its own engine: the ones already given out are
/// passed on, so two screens are never put on one.
pub(super) fn find_targets(device: &DrmDevice) -> Result<Vec<Target>> {
    let resources = device
        .resource_handles()
        .map_err(hardware("reading the screens"))?;
    let mut taken = 0u32;
    let mut found = Vec::new();
    for &handle in resources.connectors() {
        let info = device
            .get_connector(handle, true)
            .map_err(hardware("reading a connector"))?;
        if info.state() != connector::State::Connected {
            continue;
        }
        let listed: Vec<ModeInfo> = info
            .modes()
            .iter()
            .map(|mode| ModeInfo {
                width: mode.size().0,
                height: mode.size().1,
                refresh_hz: mode.vrefresh(),
                preferred: mode.mode_type().contains(ModeTypeFlags::PREFERRED),
            })
            .collect();
        let Some(index) = best_mode(&listed) else {
            continue;
        };
        let mut possible = 0u32;
        for encoder in info.encoders() {
            let encoder = device
                .get_encoder(*encoder)
                .map_err(hardware("reading an encoder"))?;
            let usable = resources.filter_crtcs(encoder.possible_crtcs());
            for (position, crtc) in resources.crtcs().iter().enumerate().take(32) {
                if usable.contains(crtc) {
                    possible |= 1 << position;
                }
            }
        }
        let Some(position) = pick_crtc(possible, taken, resources.crtcs().len()) else {
            continue;
        };
        // `pick_crtc` answers below 32, so the shift fits.
        taken |= 1 << position;
        let mode = info.modes()[index];
        found.push(Target {
            name: format!("{:?}-{}", info.interface(), info.interface_id()),
            connector: handle,
            crtc: resources.crtcs()[position],
            mode,
            size: [listed[index].width, listed[index].height],
            refresh_hz: listed[index].refresh_hz,
        });
    }
    if found.is_empty() {
        return Err(Error::Drm(
            "no connected screen with a mode and a free scanout engine".to_owned(),
        ));
    }
    Ok(found)
}

/// Builds the GL renderer on the GPU's own buffer device.
#[allow(unsafe_code)]
pub(super) fn make_renderer(gbm: GbmDevice<DrmDeviceFd>) -> Result<GlesRenderer> {
    // SAFETY: `gbm` is a live buffer device that this call takes and the display keeps
    // alive; nothing else makes an EGL display from it.
    let display = unsafe { EGLDisplay::new(gbm) }.map_err(hardware("EGL display"))?;
    let context = EGLContext::new(&display).map_err(hardware("EGL context"))?;
    // SAFETY: the context was made just above from a display that is still alive, and is
    // not current on any thread, so the renderer may make it current on this one.
    unsafe { GlesRenderer::new(context) }.map_err(hardware("renderer"))
}

/// Names the step in an error, so a failure on a machine without a screen says where.
pub(super) fn hardware<E: Display>(step: &'static str) -> impl Fn(E) -> Error {
    move |error| Error::Drm(format!("{step}: {error}"))
}
