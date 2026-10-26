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
    }
}
