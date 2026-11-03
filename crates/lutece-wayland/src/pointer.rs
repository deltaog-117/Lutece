//! The pointer: which window it is over, focus following it, drags of floating windows,
//! and the cursor image clients ask for.
//!
//! Focus follows the pointer, as on the X11 backend: entering a window asks the core to
//! focus it. A drag with the drag modifier held becomes [`Event::FloatAdjusted`]; the
//! core decides whether the window floats and so whether it moves.

use crate::compositor::State;
use crate::translate;
use lutece::{Event, Modifiers, WinId};
use smithay::backend::input::{Axis, AxisSource, ButtonState};
use smithay::desktop::WindowSurfaceType;
use smithay::desktop::utils::under_from_surface_tree;
use smithay::input::pointer::{AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::shell::xdg::PopupSurface;

/// Linux input codes for the buttons a drag can start with.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragKind {
    Move,
    Resize,
}

/// A drag of a window in progress. Clients see none of it.
#[derive(Debug, Clone, Copy)]
struct Drag {
    id: WinId,
    kind: DragKind,
    /// The button that started it; releasing this button ends it.
    button: u32,
    /// Where the last whole-pixel step ended, so sub-pixel motion is not lost.
    last: Point<f64, Logical>,
}

/// What is under the pointer.
struct Hit {
    /// The managed window it belongs to; an X11 menu belongs to none.
    id: Option<WinId>,
    surface: WlSurface,
    /// Where the surface's own origin is, which is what the protocol wants as focus.
    origin: Point<f64, Logical>,
    popup: bool,
}

/// Everything the backend remembers about the pointer.
pub(crate) struct PointerState {
    pub(crate) location: Point<f64, Logical>,
    pub(crate) cursor: CursorImageStatus,
    /// Modifier that turns a button press into a drag. [`Modifiers::NONE`] disables drags.
    pub(crate) drag_modifier: Modifiers,
    /// The window the pointer was last over, to ask for focus only when it changes.
    over: Option<WinId>,
    drag: Option<Drag>,
    /// Popups that asked to be dismissed by a click elsewhere.
    pub(crate) grabbed_popups: Vec<PopupSurface>,
}

impl PointerState {
    pub(crate) fn new() -> Self {
        Self {
            location: Point::from((0.0, 0.0)),
            cursor: CursorImageStatus::default_named(),
            drag_modifier: Modifiers::ALT,
            over: None,
            drag: None,
            grabbed_popups: Vec::new(),
        }
    }
}

fn drag_kind(button: u32) -> Option<DragKind> {
    match button {
        BTN_LEFT => Some(DragKind::Move),
        BTN_RIGHT => Some(DragKind::Resize),
        _ => None,
    }
}

/// A distance in whole pixels. Rounding to nearest keeps a drag centered on the cursor.
fn whole(distance: f64) -> i32 {
    // A float-to-int `as` saturates and maps NaN to zero, which is what a drag wants.
    distance.round() as i32
}

impl State {
    /// The pointer moved to `location`, in output coordinates.
    pub(crate) fn pointer_motion(&mut self, location: Point<f64, Logical>, time: u32) {
        self.pointer.location = location;
        if let Some(drag) = self.pointer.drag {
            self.drag_to(drag, location);
            return;
        }
        let hit = self.surface_under(location);
        self.follow_pointer(hit.as_ref().and_then(|hit| hit.id));
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let focus = hit.map(|hit| (hit.surface, hit.origin));
        let event = MotionEvent {
            location,
            serial: SERIAL_COUNTER.next_serial(),
            time,
        };
        pointer.motion(self, focus, &event);
        pointer.frame(self);
    }

