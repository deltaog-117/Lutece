//! Mouse drags of floating windows: the press that starts one, the motion that follows it
//! and the release that ends it.
//!
//! The press is grabbed on the root window with the drag modifier held, so the window under
//! the pointer never sees it. Everything here is arithmetic on what the grab reports, so it
//! is checked without a server.

use crate::translate::win_id;
use crate::windows::Windows;
use lutece::{Event, WinId};
use x11rb::protocol::xproto::Window;

/// X numbers the left, middle and right buttons 1, 2 and 3.
pub(crate) const MOVE_BUTTON: u8 = 1;
pub(crate) const RESIZE_BUTTON: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Move,
    Resize,
}

/// A drag in progress. Clients see none of it.
#[derive(Debug, Clone, Copy)]
struct Drag {
    id: WinId,
    kind: Kind,
    /// The button that started it; releasing this button ends it.
    button: u8,
    /// Where the pointer was at the last step, in root coordinates.
    last: (i16, i16),
}

/// The drag in progress, if any. At most one runs at a time.
#[derive(Debug, Default)]
pub(crate) struct Drags(Option<Drag>);

impl Drags {
    /// A grabbed button went down with the pointer at `at`, over the child of the root
    /// `child`. Starts a drag when it is a drag button over a window we manage; a second
    /// button pressed during a drag is ignored.
    pub(crate) fn press(&mut self, button: u8, child: Window, at: (i16, i16), windows: &Windows) {
        if self.0.is_some() || !windows.is_managed(child) {
            return;
        }
        let kind = match button {
            MOVE_BUTTON => Kind::Move,
            RESIZE_BUTTON => Kind::Resize,
            _ => return,
        };
        self.0 = Some(Drag {
            id: win_id(child),
            kind,
            button,
            last: at,
        });
    }

    /// The pointer moved to `at`. Returns the adjustment since the last step, if the
    /// pointer really moved.
    pub(crate) fn motion(&mut self, at: (i16, i16)) -> Option<Event> {
        let drag = self.0.as_mut()?;
        let dx = i32::from(at.0) - i32::from(drag.last.0);
        let dy = i32::from(at.1) - i32::from(drag.last.1);
        if dx == 0 && dy == 0 {
            return None;
        }
        drag.last = at;
        let id = drag.id;
        Some(match drag.kind {
            Kind::Move => Event::FloatAdjusted {
                id,
                dx,
                dy,
                dw: 0,
                dh: 0,
            },
            Kind::Resize => Event::FloatAdjusted {
                id,
                dx: 0,
                dy: 0,
                dw: dx,
                dh: dy,
            },
        })
    }

    /// A button came up. Ends the drag if it is the one that started it.
    pub(crate) fn release(&mut self, button: u8) {
        if self.0.is_some_and(|drag| drag.button == button) {
            self.0 = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn managed(window: Window) -> Windows {
        let mut windows = Windows::default();
        assert!(windows.manage(window));
        windows
    }

    fn adjustment(event: Event) -> (i32, i32, i32, i32) {
        match event {
            Event::FloatAdjusted { dx, dy, dw, dh, .. } => (dx, dy, dw, dh),
            other => panic!("expected a float adjustment, got {other:?}"),
        }
    }

    #[test]
    fn the_left_button_moves_and_the_right_button_resizes() {
        let windows = managed(5);

        let mut moving = Drags::default();
        moving.press(MOVE_BUTTON, 5, (10, 10), &windows);
        assert_eq!(
            adjustment(moving.motion((13, 8)).expect("moved")),
            (3, -2, 0, 0)
        );

        let mut resizing = Drags::default();
        resizing.press(RESIZE_BUTTON, 5, (10, 10), &windows);
        assert_eq!(
            adjustment(resizing.motion((13, 8)).expect("moved")),
            (0, 0, 3, -2)
        );
    }

    #[test]
    fn only_managed_windows_and_drag_buttons_start_a_drag() {
        let windows = managed(5);
        let mut drags = Drags::default();

        drags.press(MOVE_BUTTON, 9, (0, 0), &windows);
        drags.press(2, 5, (0, 0), &windows);

        assert_eq!(drags.motion((4, 4)), None);
    }

    #[test]
    fn a_still_pointer_reports_nothing() {
        let mut drags = Drags::default();
        drags.press(MOVE_BUTTON, 5, (7, 7), &managed(5));
        assert_eq!(drags.motion((7, 7)), None);
    }

    #[test]
    fn releasing_the_starting_button_ends_the_drag() {
        let mut drags = Drags::default();
        drags.press(MOVE_BUTTON, 5, (0, 0), &managed(5));

        drags.release(RESIZE_BUTTON);
        assert!(drags.motion((1, 1)).is_some());
        drags.release(MOVE_BUTTON);
        assert_eq!(drags.motion((2, 2)), None);
    }

    #[test]
    fn a_second_button_does_not_take_over_a_drag() {
        let windows = managed(5);
        let mut drags = Drags::default();
        drags.press(MOVE_BUTTON, 5, (0, 0), &windows);
        drags.press(RESIZE_BUTTON, 5, (0, 0), &windows);

        assert_eq!(
            adjustment(drags.motion((2, 3)).expect("moved")),
            (2, 3, 0, 0)
        );
    }

    proptest! {
        /// However the pointer wanders, the steps of a move add up to the distance from
        /// where it started to where it ended, so the window ends up under the cursor.
        #[test]
        fn the_steps_add_up_to_the_pointer_displacement(
            start in (any::<i16>(), any::<i16>()),
            path in proptest::collection::vec((any::<i16>(), any::<i16>()), 0..40),
            resize in any::<bool>(),
        ) {
            let button = if resize { RESIZE_BUTTON } else { MOVE_BUTTON };
            let mut drags = Drags::default();
            drags.press(button, 5, start, &managed(5));

            let (mut across, mut down) = (0i64, 0i64);
            for point in &path {
                if let Some(event) = drags.motion(*point) {
                    let (dx, dy, dw, dh) = adjustment(event);
                    let (x, y) = if resize { (dw, dh) } else { (dx, dy) };
                    across += i64::from(x);
                    down += i64::from(y);
                }
            }
            let end = path.last().copied().unwrap_or(start);
            prop_assert_eq!(across, i64::from(end.0) - i64::from(start.0));
            prop_assert_eq!(down, i64::from(end.1) - i64::from(start.1));
        }
    }
}
