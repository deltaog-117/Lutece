//! Screen geometry. Sizes are non-zero by construction, because X11 rejects
//! zero-sized windows and a Wayland surface of zero size is invisible.

use crate::shared::error::{Error, Result};
use std::num::NonZeroU32;

/// A position in screen coordinates. Any pair of values is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Point {
    /// Distance from the left edge.
    pub x: i32,
    /// Distance from the top edge.
    pub y: i32,
}

/// A width and height, both at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Size {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl Size {
    /// Builds a size.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSize`] if either dimension is zero.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        match (NonZeroU32::new(width), NonZeroU32::new(height)) {
            (Some(width), Some(height)) => Ok(Self { width, height }),
            _ => Err(Error::InvalidSize { width, height }),
        }
    }

    /// The width in pixels, at least 1.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    /// The height in pixels, at least 1.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }
}

/// A rectangle with a top-left origin and a non-zero size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Rect {
    origin: Point,
    size: Size,
}

impl Rect {
    /// Builds a rectangle from its parts.
    #[must_use]
    pub const fn from_parts(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// Builds a rectangle from raw numbers.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidSize`] if `width` or `height` is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::Rect;
    ///
    /// let r = Rect::new(10, 20, 640, 480)?;
    /// assert_eq!((r.x(), r.y(), r.width(), r.height()), (10, 20, 640, 480));
    /// assert!(Rect::new(0, 0, 0, 480).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        Ok(Self::from_parts(Point { x, y }, Size::new(width, height)?))
    }

    /// Left edge.
    #[must_use]
    pub const fn x(self) -> i32 {
        self.origin.x
    }

    /// Top edge.
    #[must_use]
    pub const fn y(self) -> i32 {
        self.origin.y
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.size.width()
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.size.height()
    }

    /// The top-left corner.
    #[must_use]
    pub const fn origin(self) -> Point {
        self.origin
    }

    /// The width and height.
    #[must_use]
    pub const fn size(self) -> Size {
        self.size
    }

    /// Whether `p` lies inside the rectangle; the right and bottom edges are outside.
    #[must_use]
    pub fn contains(self, p: Point) -> bool {
        // Widened to i64 so `x + width` cannot overflow near the i32 limits.
        let (px, py) = (i64::from(p.x), i64::from(p.y));
        let (x, y) = (i64::from(self.x()), i64::from(self.y()));
        px >= x && py >= y && px < x + i64::from(self.width()) && py < y + i64::from(self.height())
    }

    /// The rectangle with `insets` cut from its edges.
    ///
    /// Insets that would leave nothing are reduced until one pixel is left on each axis,
    /// so the result is always a valid rectangle inside `self`. A bar taller than its
    /// screen therefore cannot make the screen vanish.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{Insets, Rect};
    ///
    /// let screen = Rect::new(0, 0, 1920, 1080)?;
    /// let below_a_bar = screen.inset(Insets::new(0, 30, 0, 0));
    /// assert_eq!(below_a_bar, Rect::new(0, 30, 1920, 1050)?);
    /// # Ok::<(), lutece::Error>(())
    /// ```
    #[must_use]
    pub fn inset(self, insets: Insets) -> Self {
        let (x, width) = shrink(self.x(), self.width(), insets.left, insets.right);
        let (y, height) = shrink(self.y(), self.height(), insets.top, insets.bottom);
        Self::from_parts(
            Point { x, y },
            Size {
                width: NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN),
                height: NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN),
            },
        )
    }
}

/// Cuts `near` from the start and `far` from the end of one side, keeping one pixel at
/// least. `length` is at least 1, because every [`Size`] is.
fn shrink(origin: i32, length: u32, near: u32, far: u32) -> (i32, u32) {
    let near = near.min(length - 1);
    let far = far.min(length - 1 - near);
    // Saturating keeps a rectangle that already reaches the coordinate limit valid.
    let origin = i32::try_from(i64::from(origin) + i64::from(near)).unwrap_or(i32::MAX);
    (origin, length - near - far)
}

/// Space a bar or panel takes from each edge of a screen, in pixels. Any four values are
/// valid; [`Rect::inset`] keeps the result inside the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Insets {
    /// Taken from the left edge.
    pub left: u32,
    /// Taken from the top edge.
    pub top: u32,
    /// Taken from the right edge.
    pub right: u32,
    /// Taken from the bottom edge.
    pub bottom: u32,
}

impl Insets {
    /// Nothing reserved.
    pub const NONE: Self = Self::new(0, 0, 0, 0);

    /// Builds insets from the four edges, in the order left, top, right, bottom.
    #[must_use]
    pub const fn new(left: u32, top: u32, right: u32, bottom: u32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Whether nothing is reserved.
    #[must_use]
    pub fn is_none(self) -> bool {
        self == Self::NONE
    }

    /// These insets, except that on an axis where they would take more than half of
    /// `area`, that axis is left alone.
    ///
    /// Any client can ask to reserve space, so a bar that claims nearly the whole screen
    /// must not leave the tiled windows with no room. Ignoring such a claim, instead of
    /// honouring part of it, keeps the screen usable and the rule easy to state.
    #[must_use]
    pub fn limited_to(self, area: Rect) -> Self {
        let fits = |near: u32, far: u32, length: u32| {
            u64::from(near) + u64::from(far) <= u64::from(length / 2)
        };
        let (left, right) = if fits(self.left, self.right, area.width()) {
            (self.left, self.right)
        } else {
            (0, 0)
        };
        let (top, bottom) = if fits(self.top, self.bottom, area.height()) {
            (self.top, self.bottom)
        } else {
            (0, 0)
        };
        Self::new(left, top, right, bottom)
    }

    /// The larger of the two on each edge. Two bars on the same edge both start at the
    /// screen's edge, so the space they reserve together is the larger one, not the sum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn size_accepts_exactly_the_non_zero_pairs(w in any::<u32>(), h in any::<u32>()) {
            prop_assert_eq!(Size::new(w, h).is_ok(), w != 0 && h != 0);
        }

