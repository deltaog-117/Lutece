//! Where the connected screens go in the compositor's space, and what the user said about
//! them.
//!
//! Like [`super::pick`], these take no device: the order and the places are worked out from
//! plain numbers, so property tests can check them, and the code that opens the screens only
//! applies the answer.

use crate::error::{Error, Result};
use crate::scale::Scale;
use lutece::Rect;

/// What the user said about the screens: which come first, and how finely each draws.
///
/// A screen is named as it is printed when the session starts, for example `HDMIA-1` or
/// `eDP-1`. The screens named come first, left to right, in the order they are named, so the
/// first one named shows the first workspace; a connected screen that is not named follows,
/// in the order the machine lists them, at a scale of one. A name that matches no connected
/// screen is ignored, so one configuration can serve a laptop with and without its dock.
///
/// # Examples
///
/// ```
/// use lutece_wayland::{Scale, ScreenSettings};
///
/// let settings = ScreenSettings::new()
///     .with_screen("eDP-1", Scale::from_factor(1.5))
///     .with_screen("HDMIA-1", None);
/// assert_eq!(settings.scale_of("eDP-1"), Scale::from_factor(1.5));
/// assert_eq!(settings.scale_of("HDMIA-1"), None);
/// assert_eq!(settings.names().collect::<Vec<_>>(), ["eDP-1", "HDMIA-1"]);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenSettings {
    /// In the order named. A name appears once: the first mention wins.
    screens: Vec<(String, Option<Scale>)>,
}

impl ScreenSettings {
    /// Settings that say nothing: the machine's order, every screen at a scale of one.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Names a screen, after the ones named before, with a scale of its own or, with `None`,
    /// the usual one. A name already given is not changed.
    #[must_use]
    pub fn with_screen(mut self, name: impl Into<String>, scale: Option<Scale>) -> Self {
        let name = name.into();
        if self.screens.iter().all(|(known, _)| *known != name) {
            self.screens.push((name, scale));
        }
        self
    }

    /// The names, in the order they were given.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.screens.iter().map(|(name, _)| name.as_str())
    }

    /// The scale given to `name`, if it was named with one.
    #[must_use]
    pub fn scale_of(&self, name: &str) -> Option<Scale> {
        self.screens
            .iter()
            .find(|(known, _)| known == name)
            .and_then(|(_, scale)| *scale)
    }

    /// Where `name` was given, if it was.
    fn place_of(&self, name: &str) -> Option<usize> {
        self.screens.iter().position(|(known, _)| known == name)
    }
}

/// The order the connected screens are laid out in, as indexes into `found` (the names in
/// the order the machine lists them): the named screens first, in the order named, then the
/// rest as listed.
pub(super) fn order(found: &[&str], settings: &ScreenSettings) -> Vec<usize> {
    let mut indexes: Vec<usize> = (0..found.len()).collect();
    // A stable sort, so the screens that are not named keep the machine's order.
    indexes.sort_by_key(|&index| settings.place_of(found[index]).unwrap_or(usize::MAX));
    indexes
}

/// The rectangle each screen covers in the compositor's space, left to right with no gap and
/// no overlap, all starting at the top. `screens` holds each one's size in pixels and its
/// scale; the rectangle is in logical pixels, which is what the core lays windows out in, so
/// a screen at a scale of two is half as wide and as high as its picture.
///
/// # Errors
///
/// Returns [`Error::Drm`] if the screens together are wider than the coordinates reach,
/// which no real set of screens is.
pub(super) fn arrange(screens: &[([u16; 2], Scale)]) -> Result<Vec<Rect>> {
    let mut left: i64 = 0;
    let mut rects = Vec::with_capacity(screens.len());
    for &([width, height], scale) in screens {
        // A screen at a large scale is never zero pixels: a rectangle must have an area.
        let logical =
            |pixels: u16| u32::try_from(scale.to_logical(i32::from(pixels)).max(1)).unwrap_or(1);
        let (wide, high) = (logical(width), logical(height));
        let x = i32::try_from(left).map_err(|_| too_wide())?;
        rects.push(Rect::new(x, 0, wide, high)?);
        left += i64::from(wide);
    }
    i32::try_from(left).map_err(|_| too_wide())?;
    Ok(rects)
}

