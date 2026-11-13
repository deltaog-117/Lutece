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

/// A screen the pointer may be on: its left edge, top edge, width and height, in logical
/// pixels.
pub(crate) type Area = (f64, f64, f64, f64);

/// Hands `input` to the compositor. A key combination the core grabbed comes back as
/// its event. `areas` are the screens: a moving pointer stays on one of them.
pub(crate) fn apply(wayland: &mut WaylandBackend, input: Input, areas: &[Area]) -> Option<Event> {
    match input {
        Input::Key { code, state, time } => {
            let action = match state {
                KeyState::Pressed => KeyAction::Press,
                KeyState::Released => KeyAction::Release,
            };
            return wayland.key_event(code, action, time).map(Event::KeyPressed);
        }
        Input::Motion { x, y, time } => {
            // An absolute device spreads over the box around every screen, which can hold
            // places no screen covers.
            let (x, y) = nearest(areas, (x, y));
            wayland.pointer_motion(x, y, time);
        }
        Input::Move { dx, dy, time } => {
            let (x, y) = moved(wayland.pointer_location(), (dx, dy), areas);
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

/// Where a pointer at `from` ends up after moving by `delta`, kept on the screens. A device
/// can report a move far past the edge, and the pointer would be lost out there.
fn moved(from: (f64, f64), delta: (f64, f64), areas: &[Area]) -> (f64, f64) {
    let next = (from.0 + delta.0, from.1 + delta.1);
    // Not-a-number would slip through the clamping; a device that sends it moves nothing.
    if next.0.is_nan() || next.1.is_nan() {
        return from;
    }
    nearest(areas, next)
}

/// The point of the screens closest to `at`: `at` itself when a screen holds it, otherwise
/// its nearest point on the edge of the nearest screen. With no screens, `at` is kept.
///
/// Screens of different sizes side by side leave places inside their bounding box that no
/// screen covers; the pointer is moved off them rather than left where nothing is drawn.
fn nearest(areas: &[Area], at: (f64, f64)) -> (f64, f64) {
    areas
        .iter()
        .map(|&(left, top, width, height)| {
            (
                at.0.clamp(left, left + width),
                at.1.clamp(top, top + height),
            )
        })
        .min_by(|a, b| distance(*a, at).total_cmp(&distance(*b, at)))
        .unwrap_or(at)
}

/// The square of the distance between two points; enough to tell which is nearer.
fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
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
            let (nx, ny) = moved((x * width, y * height), (dx, dy), &[(0.0, 0.0, width, height)]);
            prop_assert!((0.0..=width).contains(&nx));
            prop_assert!((0.0..=height).contains(&ny));
        }

        /// With several screens, the pointer is always on one of them, however far the
        /// device reports and wherever the screens are.
        #[test]
        fn the_pointer_is_always_on_a_screen(
            areas in prop::collection::vec(
                (-5000.0f64..5000.0, -5000.0f64..5000.0, 1.0f64..4000.0, 1.0f64..4000.0),
                1..4,
            ),
            dx in -1.0e9f64..1.0e9,
            dy in -1.0e9f64..1.0e9,
        ) {
            let from = (areas[0].0, areas[0].1);
            let (nx, ny) = moved(from, (dx, dy), &areas);
            let on_a_screen = areas.iter().any(|&(left, top, width, height)| {
                (left..=left + width).contains(&nx) && (top..=top + height).contains(&ny)
            });
            prop_assert!(on_a_screen);
        }

        /// A place on a screen is kept exactly: the pointer is only moved off places no screen
        /// covers.
        #[test]
        fn a_place_on_a_screen_is_kept(
            width in 1.0f64..4000.0,
            height in 1.0f64..4000.0,
            other_height in 1.0f64..4000.0,
            x in 0.0f64..1.0,
            y in 0.0f64..1.0,
        ) {
            // Two screens side by side, top aligned, of different heights.
            let areas = [(0.0, 0.0, width, height), (width, 0.0, width, other_height)];
            let on = (x * width, y * height);
            prop_assert_eq!(nearest(&areas, on), on);
        }

        /// A move that stays on the output lands exactly where it says.
        #[test]
        fn a_move_inside_the_output_is_exact(
            x in 100.0f64..200.0,
            y in 100.0f64..200.0,
            dx in -50.0f64..50.0,
            dy in -50.0f64..50.0,
        ) {
            let (nx, ny) = moved((x, y), (dx, dy), &[(0.0, 0.0, 1000.0, 1000.0)]);
            prop_assert_eq!(nx, x + dx);
            prop_assert_eq!(ny, y + dy);
        }
    }

    #[test]
    fn a_place_no_screen_covers_goes_to_the_nearest_screen() {
        // A 1080p screen beside a taller one: the box around both has a corner no screen has.
        let areas = [(0.0, 0.0, 1920.0, 1080.0), (1920.0, 0.0, 1920.0, 2160.0)];
        // Below the short screen and well inside its width: its lower edge is nearest.
        assert_eq!(nearest(&areas, (100.0, 1500.0)), (100.0, 1080.0));
        // Just left of the tall screen: that one's edge is nearer than the short screen's.
        assert_eq!(nearest(&areas, (1900.0, 1500.0)), (1920.0, 1500.0));
    }

    #[test]
    fn a_device_reporting_nothing_sensible_moves_nothing() {
        assert_eq!(
            moved(
                (10.0, 20.0),
                (f64::NAN, f64::NAN),
                &[(0.0, 0.0, 100.0, 100.0)]
            ),
            (10.0, 20.0)
        );
    }
}
