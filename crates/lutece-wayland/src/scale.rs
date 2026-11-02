//! The scale between the logical pixels the core and the clients lay out in and the
//! physical pixels of the screen, and how clients are told about it.
//!
//! The core never sees the scale: its rectangles are logical, and a window sized 800 wide
//! is 800 logical pixels whatever the screen. The scale only matters when drawing and when
//! telling clients how finely they should draw.

use crate::compositor::State;
use crate::windows::Windows;
use smithay::delegate_fractional_scale;
use smithay::desktop::PopupManager;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle};
use smithay::wayland::compositor::{TraversalAction, with_surface_tree_downward};
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};
use std::num::NonZeroU32;

/// The protocol counts a scale in 120ths, so this is the whole precision there is.
const UNITS_PER_ONE: u32 = 120;
/// No screen needs more, and it keeps every product below inside `i64` with room to spare.
const MAX_UNITS: u32 = 16 * UNITS_PER_ONE;

/// How many physical pixels one logical pixel is, in 120ths, so 1.5 is 180.
///
/// Invariant: between 1/120 and 16 (the protocol's resolution and a bound no screen
/// reaches), so a scale is never zero, negative, infinite or not a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scale(NonZeroU32);

impl Scale {
    /// One physical pixel per logical pixel.
    pub const ONE: Self = match NonZeroU32::new(UNITS_PER_ONE) {
        Some(units) => Self(units),
        None => unreachable!(),
    };

    /// The scale closest to `factor`, to a 120th. `None` if `factor` is not a number, is
    /// not positive, rounds to nothing, or is beyond 16.
    #[must_use]
    pub fn from_factor(factor: f64) -> Option<Self> {
        if !factor.is_finite() || factor <= 0.0 {
            return None;
        }
        let units = (factor * f64::from(UNITS_PER_ONE)).round();
        if !(1.0..=f64::from(MAX_UNITS)).contains(&units) {
            return None;
        }
        // The range check above makes this a whole number between 1 and `MAX_UNITS`.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let units = units as u32;
        NonZeroU32::new(units).map(Self)
    }

    /// The scale as a factor, 1.5 for a host at 150%.
    #[must_use]
    pub fn factor(self) -> f64 {
        f64::from(self.0.get()) / f64::from(UNITS_PER_ONE)
    }

    /// The scale rounded up to a whole number, which is what `wl_output` can say.
    #[must_use]
    pub const fn integer(self) -> i32 {
        // At most `MAX_UNITS / 120`, which is 16.
        self.0.get().div_ceil(UNITS_PER_ONE) as i32
    }

    /// A logical length on the screen, in physical pixels, rounded to the nearest (halves
    /// up).
    pub(crate) fn to_physical(self, logical: i32) -> i32 {
        let scaled = i64::from(logical) * i64::from(self.0.get());
        clamp((scaled + i64::from(UNITS_PER_ONE) / 2).div_euclid(i64::from(UNITS_PER_ONE)))
    }

    /// A physical length as logical pixels, rounded to the nearest (halves up).
    pub(crate) fn to_logical(self, physical: i32) -> i32 {
        let scaled = i64::from(physical) * i64::from(UNITS_PER_ONE);
        let units = i64::from(self.0.get());
        clamp((scaled + units / 2).div_euclid(units))
    }

    /// A point as physical pixels.
    pub(crate) fn point_to_physical(self, point: Point<i32, Logical>) -> Point<i32, Physical> {
        (self.to_physical(point.x), self.to_physical(point.y)).into()
    }

    /// A rectangle as physical pixels. Each edge is rounded on its own, so rectangles that
    /// touch still touch and none gains or loses a pixel at a seam.
    pub(crate) fn rect_to_physical(
        self,
        rect: Rectangle<i32, Logical>,
    ) -> Rectangle<i32, Physical> {
        let (left, top) = (self.to_physical(rect.loc.x), self.to_physical(rect.loc.y));
        let right = self.to_physical(rect.loc.x.saturating_add(rect.size.w));
        let bottom = self.to_physical(rect.loc.y.saturating_add(rect.size.h));
        Rectangle::new(
            (left, top).into(),
            (
                right.saturating_sub(left).max(0),
                bottom.saturating_sub(top).max(0),
            )
                .into(),
        )
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::ONE
    }
}

