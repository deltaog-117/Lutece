//! The compositor shown in a window of the session it runs in, for development.
//!
//! The window is opened with Smithay's winit backend, so it works inside any X11 or
//! Wayland session. Its keyboard and pointer are the compositor's. It can stand for several
//! outputs, laid side by side in the one window.

use crate::apply::Flow;
use crate::backend::{ButtonAction, KeyAction, WaylandBackend};
use crate::dmabuf::DmabufSupport;
use crate::error::{Error, Result};
use crate::scale::Scale;
use lutece::{Effect, Event, KeyPress, Modifiers, Output, OutputId, Rect, Scene, Size};
use smithay::backend::egl::EGLDevice;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, ButtonState, Event as _, InputEvent, KeyState, KeyboardKeyEvent,
    PointerAxisEvent, PointerButtonEvent,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{Color32F, Frame, ImportDma, Renderer};
use smithay::backend::winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend};
use smithay::input::pointer::CursorImageStatus;
use smithay::reexports::winit::platform::pump_events::PumpStatus;
use smithay::utils::{Rectangle, Transform};
use std::ffi::OsStr;
use std::fmt::Display;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

const BACKGROUND: Color32F = Color32F::new(0.10, 0.12, 0.16, 1.0);
/// The window system and the clients are watched in turns, so waiting on the clients for
/// long would leave the window deaf. This bounds the latency of a key press.
const WAIT_SLICE: Duration = Duration::from_millis(8);

/// A [`WaylandBackend`] shown in a window, with that window's keyboard as its keyboard.
///
/// Drive it like the other backends, and draw once per loop turn with
/// [`render`](Self::render). Stop when [`is_closed`](Self::is_closed) is true.
pub struct NestedBackend {
    wayland: WaylandBackend,
    graphics: WinitGraphicsBackend<GlesRenderer>,
    window: WinitEventLoop,
    started: Instant,
    closed: bool,
    /// How many outputs the window is cut into.
    outputs: NonZeroU32,
    /// How many of the window's pixels one logical pixel takes, as the host reports it.
    scale: Scale,
    /// The cursor last given to the window, so it is changed only when it differs.
    shown_cursor: Option<CursorImageStatus>,
}

/// Something the window's input devices did, collected while the window system is
/// pumped and applied afterwards.
enum Input {
    Key {
        code: u32,
        state: KeyState,
        time: u32,
    },
    Motion {
        x: f64,
        y: f64,
        time: u32,
    },
    Button {
        code: u32,
        state: ButtonState,
        time: u32,
    },
    Axis {
        horizontal: f64,
        vertical: f64,
        time: u32,
    },
}

/// How far one wheel notch scrolls, in the units clients expect, when the window
/// system reports only whole notches.
const PIXELS_PER_NOTCH: f64 = 15.0;
const V120_PER_NOTCH: f64 = 120.0;

impl NestedBackend {
    /// Opens the window and starts listening for clients. The output is as large as the
    /// window and follows it when it is resized. It is measured in logical pixels: the
    /// window's pixels divided by the scale the host shows the window at, which is also
    /// followed when it changes.
    ///
    /// # Errors
    ///
    /// Returns an error if the window cannot be opened (no graphics session, no OpenGL)
    /// or the compositor cannot start.
    pub fn open() -> Result<Self> {
        Self::open_with_outputs(NonZeroU32::MIN)
    }

    /// Like [`open`](Self::open), with the window cut into `outputs` outputs of equal width
    /// side by side, to try several monitors without owning them. They are not real
    /// monitors: they share the window's one scale and refresh. A window narrower than
    /// `outputs` pixels gets one output per pixel column.
    ///
    /// # Errors
    ///
    /// Returns an error if the window cannot be opened (no graphics session, no OpenGL)
    /// or the compositor cannot start.
    pub fn open_with_outputs(outputs: NonZeroU32) -> Result<Self> {
        let (mut graphics, window) = winit::init::<GlesRenderer>()?;
        let scale = Scale::from_factor(graphics.scale_factor()).unwrap_or_default();
        let size = size_of(graphics.window_size(), scale)?;
        let mut wayland = WaylandBackend::bind(size)?;
        wayland.set_scale(scale);
        wayland.set_outputs(&split(size, outputs));
        if let Some(support) = dmabuf_support(graphics.renderer()) {
            wayland.enable_dmabuf(&support)?;
        }
        Ok(Self {
            wayland,
            graphics,
            window,
            started: Instant::now(),
            closed: false,
            outputs,
            scale,
            shown_cursor: None,
        })
    }

    /// The name clients put in `WAYLAND_DISPLAY` to reach this compositor.
    #[must_use]
    pub fn socket_name(&self) -> &OsStr {
        self.wayland.socket_name()
    }

