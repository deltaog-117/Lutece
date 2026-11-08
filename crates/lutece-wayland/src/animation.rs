//! The animation of a window moving from one rectangle to another.
//!
//! The core only says where a window starts and ends; the frames in between belong to the
//! backend, which keeps a clock. Everything here is a pure function of the rectangles, the
//! transition and the time that has passed, so it can be tested without a display.

use lutece::{Curve, Rect, Transition};
use std::time::{Duration, Instant};

/// A window on its way from `from` to `to`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Animation {
    from: Rect,
    to: Rect,
    started: Instant,
    transition: Transition,
}

impl Animation {
    /// Starts moving from `from` to `to` at `now`.
    pub(crate) const fn new(from: Rect, to: Rect, now: Instant, transition: Transition) -> Self {
        Self {
            from,
            to,
            started: now,
            transition,
        }
    }

    /// Whether the window has arrived at `now`.
    pub(crate) fn finished(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// The rectangle to draw at `now`: `from` at the start, `to` from the end of the
    /// duration on, the curve's blend in between.
    pub(crate) fn rect_at(&self, now: Instant) -> Rect {
        let t = ease(self.transition.curve, self.progress(now));
        let lerp = |a: i64, b: i64| a + ((b - a) as f64 * t).round() as i64;
        let (x, y) = (
            lerp(self.from.x().into(), self.to.x().into()),
            lerp(self.from.y().into(), self.to.y().into()),
        );
        let (w, h) = (
            lerp(self.from.width().into(), self.to.width().into()),
            lerp(self.from.height().into(), self.to.height().into()),
        );
        // A blend of two valid rectangles is inside the range of both, but a window
        // is never drawn with no area: one pixel is the smallest.
        Rect::new(
            i32::try_from(x).unwrap_or(i32::MAX),
            i32::try_from(y).unwrap_or(i32::MAX),
            u32::try_from(w.max(1)).unwrap_or(u32::MAX),
            u32::try_from(h.max(1)).unwrap_or(u32::MAX),
        )
        .unwrap_or(self.to)
    }

    /// How far along it is at `now`, from 0 to 1. A duration of zero is done at once.
    fn progress(&self, now: Instant) -> f64 {
        let total = Duration::from_millis(u64::from(self.transition.duration_ms));
        if total.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started);
        (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
    }
}

/// Maps linear progress `t` (0 to 1) through `curve`. Every curve starts at 0 and ends at 1.
pub(crate) fn ease(curve: Curve, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        Curve::EaseIn => t * t * t,
        Curve::EaseOut => 1.0 - (1.0 - t).powi(3),
        Curve::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            }
        }
        // `Linear`, and any curve the core adds later: constant speed is the safe choice.
        _ => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect::new(x, y, w, h).expect("non-zero size")
    }

    fn animation(curve: Curve, ms: u16, now: Instant) -> Animation {
        Animation::new(
            rect(0, 0, 100, 100),
            rect(200, 100, 300, 50),
            now,
            Transition {
                curve,
                duration_ms: ms,
            },
        )
    }

    #[test]
    fn it_starts_at_the_old_rectangle_and_ends_at_the_new_one() {
        let start = Instant::now();
        let a = animation(Curve::Linear, 100, start);

        assert_eq!(a.rect_at(start), rect(0, 0, 100, 100));
        assert_eq!(
            a.rect_at(start + Duration::from_millis(100)),
            rect(200, 100, 300, 50)
        );
        assert_eq!(
            a.rect_at(start + Duration::from_secs(60)),
            rect(200, 100, 300, 50)
        );
        assert!(a.finished(start + Duration::from_millis(100)));
        assert!(!a.finished(start + Duration::from_millis(99)));
    }

    #[test]
    fn linear_is_halfway_at_half_time() {
        let start = Instant::now();
        let a = animation(Curve::Linear, 100, start);

        assert_eq!(
            a.rect_at(start + Duration::from_millis(50)),
            rect(100, 50, 200, 75)
        );
    }

    #[test]
    fn a_zero_duration_is_already_there() {
        let start = Instant::now();
        let a = animation(Curve::EaseInOut, 0, start);

        assert!(a.finished(start));
        assert_eq!(a.rect_at(start), rect(200, 100, 300, 50));
    }

    #[test]
    fn a_clock_that_goes_back_stays_at_the_start() {
        let start = Instant::now() + Duration::from_secs(5);
        let a = animation(Curve::Linear, 100, start);

        assert_eq!(a.rect_at(Instant::now()), rect(0, 0, 100, 100));
    }

    #[test]
    fn ease_in_is_slower_than_linear_early_and_ease_out_faster() {
        assert!(ease(Curve::EaseIn, 0.3) < 0.3);
        assert!(ease(Curve::EaseOut, 0.3) > 0.3);
        assert!((ease(Curve::EaseInOut, 0.5) - 0.5).abs() < 1e-9);
    }

    proptest! {
        #[test]
        fn every_curve_runs_from_zero_to_one_without_going_backwards(curve in 0u8..4) {
            let curve = [Curve::Linear, Curve::EaseIn, Curve::EaseOut, Curve::EaseInOut][usize::from(curve)];
            prop_assert!(ease(curve, 0.0).abs() < 1e-12);
            prop_assert!((ease(curve, 1.0) - 1.0).abs() < 1e-12);
            let mut last = 0.0;
            for step in 0..=100 {
                let now = ease(curve, f64::from(step) / 100.0);
                prop_assert!(now >= last - 1e-12, "went back at step {}", step);
                last = now;
            }
        }

        #[test]
        fn the_drawn_rectangle_stays_between_the_two_and_never_collapses(
            (x1, y1, w1, h1) in (-2000i32..2000, -2000i32..2000, 1u32..4000, 1u32..4000),
            (x2, y2, w2, h2) in (-2000i32..2000, -2000i32..2000, 1u32..4000, 1u32..4000),
            ms in 1u16..2000,
            elapsed in 0u64..3000,
            curve in 0u8..4,
        ) {
            let curve = [Curve::Linear, Curve::EaseIn, Curve::EaseOut, Curve::EaseInOut][usize::from(curve)];
            let start = Instant::now();
            let a = Animation::new(
                rect(x1, y1, w1, h1),
                rect(x2, y2, w2, h2),
                start,
                Transition { curve, duration_ms: ms },
            );

            let r = a.rect_at(start + Duration::from_millis(elapsed));

            prop_assert!(r.x() >= x1.min(x2) && r.x() <= x1.max(x2));
            prop_assert!(r.y() >= y1.min(y2) && r.y() <= y1.max(y2));
            prop_assert!(r.width() >= w1.min(w2) && r.width() <= w1.max(w2));
            prop_assert!(r.height() >= h1.min(h2) && r.height() <= h1.max(h2));
        }
    }
}