fn clamp(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

impl State {
    /// Makes `scale` the one clients are told to draw at: every surface that exists now
    /// hears it, and every fractional-scale object bound later is told on creation.
    pub(crate) fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
        for root in surface_roots(&self.windows) {
            with_surface_tree_downward(
                &root,
                (),
                |_, _, ()| TraversalAction::DoChildren(()),
                |_, states, ()| {
                    with_fractional_scale(states, |fractional| {
                        fractional.set_preferred_scale(scale.factor());
                    });
                },
                |_, _, ()| true,
            );
        }
    }
}

/// Every toplevel, pending or managed, and every popup of one.
fn surface_roots(windows: &Windows) -> Vec<WlSurface> {
    let mut roots = windows.toplevel_surfaces();
    let popups: Vec<WlSurface> = roots
        .iter()
        .flat_map(PopupManager::popups_for_surface)
        .map(|(popup, _)| popup.wl_surface().clone())
        .collect();
    roots.extend(popups);
    roots
}

impl FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.scale.factor();
        smithay::wayland::compositor::with_states(&surface, |states| {
            with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale));
        });
    }
}

delegate_fractional_scale!(State);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn scale(factor: f64) -> Scale {
        Scale::from_factor(factor).expect("a usable scale")
    }

    #[test]
    fn a_factor_is_kept_to_a_120th() {
        assert_eq!(scale(1.5).factor(), 1.5);
        assert_eq!(scale(1.25).factor(), 1.25);
        assert_eq!(Scale::ONE.factor(), 1.0);
        assert_eq!(Scale::default(), Scale::ONE);
    }

    #[test]
    fn a_factor_that_is_no_scale_is_refused() {
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.001, 17.0] {
            assert_eq!(Scale::from_factor(factor), None, "{factor}");
        }
    }

    #[test]
    fn whole_numbers_round_up_for_the_output() {
        assert_eq!(Scale::ONE.integer(), 1);
        assert_eq!(scale(1.25).integer(), 2);
        assert_eq!(scale(1.5).integer(), 2);
        assert_eq!(scale(2.0).integer(), 2);
        assert_eq!(scale(2.01).integer(), 3);
    }

    #[test]
    fn lengths_round_to_the_nearest_pixel() {
        assert_eq!(scale(1.5).to_physical(3), 5); // 4.5 rounds up
        assert_eq!(scale(1.5).to_physical(-3), -4); // -4.5 rounds up
        assert_eq!(scale(1.5).to_physical(1000), 1500);
        assert_eq!(scale(1.5).to_logical(1500), 1000);
        assert_eq!(scale(1.5).to_logical(5), 3); // 3.33
    }

    #[test]
    fn huge_lengths_do_not_overflow() {
        assert_eq!(scale(16.0).to_physical(i32::MAX), i32::MAX);
        assert_eq!(scale(16.0).to_physical(i32::MIN), i32::MIN);
    }

    proptest! {
        /// Whole-pixel lengths survive a trip to physical and back at any scale of 1 or
        /// more, so a window never changes size by being measured twice.
        #[test]
        fn a_length_survives_the_round_trip(
            units in UNITS_PER_ONE..=MAX_UNITS,
            logical in -100_000i32..100_000,
        ) {
            let scale = Scale(NonZeroU32::new(units).expect("non-zero"));
            prop_assert_eq!(scale.to_logical(scale.to_physical(logical)), logical);
        }

        /// A rectangle's physical edges are the physical edges of its logical ones, so
        /// two rectangles that touch still touch.
        #[test]
        fn touching_rectangles_still_touch(
            units in 1u32..=MAX_UNITS,
            x in -5000i32..5000,
            first in 1i32..3000,
            second in 1i32..3000,
        ) {
            let scale = Scale(NonZeroU32::new(units).expect("non-zero"));
            let left = Rectangle::<i32, Logical>::new((x, 0).into(), (first, 10).into());
            let right = Rectangle::<i32, Logical>::new((x + first, 0).into(), (second, 10).into());
            let (a, b) = (scale.rect_to_physical(left), scale.rect_to_physical(right));
            prop_assert_eq!(a.loc.x + a.size.w, b.loc.x);
            prop_assert!(a.size.w >= 0 && b.size.w >= 0);
        }

        #[test]
        fn a_scale_from_a_factor_is_within_a_120th(factor in 0.01f64..16.0) {
            let scale = Scale::from_factor(factor).expect("in range");
            prop_assert!((scale.factor() - factor).abs() <= 0.5 / f64::from(UNITS_PER_ONE) + 1e-9);
        }
    }
}
