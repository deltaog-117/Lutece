//! A window's opacity and corner radius gliding to a new value.
//!
//! The core only says what the style is now; the frames in between belong to the backend,
//! as with a move (see `animation`). Everything here is a pure function of the old and new
//! style, the transition and the time that has passed, so it is tested without a display.
//!
//! Which changes glide is the window's own choice, a [`Glide`] set on its style: a plugin
//! that never sets one gets what windows always did, moves only.

use crate::animation::ease;
use lutece::{Glide, Style, Transition};
use std::time::{Duration, Instant};

/// A window's opacity and radius on their way to the values of its style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StyleGlide {
    opacity: (u8, u8),
    radius: (u16, u16),
    started: Instant,
    transition: Transition,
}

impl StyleGlide {
    /// Whether the window has arrived at `now`.
    pub(crate) fn finished(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// The opacity to draw at `now`.
    pub(crate) fn opacity_at(&self, now: Instant) -> u8 {
        let t = self.eased(now);
        let value = blend(f64::from(self.opacity.0), f64::from(self.opacity.1), t);
        u8::try_from(value).unwrap_or(u8::MAX)
    }

    /// The corner radius to draw at `now`.
    pub(crate) fn radius_at(&self, now: Instant) -> u16 {
        let t = self.eased(now);
        let value = blend(f64::from(self.radius.0), f64::from(self.radius.1), t);
        u16::try_from(value).unwrap_or(u16::MAX)
    }

    fn eased(&self, now: Instant) -> f64 {
        ease(self.transition.curve, self.progress(now))
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

/// `from` moved `t` of the way to `to`, rounded, and never outside the two.
fn blend(from: f64, to: f64, t: f64) -> i64 {
    // The inputs are `u8` or `u16` values, so the rounded result is far inside `i64`.
    (from + (to - from) * t.clamp(0.0, 1.0)).round() as i64
}

/// The style to draw: `style` with the opacity and radius the glide has reached at `now`.
/// A glide that has finished, or none, draws the style as it is.
pub(crate) fn drawn_style(style: &Style, glide: Option<&StyleGlide>, now: Instant) -> Style {
    match glide {
        Some(glide) if !glide.finished(now) => style
            .with_opacity(glide.opacity_at(now))
            .with_radius(glide.radius_at(now)),
        _ => *style,
    }
}

/// What starts when a window that is on screen changes from `shown` (what is drawn now) to
/// `next`: a glide of the values `next` lets glide, or `None` when nothing needs one.
///
/// A value that does not glide starts at its new value, so it jumps while the others glide.
pub(crate) fn on_style_change(shown: &Style, next: &Style, now: Instant) -> Option<StyleGlide> {
    let transition = next.transition();
    if transition.duration_ms == 0 {
        return None;
    }
    let glide = next.glide();
    let opacity_from = if glide.contains(Glide::OPACITY) {
        shown.opacity()
    } else {
        next.opacity()
    };
    let radius_from = if glide.contains(Glide::RADIUS) {
        shown.radius()
    } else {
        next.radius()
    };
    if opacity_from == next.opacity() && radius_from == next.radius() {
        return None;
    }
    Some(StyleGlide {
        opacity: (opacity_from, next.opacity()),
        radius: (radius_from, next.radius()),
        started: now,
        transition,
    })
}

/// What starts when a window appears with `style`: a fade in from invisible, if the style
/// asks for it.
pub(crate) fn on_appear(style: &Style, now: Instant) -> Option<StyleGlide> {
    if !style.glide().contains(Glide::APPEARS)
        || style.transition().duration_ms == 0
        || style.opacity() == 0
    {
        return None;
    }
    Some(StyleGlide {
        opacity: (0, style.opacity()),
        radius: (style.radius(), style.radius()),
        started: now,
        transition: style.transition(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lutece::Curve;
    use proptest::prelude::*;

    fn style(opacity: u8, radius: u16, glide: Glide, ms: u16) -> Style {
        Style::default()
            .with_opacity(opacity)
            .with_radius(radius)
            .with_glide(glide)
            .with_transition(Transition {
                curve: Curve::Linear,
                duration_ms: ms,
            })
    }

    fn after(ms: u64, start: Instant) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn the_default_glide_never_starts_for_a_look_change() {
        let now = Instant::now();
        let before = style(255, 0, Glide::MOVES, 100);
        let next = style(100, 12, Glide::MOVES, 100);
        assert!(on_style_change(&before, &next, now).is_none());
        assert!(on_appear(&next, now).is_none());
    }

    #[test]
    fn opacity_glides_from_what_is_drawn_to_the_new_value() {
        let start = Instant::now();
        let before = style(200, 0, Glide::OPACITY, 100);
        let next = style(100, 0, Glide::OPACITY, 100);
        let glide = on_style_change(&before, &next, start).expect("opacity differs");

        assert_eq!(glide.opacity_at(start), 200);
        assert_eq!(glide.opacity_at(after(50, start)), 150);
        assert_eq!(glide.opacity_at(after(100, start)), 100);
        assert!(glide.finished(after(100, start)));
        assert!(!glide.finished(after(99, start)));
    }

    #[test]
    fn a_value_the_set_leaves_out_jumps_while_the_others_glide() {
        let start = Instant::now();
        let before = style(200, 0, Glide::OPACITY, 100);
        let next = style(100, 20, Glide::OPACITY, 100);
        let glide = on_style_change(&before, &next, start).expect("opacity differs");

        assert_eq!(glide.radius_at(start), 20, "radius is not in the set");
        assert_eq!(glide.opacity_at(start), 200);
    }

    #[test]
    fn nothing_starts_when_nothing_that_may_glide_changed() {
        let now = Instant::now();
        let before = style(200, 0, Glide::OPACITY, 100);
        // Only the radius changed, and radius is not in the set.
        let next = style(200, 30, Glide::OPACITY, 100);
        assert!(on_style_change(&before, &next, now).is_none());
    }

    #[test]
    fn no_duration_means_no_glide() {
        let now = Instant::now();
        let before = style(255, 0, Glide::OPACITY.with(Glide::APPEARS), 0);
        let next = style(50, 0, Glide::OPACITY.with(Glide::APPEARS), 0);
        assert!(on_style_change(&before, &next, now).is_none());
        assert!(on_appear(&next, now).is_none());
    }

    #[test]
    fn a_window_that_appears_fades_in_to_its_opacity() {
        let start = Instant::now();
        let shown = style(200, 8, Glide::APPEARS, 100);
        let glide = on_appear(&shown, start).expect("appears is set");

        assert_eq!(glide.opacity_at(start), 0);
        assert_eq!(glide.opacity_at(after(100, start)), 200);
        assert_eq!(glide.radius_at(start), 8, "the radius does not change");
    }

    #[test]
    fn a_window_that_is_meant_to_be_invisible_does_not_fade_in() {
        assert!(on_appear(&style(0, 0, Glide::APPEARS, 100), Instant::now()).is_none());
    }

    #[test]
    fn a_finished_glide_draws_the_style_as_it_is() {
        let start = Instant::now();
        let shown = style(200, 8, Glide::APPEARS, 100);
        let glide = on_appear(&shown, start);

        let mid = drawn_style(&shown, glide.as_ref(), after(50, start));
        assert_eq!(mid.opacity(), 100);
        assert_eq!(mid.radius(), 8);
        assert_eq!(
            drawn_style(&shown, glide.as_ref(), after(100, start)),
            shown
        );
        assert_eq!(drawn_style(&shown, None, start), shown);
    }

    #[test]
    fn a_clock_that_goes_back_stays_at_the_start() {
        let start = Instant::now() + Duration::from_secs(5);
        let shown = style(200, 0, Glide::APPEARS, 100);
        let glide = on_appear(&shown, start).expect("appears is set");
        assert_eq!(glide.opacity_at(Instant::now()), 0);
    }

    proptest! {
        /// Whatever the two styles, the curve and the time, what is drawn stays between the
        /// old and the new value, reaches the new one at the end, and never leaves the range
        /// of its type.
        #[test]
        fn a_glide_stays_between_its_ends_and_arrives(
            (o1, o2) in (any::<u8>(), any::<u8>()),
            (r1, r2) in (any::<u16>(), any::<u16>()),
            ms in 1u16..2000,
            elapsed in 0u64..4000,
            curve in 0u8..4,
        ) {
            let curve = [Curve::Linear, Curve::EaseIn, Curve::EaseOut, Curve::EaseInOut]
                [usize::from(curve)];
            let transition = Transition { curve, duration_ms: ms };
            let both = Glide::OPACITY.with(Glide::RADIUS);
            let before = style(o1, r1, both, ms);
            let next = style(o2, r2, both, ms).with_transition(transition);
            let start = Instant::now();
            let Some(glide) = on_style_change(&before, &next, start) else {
                prop_assert!(o1 == o2 && r1 == r2);
                return Ok(());
            };

            let now = after(elapsed, start);
            let (opacity, radius) = (glide.opacity_at(now), glide.radius_at(now));
            prop_assert!(opacity >= o1.min(o2) && opacity <= o1.max(o2));
            prop_assert!(radius >= r1.min(r2) && radius <= r1.max(r2));
            let end = after(u64::from(ms), start);
            prop_assert_eq!(glide.opacity_at(end), o2);
            prop_assert_eq!(glide.radius_at(end), r2);
        }
    }
}
