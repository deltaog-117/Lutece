//! Floating windows: the layout leaves them alone and they sit above the tiled ones.
//!
//! A [`Float`] does not store a rectangle. It stores how far the window was moved and
//! resized from a default rectangle centered in the screen, and [`Float::rect`] works the
//! rectangle out when the scene is built. Because of that, a floating window is valid
//! before the backend has reported a screen and stays on screen when the screen changes.

use crate::shared::error::Result;
use crate::shared::geometry::{Point, Rect, Size};

/// A floating window starts at this share of the screen's width and height.
const DEFAULT_PERCENT: i64 = 60;

/// The smallest width or height a floating window can be resized to, so it never
/// shrinks to something that cannot be grabbed again.
const MIN_SIDE: i64 = 64;

/// How a floating window differs from the default rectangle, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Float {
    dx: i32,
    dy: i32,
    dw: i32,
    dh: i32,
}

impl Float {
    /// A float from how far it was moved and resized, as `parts` gives them.
    pub(crate) const fn from_parts(dx: i32, dy: i32, dw: i32, dh: i32) -> Self {
        Self { dx, dy, dw, dh }
    }

    /// How far the window was moved to the right and down, and made wider and taller.
    pub(crate) const fn parts(self) -> (i32, i32, i32, i32) {
        (self.dx, self.dy, self.dw, self.dh)
    }

    /// The window moved by `dx` and `dy`; positive goes right and down.
    pub(crate) fn moved(self, dx: i32, dy: i32) -> Self {
        Self {
            dx: self.dx.saturating_add(dx),
            dy: self.dy.saturating_add(dy),
            ..self
        }
    }

    /// The window grown by `dw` and `dh` inside `area`; negative values shrink it.
    ///
    /// The change stops at the smallest size. Keeping the offset at that limit, instead
    /// of letting it run past, means one step back out always grows the window again.
    pub(crate) fn resized(self, area: Rect, dw: i32, dh: i32) -> Self {
        let grow = |current: i32, delta: i32, side: u32| {
            let floor = MIN_SIDE - default_side(side);
            clamp_i32((i64::from(current) + i64::from(delta)).max(floor))
        };
        Self {
            dw: grow(self.dw, dw, area.width()),
            dh: grow(self.dh, dh, area.height()),
            ..self
        }
    }

    /// The window's rectangle for a screen `area`. Moving it off the screen is allowed,
    /// so the position is clamped to the coordinate range instead of refused.
    pub(crate) fn rect(self, area: Rect) -> Result<Rect> {
        let side = |size: u32, delta: i32| {
            let wanted = (default_side(size) + i64::from(delta)).max(MIN_SIDE);
            u32::try_from(wanted).unwrap_or(u32::MAX)
        };
        let centered = |start: i32, size: u32, delta: i32| {
            let margin = (i64::from(size) - default_side(size)) / 2;
            clamp_i32(i64::from(start) + margin + i64::from(delta))
        };
        let origin = Point {
            x: centered(area.x(), area.width(), self.dx),
            y: centered(area.y(), area.height(), self.dy),
        };
        let size = Size::new(side(area.width(), self.dw), side(area.height(), self.dh))?;
        Ok(Rect::from_parts(origin, size))
    }
}

fn default_side(side: u32) -> i64 {
    i64::from(side) * DEFAULT_PERCENT / 100
}

fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn area(width: u32, height: u32) -> Rect {
        Rect::new(0, 0, width, height).expect("non-zero size")
    }

    #[test]
    fn a_new_float_is_centered_at_the_default_share() {
        let rect = Float::default().rect(area(1000, 600)).expect("valid");
        assert_eq!(rect, Rect::new(200, 120, 600, 360).expect("non-zero size"));
    }

    #[test]
    fn moving_and_resizing_shift_the_rectangle() {
        let float = Float::default()
            .moved(10, -20)
            .resized(area(1000, 600), 40, -40);
        let rect = float.rect(area(1000, 600)).expect("valid");
        assert_eq!(rect, Rect::new(210, 100, 640, 320).expect("non-zero size"));
    }

    #[test]
    fn the_rectangle_follows_the_screen_it_is_asked_for() {
        let float = Float::default().moved(5, 5);
        let small = float.rect(area(500, 300)).expect("valid");
        let big = float.rect(area(1000, 600)).expect("valid");
        assert_eq!((small.width(), big.width()), (300, 600));
    }

    proptest! {
        #[test]
        fn any_float_on_any_screen_is_a_valid_rectangle(
            x in any::<i32>(), y in any::<i32>(),
            width in 1..=u32::MAX, height in 1..=u32::MAX,
            dx in any::<i32>(), dy in any::<i32>(), dw in any::<i32>(), dh in any::<i32>(),
        ) {
            let screen = Rect::new(x, y, width, height).expect("dimensions are non-zero");
            let float = Float::default().moved(dx, dy).resized(screen, dw, dh);
            let rect = float.rect(screen).expect("a float always has a valid size");
            prop_assert!(i64::from(rect.width()) >= MIN_SIDE);
            prop_assert!(i64::from(rect.height()) >= MIN_SIDE);
        }

        #[test]
        fn moving_back_undoes_a_move(dx in -100_000..100_000, dy in -100_000..100_000) {
            let float = Float::default().moved(7, 9);
            prop_assert_eq!(float.moved(dx, dy).moved(-dx, -dy), float);
        }

        #[test]
        fn shrinking_stops_at_the_floor_and_one_step_back_grows_again(
            width in 200_u32..5000, height in 200_u32..5000, step in 1_i32..100,
        ) {
            let screen = area(width, height);
            let mut float = Float::default();
            for _ in 0..10_000 {
                float = float.resized(screen, -step, -step);
            }
            let floor = float.rect(screen).expect("valid");
            prop_assert_eq!(i64::from(floor.width()), MIN_SIDE);
            prop_assert_eq!(i64::from(floor.height()), MIN_SIDE);

            let regrown = float.resized(screen, step, step).rect(screen).expect("valid");
            prop_assert_eq!(i64::from(regrown.width()), MIN_SIDE + i64::from(step));
        }
    }
}