    /// A pointer button went down or up. `button` is a Linux input code such as
    /// `BTN_LEFT`.
    pub(crate) fn pointer_button(&mut self, button: u32, pressed: bool, time: u32) {
        if self.handle_drag_button(button, pressed) {
            return;
        }
        if pressed {
            self.dismiss_popup_grabs();
            // A bar or launcher that takes the keyboard when clicked gets it now.
            let clicked = self.surface_under(self.pointer.location);
            self.click_on(clicked.as_ref().map(|hit| &hit.surface));
        }
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let state = if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };
        let event = ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time,
            button,
            state,
        };
        pointer.button(self, &event);
        pointer.frame(self);
    }

    /// The wheel turned. Positive values scroll down and right.
    pub(crate) fn pointer_axis(&mut self, horizontal: f64, vertical: f64, time: u32) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let mut frame = AxisFrame::new(time).source(AxisSource::Wheel);
        if horizontal != 0.0 {
            frame = frame.value(Axis::Horizontal, horizontal);
        }
        if vertical != 0.0 {
            frame = frame.value(Axis::Vertical, vertical);
        }
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    /// Asks the core to focus the window the pointer entered.
    fn follow_pointer(&mut self, over: Option<WinId>) {
        if self.pointer.over == over {
            return;
        }
        self.pointer.over = over;
        if let Some(id) = over {
            self.events.push(Event::FocusRequested(id));
            // Entering a window gives the keyboard back to the core's focus.
            self.release_layer_focus();
        }
    }

    /// Starts or ends a drag when `button` is one that does. Returns whether the event
    /// was taken, in which case clients must not see it.
    fn handle_drag_button(&mut self, button: u32, pressed: bool) -> bool {
        if let Some(drag) = self.pointer.drag {
            if !pressed && drag.button == button {
                self.pointer.drag = None;
            }
            return true;
        }
        if !pressed || !self.drag_modifier_held() {
            return false;
        }
        let Some(kind) = drag_kind(button) else {
            return false;
        };
        let Some(id) = self
            .surface_under(self.pointer.location)
            .and_then(|hit| hit.id)
        else {
            return false;
        };
        self.pointer.drag = Some(Drag {
            id,
            kind,
            button,
            last: self.pointer.location,
        });
        true
    }

    fn drag_modifier_held(&self) -> bool {
        let wanted = self.pointer.drag_modifier;
        wanted != Modifiers::NONE
            && self.seat.get_keyboard().is_some_and(|keyboard| {
                translate::modifiers(&keyboard.modifier_state()).contains(wanted)
            })
    }

    fn drag_to(&mut self, drag: Drag, location: Point<f64, Logical>) {
        let dx = whole(location.x - drag.last.x);
        let dy = whole(location.y - drag.last.y);
        if dx == 0 && dy == 0 {
            return;
        }
        // Only the whole pixels sent are used up; the remainder carries to the next step.
        let last = Point::from((drag.last.x + f64::from(dx), drag.last.y + f64::from(dy)));
        self.pointer.drag = Some(Drag { last, ..drag });
        let id = drag.id;
        self.events.push(match drag.kind {
            DragKind::Move => Event::FloatAdjusted {
                id,
                dx,
                dy,
                dw: 0,
                dh: 0,
            },
            DragKind::Resize => Event::FloatAdjusted {
                id,
                dx: 0,
                dy: 0,
                dw: dx,
                dh: dy,
            },
        });
    }

    /// A click outside the popups that grabbed the pointer dismisses them, which is how
    /// a menu closes.
    fn dismiss_popup_grabs(&mut self) {
        if self.pointer.grabbed_popups.is_empty() {
            return;
        }
        let over_popup = self
            .surface_under(self.pointer.location)
            .is_some_and(|hit| hit.popup);
        if !over_popup {
            for popup in self.pointer.grabbed_popups.drain(..) {
                popup.send_popup_done();
            }
        }
    }

    /// The topmost surface at `location`: popups above their window, windows in
    /// stacking order.
    fn surface_under(&self, location: Point<f64, Logical>) -> Option<Hit> {
        self.layers().into_iter().find_map(|layer| {
            under_from_surface_tree(
                &layer.surface,
                location,
                layer.origin,
                WindowSurfaceType::ALL,
            )
            .map(|(surface, at)| Hit {
                id: layer.id,
                surface,
                origin: at.to_f64(),
                popup: layer.popup,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn whole_pixels_never_lose_more_than_half_a_pixel(distance in -1.0e6_f64..1.0e6) {
            prop_assert!((distance - f64::from(whole(distance))).abs() <= 0.5);
        }
    }

    #[test]
    fn only_the_left_and_right_buttons_start_drags() {
        assert_eq!(drag_kind(BTN_LEFT), Some(DragKind::Move));
        assert_eq!(drag_kind(BTN_RIGHT), Some(DragKind::Resize));
        assert_eq!(drag_kind(0x112), None);
    }
}
