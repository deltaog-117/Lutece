//! Keyboard and pointer input, from a window system or from libinput, in the form the
//! backend takes it.
//!
//! Both sources are collected first and applied afterwards, so the source is never held
//! while the compositor reacts.

use crate::backend::{ButtonAction, KeyAction, WaylandBackend};
use lutece::Event;
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, ButtonState, Event as _, InputBackend, InputEvent, KeyState,
    KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};

/// How far one wheel notch scrolls, in the units clients expect, when the source reports
/// only whole notches.
const PIXELS_PER_NOTCH: f64 = 15.0;
const V120_PER_NOTCH: f64 = 120.0;

/// Something the input devices did.
pub(crate) enum Input {
    Key {
        code: u32,
        state: KeyState,
        time: u32,
    },
    /// The pointer is at a place on the output.
    Motion { x: f64, y: f64, time: u32 },
    /// The pointer moved by an amount, from wherever it was.
    Move { dx: f64, dy: f64, time: u32 },
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

/// Picks out the keyboard and pointer events the compositor handles. `width` and
/// `height` are the area an absolute pointer position is spread over.
pub(crate) fn collect<B: InputBackend>(
    input: InputEvent<B>,
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
        InputEvent::PointerMotion { event } => Input::Move {
            dx: event.delta_x(),
            dy: event.delta_y(),
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

/// Hands `input` to the compositor. A key combination the core grabbed comes back as
/// its event. `bounds` is the size of the area a moving pointer must stay inside.
pub(crate) fn apply(
    wayland: &mut WaylandBackend,
    input: Input,
    bounds: (f64, f64),
) -> Option<Event> {
    match input {
        Input::Key { code, state, time } => {
            let action = match state {
                KeyState::Pressed => KeyAction::Press,
                KeyState::Released => KeyAction::Release,
            };
            return wayland.key_event(code, action, time).map(Event::KeyPressed);
        }
        Input::Motion { x, y, time } => wayland.pointer_motion(x, y, time),
        Input::Move { dx, dy, time } => {
            let (x, y) = moved(wayland.pointer_location(), (dx, dy), bounds);
            wayland.pointer_motion(x, y, time);
        }
        Input::Button { code, state, time } => {
            let action = match state {
                ButtonState::Pressed => ButtonAction::Press,
                ButtonState::Released => ButtonAction::Release,
            };
            wayland.pointer_button(code, action, time);
        }
        Input::Axis {
            horizontal,
            vertical,
            time,
        } => wayland.pointer_axis(horizontal, vertical, time),
    }
    None
}

/// Hands `input` to the compositor while the screen is read-only: only the release of a key
/// or a button gets through, so a key or button that was already down is not left down in a
/// client. Nothing comes back as an event, so no key reaches the core either.
pub(crate) fn apply_read_only(wayland: &mut WaylandBackend, input: Input) {
    match input {
        Input::Key {
            code,
            state: KeyState::Released,
            time,
        } => {
            // A release never grabs, so there is no event to hand back.
            let _ = wayland.key_event(code, KeyAction::Release, time);
        }
        Input::Button {
            code,
            state: ButtonState::Released,
            time,
        } => wayland.pointer_button(code, ButtonAction::Release, time),
        _ => {}
    }
}

/// Where a pointer at `from` ends up after moving by `delta`, kept on the area that
/// starts at the origin and measures `bounds`. A device can report a move far past the
/// edge, and the pointer would be lost out there.
fn moved(from: (f64, f64), delta: (f64, f64), bounds: (f64, f64)) -> (f64, f64) {
    let along = |at: f64, by: f64, limit: f64| {
        let next = at + by;
        // Not-a-number would slip through `clamp`; a device that sends it moves nothing.
        if next.is_nan() {
            at
        } else {
            next.clamp(0.0, limit)
        }
    };
    (
        along(from.0, delta.0, bounds.0),
        along(from.1, delta.1, bounds.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// However far a device reports, the pointer stays on the output.
        #[test]
        fn the_pointer_never_leaves_the_output(
            width in 1.0f64..8000.0,
            height in 1.0f64..4000.0,
            x in 0.0f64..1.0,
            y in 0.0f64..1.0,
            dx in -1.0e9f64..1.0e9,
            dy in -1.0e9f64..1.0e9,
        ) {
            let (nx, ny) = moved((x * width, y * height), (dx, dy), (width, height));
            prop_assert!((0.0..=width).contains(&nx));
            prop_assert!((0.0..=height).contains(&ny));
        }

        /// A move that stays on the output lands exactly where it says.
        #[test]
        fn a_move_inside_the_output_is_exact(
            x in 100.0f64..200.0,
            y in 100.0f64..200.0,
            dx in -50.0f64..50.0,
            dy in -50.0f64..50.0,
        ) {
            let (nx, ny) = moved((x, y), (dx, dy), (1000.0, 1000.0));
            prop_assert_eq!(nx, x + dx);
            prop_assert_eq!(ny, y + dy);
        }
    }

    #[test]
    fn a_device_reporting_nothing_sensible_moves_nothing() {
        assert_eq!(
            moved((10.0, 20.0), (f64::NAN, f64::NAN), (100.0, 100.0)),
            (10.0, 20.0)
        );
    }
}
