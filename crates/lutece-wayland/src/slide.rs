//! A window sliding off its screen when it is hidden, and back in when it is shown.
//!
//! A workspace switch hides the windows of one workspace and shows those of another. With
//! [`Glide::SLIDES`](lutece::Glide::SLIDES) the hidden ones glide off to the left by the
//! width of their screen while the shown ones come in from the right, so the switch reads as
//! a move along a row. The core only says hidden and shown; the frames between are the
//! backend's, as with a move (see `animation`), so everything here is a pure function of
//! rectangles and time and is tested without a display.

use crate::animation::Animation;
use crate::presence::shared_area;
use lutece::{Rect, Transition};
use std::time::Instant;

/// How far a window at `window` slides: the width of the screen it covers most, or its own
/// width when it is on none. Far enough that a window inside its screen ends up entirely
/// outside it.
fn distance(window: Rect, screens: &[Rect]) -> i64 {
    screens
        .iter()
        .map(|screen| (shared_area(window, *screen), *screen))
        // `max_by_key` keeps the last of equals; the earlier screen wins a tie, as for scales.
        .rev()
        .max_by_key(|(shared, _)| *shared)
        .filter(|(shared, _)| *shared > 0)
        .map_or(i64::from(window.width()), |(_, screen)| {
            i64::from(screen.width())
        })
}

/// `window` moved sideways by `by` pixels, kept inside what a rectangle can hold.
fn shifted(window: Rect, by: i64) -> Rect {
    let x = i64::from(window.x()).saturating_add(by);
    let x = i32::try_from(x).unwrap_or(if by < 0 { i32::MIN } else { i32::MAX });
    Rect::new(x, window.y(), window.width(), window.height()).unwrap_or(window)
}

/// Where a window that is hidden ends up: one screen to the left of where it was.
pub(crate) fn off_left(window: Rect, screens: &[Rect]) -> Rect {
    shifted(window, -distance(window, screens))
}

/// Where a window that is shown again starts: one screen to the right of where it will sit.
pub(crate) fn off_right(window: Rect, screens: &[Rect]) -> Rect {
    shifted(window, distance(window, screens))
}

/// A hidden window on its way off the screen. It is no longer placed, but it is drawn until
/// the slide is over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Leaving {
    /// The rectangle the core had given the window, which its client sized itself to.
    target: Rect,
    animation: Animation,
}

impl Leaving {
    /// Starts sliding at `now` from where the window is drawn (`from`, which differs from
    /// `target` while it moves) to one screen to the left.
    pub(crate) fn new(
        target: Rect,
        from: Rect,
        screens: &[Rect],
        now: Instant,
        transition: Transition,
    ) -> Self {
        Self {
            target,
            animation: Animation::new(from, off_left(from, screens), now, transition),
        }
    }

    /// The rectangle the window's client sized itself to.
    pub(crate) const fn target(&self) -> Rect {
        self.target
    }

    /// Whether the window is gone from the screen at `now`.
    pub(crate) fn finished(&self, now: Instant) -> bool {
        self.animation.finished(now)
    }

    /// Where the window is drawn at `now`.
    pub(crate) fn rect_at(&self, now: Instant) -> Rect {
        self.animation.rect_at(now)
    }

    /// How far the slide is at `now`, eased, from 0 to 1. For a user's effect.
    pub(crate) fn progress_at(&self, now: Instant) -> f32 {
        self.animation.progress_at(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::overlaps;
    use lutece::Curve;
    use proptest::prelude::*;
    use std::time::Duration;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect::new(x, y, w, h).expect("non-zero size")
    }

    fn transition(ms: u16) -> Transition {
        Transition {
            curve: Curve::Linear,
            duration_ms: ms,
        }
    }

    #[test]
    fn a_window_slides_by_the_width_of_its_screen() {
        let screens = [rect(0, 0, 1920, 1080), rect(1920, 0, 1280, 1024)];
        let window = rect(2000, 10, 300, 200);

        assert_eq!(off_left(window, &screens), rect(720, 10, 300, 200));
        assert_eq!(off_right(window, &screens), rect(3280, 10, 300, 200));
    }

    #[test]
    fn a_window_on_no_screen_slides_by_its_own_width() {
        assert_eq!(off_left(rect(0, 0, 300, 200), &[]), rect(-300, 0, 300, 200));
    }

    #[test]
    fn a_window_on_two_screens_slides_by_the_one_it_covers_most() {
        let screens = [rect(0, 0, 1000, 500), rect(1000, 0, 400, 500)];
        let window = rect(900, 0, 200, 100);

        assert_eq!(off_left(window, &screens), rect(-100, 0, 200, 100));
    }

    #[test]
    fn a_window_at_the_edge_of_the_range_does_not_wrap() {
        let screens = [rect(i32::MIN, 0, u32::MAX, 10)];

        let moved = off_left(rect(i32::MIN, 0, 10, 10), &screens);
        assert_eq!(moved.x(), i32::MIN);
        let moved = off_right(rect(i32::MAX - 5, 0, 10, 10), &[]);
        assert_eq!(moved.x(), i32::MAX);
    }

    #[test]
    fn a_leaving_window_starts_where_it_is_and_ends_off_the_screen() {
        let screens = [rect(0, 0, 1000, 500)];
        let start = Instant::now();
        let window = rect(100, 50, 300, 200);

        let leaving = Leaving::new(window, window, &screens, start, transition(100));

        assert_eq!(leaving.rect_at(start), window);
        assert!(!leaving.finished(start + Duration::from_millis(50)));
        assert!(leaving.finished(start + Duration::from_millis(100)));
        let end = leaving.rect_at(start + Duration::from_millis(100));
        assert!(!overlaps(end, screens[0]));
        assert_eq!(leaving.target(), window);
    }

    #[test]
    fn a_leaving_window_with_no_duration_is_gone_at_once() {
        let window = rect(0, 0, 10, 10);
        let now = Instant::now();

        assert!(Leaving::new(window, window, &[], now, transition(0)).finished(now));
    }

    proptest! {
        #[test]
        fn a_window_inside_its_screen_ends_outside_it_either_way(
            sx in -5000i32..5000, sy in -5000i32..5000,
            sw in 1u32..4000, sh in 1u32..4000,
            fx in 0.0f64..1.0, fy in 0.0f64..1.0,
            fw in 0.01f64..1.0, fh in 0.01f64..1.0,
        ) {
            let screen = rect(sx, sy, sw, sh);
            // A window placed anywhere inside the screen, by fractions of its size.
            let w = ((f64::from(sw) * fw) as u32).max(1);
            let h = ((f64::from(sh) * fh) as u32).max(1);
            let x = sx + (f64::from(sw.saturating_sub(w)) * fx) as i32;
            let y = sy + (f64::from(sh.saturating_sub(h)) * fy) as i32;
            let window = rect(x, y, w, h);

            prop_assert!(!overlaps(off_left(window, &[screen]), screen));
            prop_assert!(!overlaps(off_right(window, &[screen]), screen));
        }

        #[test]
        fn sliding_keeps_the_size_and_the_row(
            x in any::<i32>(), y in any::<i32>(), w in 1u32.., h in 1u32..,
            sw in 1u32..,
        ) {
            let window = rect(x, y, w, h);
            let screens = [rect(0, 0, sw, 100)];

            for moved in [off_left(window, &screens), off_right(window, &screens)] {
                prop_assert_eq!((moved.y(), moved.width(), moved.height()), (y, w, h));
            }
        }
    }
}