    /// Whether the window was closed. The loop should end.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Sets the key combinations the core wants for itself.
    pub fn grab_keys(&mut self, presses: impl IntoIterator<Item = KeyPress>) {
        self.wayland.grab_keys(presses);
    }

    /// Sets the modifier that turns a left or right button press into a drag of the
    /// window under the pointer; see [`WaylandBackend::set_drag_modifier`].
    pub fn set_drag_modifier(&mut self, modifier: Modifiers) {
        self.wayland.set_drag_modifier(modifier);
    }

    /// Collects what happened in the window and among the clients. A grabbed key press
    /// arrives as [`Event::KeyPressed`]; every other key goes to the focused client.
    ///
    /// # Errors
    ///
    /// Returns an error if talking to the clients fails.
    pub fn next_events(&mut self, timeout: Duration) -> Result<Vec<Event>> {
        let mut inputs = Vec::new();
        let mut resized = None;
        let mut closed = false;
        // The pointer is measured in logical pixels, like everything the core sees.
        let window_size = self.graphics.window_size();
        let (width, height) = (
            self.scale.to_logical(window_size.w),
            self.scale.to_logical(window_size.h),
        );
        let status = self.window.dispatch_new_events(|event| match event {
            WinitEvent::Resized { size, scale_factor } => resized = Some((size, scale_factor)),
            WinitEvent::CloseRequested => closed = true,
            WinitEvent::Input(input) => inputs.extend(collect(input, width, height)),
            _ => {}
        });
        self.closed |= closed || matches!(status, PumpStatus::Exit(_));
        if let Some((size, factor)) = resized {
            if let Some(scale) = Scale::from_factor(factor) {
                self.scale = scale;
                self.wayland.set_scale(scale);
            }
            self.wayland
                .set_outputs(&split(size_of(size, self.scale)?, self.outputs));
        }

        let mut events = Vec::new();
        for input in inputs {
            match input {
                Input::Key { code, state, time } => {
                    let action = match state {
                        KeyState::Pressed => KeyAction::Press,
                        KeyState::Released => KeyAction::Release,
                    };
                    if let Some(press) = self.wayland.key_event(code, action, time) {
                        events.push(Event::KeyPressed(press));
                    }
                }
                Input::Motion { x, y, time } => self.wayland.pointer_motion(x, y, time),
                Input::Button { code, state, time } => {
                    let action = match state {
                        ButtonState::Pressed => ButtonAction::Press,
                        ButtonState::Released => ButtonAction::Release,
                    };
                    self.wayland.pointer_button(code, action, time);
                }
                Input::Axis {
                    horizontal,
                    vertical,
                    time,
                } => {
                    self.wayland.pointer_axis(horizontal, vertical, time);
                }
            }
        }
        events.extend(self.wayland.next_events(timeout.min(WAIT_SLICE))?);
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

    /// Draws every placed window into the window and tells the clients the frame was
    /// shown.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Render`] if the frame cannot be drawn.
    pub fn render(&mut self) -> Result<()> {
        let size = self.graphics.window_size();
        let whole = [Rectangle::from_size(size)];
        {
            let (renderer, mut framebuffer) = self.graphics.bind().map_err(render_error)?;
            let mut elements = self.wayland.cursor_elements(renderer);
            elements.extend(self.wayland.surface_elements(renderer));
            let mut frame = renderer
                .render(&mut framebuffer, size, Transform::Normal)
                .map_err(render_error)?;
            frame.clear(BACKGROUND, &whole).map_err(render_error)?;
            draw_render_elements::<GlesRenderer, _, _>(
                &mut frame,
                self.scale.factor(),
                &elements,
                &whole,
            )
            .map_err(render_error)?;
            // Swapping buffers below already orders the GPU work, so the fence is not awaited.
            let _fence = frame.finish().map_err(render_error)?;
        }
        self.graphics.submit(None).map_err(render_error)?;
        self.show_cursor();
        // Milliseconds wrap after about 49 days, which the protocol allows.
        let time_ms = u32::try_from(self.started.elapsed().as_millis() % u128::from(u32::MAX))
            .unwrap_or_default();
        self.wayland.send_frame_callbacks(time_ms)
    }
}

impl NestedBackend {
    /// Shows the cursor the focused client asked for in the host window: its named
    /// cursor, or nothing when the client draws one itself (drawn with the windows) or
    /// wants none.
    fn show_cursor(&mut self) {
        let wanted = self.wayland.cursor().clone();
        if self.shown_cursor.as_ref() == Some(&wanted) {
            return;
        }
        let window = self.graphics.window();
        match &wanted {
            CursorImageStatus::Named(icon) => {
                window.set_cursor_visible(true);
                window.set_cursor(*icon);
            }
            CursorImageStatus::Hidden | CursorImageStatus::Surface(_) => {
                window.set_cursor_visible(false);
            }
        }
        self.shown_cursor = Some(wanted);
    }
}

/// Picks out the keyboard and pointer events the compositor handles.
fn collect(
    input: InputEvent<smithay::backend::winit::WinitInput>,
    width: i32,
    height: i32,
) -> Option<Input> {
    Some(match input {
        InputEvent::Keyboard { event } => Input::Key {
            code: event.key_code().raw(),
            state: event.state(),
            time: event.time_msec(),
        },
        InputEvent::PointerMotionAbsolute { event } => Input::Motion {
            x: event.x_transformed(width),
            y: event.y_transformed(height),
            time: event.time_msec(),
        },
        InputEvent::PointerButton { event } => Input::Button {
            code: event.button_code(),
            state: event.state(),
            time: event.time_msec(),
        },
        InputEvent::PointerAxis { event } => {
            let amount = |axis| {
                event
                    .amount(axis)
                    .or_else(|| {
                        event
                            .amount_v120(axis)
                            .map(|v120| v120 / V120_PER_NOTCH * PIXELS_PER_NOTCH)
                    })
                    .unwrap_or(0.0)
            };
            Input::Axis {
                horizontal: amount(Axis::Horizontal),
                vertical: amount(Axis::Vertical),
                time: event.time_msec(),
            }
        }
        _ => return None,
    })
}

/// The GPU the renderer draws with, as clients need to know it. `None` when it draws in
/// software or its device cannot be found, in which case clients keep using shared memory.
fn dmabuf_support(renderer: &GlesRenderer) -> Option<DmabufSupport> {
    let device = EGLDevice::device_for_display(renderer.egl_context().display()).ok()?;
    let node = device.try_get_render_node().ok()??;
    let offered = renderer.dmabuf_formats();
    let formats = offered
        .iter()
        .map(|format| (format.code as u32, u64::from(format.modifier)));
    Some(DmabufSupport::new(node.dev_id(), formats))
}

fn render_error(error: impl Display) -> Error {
    Error::Render(error.to_string())
}

/// Cuts a window of `size` into `count` outputs side by side that cover it exactly, their
/// widths differing by at most one pixel. Fewer come back when the window has fewer pixel
/// columns than `count`, since an output cannot be empty.
fn split(size: Size, count: NonZeroU32) -> Vec<Output> {
    let pieces = count.get().min(size.width());
    // Cut positions are computed from the whole width, not summed, so rounding never drifts.
    let cut = |piece: u32| {
        let at = u64::from(size.width()) * u64::from(piece) / u64::from(pieces);
        i32::try_from(at).unwrap_or(i32::MAX)
    };
    (0..pieces)
        .filter_map(|piece| {
            let left = cut(piece);
            let width = u32::try_from(cut(piece + 1) - left).ok()?;
            Some(Output {
                id: OutputId::new(piece),
                rect: Rect::new(left, 0, width, size.height()).ok()?,
            })
        })
        .collect()
}

/// The core size, in logical pixels, for a window of `size` physical pixels shown at
/// `scale`.
fn size_of(
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    scale: Scale,
) -> Result<Size> {
    let positive = |length: i32| u32::try_from(scale.to_logical(length)).unwrap_or(0);
    Ok(Size::new(positive(size.w), positive(size.h))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The outputs tile the window: each starts where the one before ends, together
        /// they span it, and none is empty.
        #[test]
        fn the_outputs_tile_the_window(width in 1u32..6000, height in 1u32..4000, count in 1u32..20) {
            let size = Size::new(width, height).expect("non-zero");
            let outputs = split(size, NonZeroU32::new(count).expect("non-zero"));

            prop_assert_eq!(outputs.len(), usize::try_from(count.min(width)).expect("small"));
            let mut next = 0;
            for (index, output) in outputs.iter().enumerate() {
                prop_assert_eq!(output.id, OutputId::new(u32::try_from(index).expect("small")));
                prop_assert_eq!(output.rect.x(), next);
                prop_assert_eq!(output.rect.y(), 0);
                prop_assert_eq!(output.rect.height(), height);
                next += i32::try_from(output.rect.width()).expect("small");
            }
            prop_assert_eq!(next, i32::try_from(width).expect("small"));
        }

        #[test]
        fn the_widths_differ_by_at_most_a_pixel(width in 1u32..6000, count in 1u32..20) {
            let size = Size::new(width, 10).expect("non-zero");
            let outputs = split(size, NonZeroU32::new(count).expect("non-zero"));
            let widths = outputs.iter().map(|output| output.rect.width());
            let (least, most) = widths.fold((u32::MAX, 0), |(l, m), w| (l.min(w), m.max(w)));
            prop_assert!(most - least <= 1);
        }
    }

    #[test]
    fn one_output_is_the_whole_window() {
        let size = Size::new(1280, 800).expect("non-zero");
        let outputs = split(size, NonZeroU32::MIN);
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0].rect,
            Rect::from_parts(lutece::Point { x: 0, y: 0 }, size)
        );
    }
}