        #[test]
        fn rect_keeps_what_it_was_given(
            x in any::<i32>(), y in any::<i32>(), w in 1..=u32::MAX, h in 1..=u32::MAX,
        ) {
            let r = Rect::new(x, y, w, h).expect("dimensions are non-zero");
            prop_assert_eq!((r.x(), r.y(), r.width(), r.height()), (x, y, w, h));
        }

        #[test]
        fn rect_contains_its_origin_but_not_its_far_corner(
            x in any::<i32>(), y in any::<i32>(), w in 1..=u32::MAX, h in 1..=u32::MAX,
        ) {
            let r = Rect::new(x, y, w, h).expect("dimensions are non-zero");
            prop_assert!(r.contains(r.origin()));
            let far = Point {
                x: i32::try_from(i64::from(x) + i64::from(w)).unwrap_or(i32::MAX),
                y: i32::try_from(i64::from(y) + i64::from(h)).unwrap_or(i32::MAX),
            };
            // Clamping to i32::MAX can land inside; only assert when it was exact.
            if i64::from(far.x) == i64::from(x) + i64::from(w)
                && i64::from(far.y) == i64::from(y) + i64::from(h)
            {
                prop_assert!(!r.contains(far));
            }
        }

        #[test]
        fn an_inset_rect_stays_inside_the_original_and_is_never_empty(
            x in -100_000_i32..100_000, y in -100_000_i32..100_000,
            w in 1_u32..10_000, h in 1_u32..10_000,
            left in any::<u32>(), top in any::<u32>(), right in any::<u32>(), bottom in any::<u32>(),
        ) {
            let r = Rect::new(x, y, w, h).expect("dimensions are non-zero");
            let inner = r.inset(Insets::new(left, top, right, bottom));
            prop_assert!(inner.x() >= r.x() && inner.y() >= r.y());
            prop_assert!(
                i64::from(inner.x()) + i64::from(inner.width()) <= i64::from(r.x()) + i64::from(w)
            );
            prop_assert!(
                i64::from(inner.y()) + i64::from(inner.height()) <= i64::from(r.y()) + i64::from(h)
            );
        }

        #[test]
        fn insets_that_fit_are_cut_exactly(
            w in 100_u32..10_000, h in 100_u32..10_000,
            left in 0_u32..40, top in 0_u32..40, right in 0_u32..40, bottom in 0_u32..40,
        ) {
            let r = Rect::new(7, -3, w, h).expect("dimensions are non-zero");
            let inner = r.inset(Insets::new(left, top, right, bottom));
            prop_assert_eq!(inner.width(), w - left - right);
            prop_assert_eq!(inner.height(), h - top - bottom);
            prop_assert_eq!(i64::from(inner.x()), 7 + i64::from(left));
            prop_assert_eq!(i64::from(inner.y()), -3 + i64::from(top));
        }

        #[test]
        fn no_insets_change_nothing(
            x in any::<i32>(), y in any::<i32>(), w in 1..=u32::MAX, h in 1..=u32::MAX,
        ) {
            let r = Rect::new(x, y, w, h).expect("dimensions are non-zero");
            prop_assert_eq!(r.inset(Insets::NONE), r);
        }
    }

    proptest! {
        #[test]
        fn limited_insets_always_leave_at_least_half_of_each_side(
            w in 1_u32..10_000, h in 1_u32..10_000,
            left in any::<u32>(), top in any::<u32>(), right in any::<u32>(), bottom in any::<u32>(),
        ) {
            let r = Rect::new(0, 0, w, h).expect("dimensions are non-zero");
            let inner = r.inset(Insets::new(left, top, right, bottom).limited_to(r));
            prop_assert!(inner.width() >= w - w / 2);
            prop_assert!(inner.height() >= h - h / 2);
        }
    }

    #[test]
    fn a_claim_on_most_of_an_axis_is_ignored_on_that_axis_only() {
        let r = Rect::new(0, 0, 1000, 600).expect("non-zero size");
        let claim = Insets::new(0, 30, 0, 0);
        assert_eq!(claim.limited_to(r), claim);
        assert_eq!(Insets::new(0, 400, 0, 0).limited_to(r), Insets::NONE);
        assert_eq!(
            Insets::new(10, 400, 10, 0).limited_to(r),
            Insets::new(10, 0, 10, 0)
        );
    }

    #[test]
    fn the_larger_inset_wins_on_each_edge() {
        let merged = Insets::new(5, 30, 0, 2).max(Insets::new(0, 20, 7, 2));
        assert_eq!(merged, Insets::new(5, 30, 7, 2));
    }
}