fn too_wide() -> Error {
    Error::Drm("the screens together are wider than the coordinates reach".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn scale(factor: f64) -> Scale {
        Scale::from_factor(factor).expect("a usable scale")
    }

    fn screen() -> impl Strategy<Value = ([u16; 2], Scale)> {
        (1u16..8000, 1u16..8000, 1u32..=8)
            .prop_map(|(w, h, quarters)| ([w, h], scale(f64::from(quarters) / 2.0)))
    }

    proptest! {
        /// One rectangle per screen, the first at the corner, each starting where the one
        /// before ends, so none overlaps and none leaves a gap.
        #[test]
        fn screens_tile_from_left_to_right(screens in prop::collection::vec(screen(), 0..6)) {
            let rects = arrange(&screens).expect("these fit");
            prop_assert_eq!(rects.len(), screens.len());
            let mut expected_x = 0i64;
            for rect in &rects {
                prop_assert_eq!(i64::from(rect.x()), expected_x);
                prop_assert_eq!(rect.y(), 0);
                expected_x += i64::from(rect.width());
            }
        }

        /// A rectangle is the picture's size in logical pixels: at a scale of one it is the
        /// picture, and a larger scale never makes it larger.
        #[test]
        fn a_rectangle_is_the_picture_in_logical_pixels(screens in prop::collection::vec(screen(), 1..6)) {
            let rects = arrange(&screens).expect("these fit");
            for (rect, ([width, height], scale)) in rects.iter().zip(&screens) {
                prop_assert_eq!(
                    i64::from(rect.width()),
                    i64::from(scale.to_logical(i32::from(*width)).max(1))
                );
                prop_assert_eq!(
                    i64::from(rect.height()),
                    i64::from(scale.to_logical(i32::from(*height)).max(1))
                );
                if *scale == Scale::ONE {
                    prop_assert_eq!(rect.width(), u32::from(*width));
                    prop_assert_eq!(rect.height(), u32::from(*height));
                }
            }
        }

        /// The order is each screen once; the named ones lead in the order named, and the
        /// others follow as listed.
        #[test]
        fn the_named_screens_lead_in_the_order_named(
            found in prop::collection::vec("[A-D][0-3]", 0..8),
            named in prop::collection::vec("[A-D][0-3]", 0..8),
        ) {
            // Names on the machine are unique.
            let mut unique: Vec<String> = Vec::new();
            for name in found {
                if !unique.contains(&name) {
                    unique.push(name);
                }
            }
            let names: Vec<&str> = unique.iter().map(String::as_str).collect();
            let settings = named
                .iter()
                .fold(ScreenSettings::new(), |s, name| s.with_screen(name.as_str(), None));

            let sequence = order(&names, &settings);

            let mut sorted = sequence.clone();
            sorted.sort_unstable();
            prop_assert_eq!(sorted, (0..names.len()).collect::<Vec<_>>());

            let places: Vec<Option<usize>> =
                sequence.iter().map(|&i| settings.place_of(names[i])).collect();
            let named_count = places.iter().flatten().count();
            // Every named screen comes before every unnamed one, and in the order named.
            prop_assert!(places[..named_count].iter().all(Option::is_some));
            prop_assert!(places[..named_count].windows(2).all(|pair| pair[0] < pair[1]));
            // The unnamed ones keep the machine's order.
            let rest = &sequence[named_count..];
            prop_assert!(rest.windows(2).all(|pair| pair[0] < pair[1]));
        }

        /// A name given twice keeps its first scale and its first place.
        #[test]
        fn the_first_mention_of_a_screen_wins(first in 1u32..9, second in 1u32..9) {
            let settings = ScreenSettings::new()
                .with_screen("A", Scale::from_factor(f64::from(first)))
                .with_screen("B", None)
                .with_screen("A", Scale::from_factor(f64::from(second)));
            prop_assert_eq!(settings.scale_of("A"), Scale::from_factor(f64::from(first)));
            prop_assert_eq!(settings.names().collect::<Vec<_>>(), ["A", "B"]);
        }
    }

    #[test]
    fn two_screens_of_different_height_are_top_aligned() {
        let rects =
            arrange(&[([1920, 1080], Scale::ONE), ([3840, 2160], scale(2.0))]).expect("these fit");
        assert_eq!((rects[0].x(), rects[0].y()), (0, 0));
        assert_eq!((rects[0].width(), rects[0].height()), (1920, 1080));
        assert_eq!((rects[1].x(), rects[1].y()), (1920, 0));
        assert_eq!((rects[1].width(), rects[1].height()), (1920, 1080));
    }

    #[test]
    fn a_name_that_matches_nothing_changes_nothing() {
        let settings = ScreenSettings::new().with_screen("DP-9", None);
        assert_eq!(order(&["eDP-1", "HDMIA-1"], &settings), [0, 1]);
    }

    #[test]
    fn naming_the_second_screen_puts_it_first() {
        let settings = ScreenSettings::new().with_screen("HDMIA-1", None);
        assert_eq!(order(&["eDP-1", "HDMIA-1"], &settings), [1, 0]);
    }

    #[test]
    fn screens_too_wide_to_place_are_refused_not_wrapped() {
        // Each is 65535 pixels at 1/120: 7.8 million logical pixels wide; enough of them
        // pass the range of i32.
        let huge = (
            [u16::MAX, 100],
            Scale::from_factor(1.0 / 120.0).expect("the smallest"),
        );
        assert!(arrange(&vec![huge; 400]).is_err());
    }
}
