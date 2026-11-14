//! Where the connected screens go in the compositor's space, and what the user said about
//! them.
//!
//! Like [`super::pick`], these take no device: the order and the places are worked out from
//! plain numbers, so property tests can check them, and the code that opens the screens only
//! applies the answer.

use crate::error::{Error, Result};
use crate::scale::Scale;
use lutece::{OutputId, Rect};

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

/// How many names the table of output numbers keeps. A number is kept for a screen that is
/// away so it gets the same one back, and the table is bounded so a machine that sees many
/// monitors over a long session does not grow it for ever.
pub(super) const MAX_REMEMBERED: usize = 32;

/// What a change in the connected screens does to the ones shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScreenPlan {
    /// Every connected screen in the order it is laid out, with the number of its output.
    pub(super) layout: Vec<(String, OutputId)>,
    /// Screens shown now that are no longer connected.
    pub(super) closed: Vec<String>,
    /// Connected screens that are not shown yet, in the order laid out.
    pub(super) opened: Vec<String>,
}

impl ScreenPlan {
    /// Whether the plan leaves the shown screens exactly as they are.
    pub(super) fn is_unchanged(&self, shown: &[String]) -> bool {
        self.closed.is_empty()
            && self.opened.is_empty()
            && self.layout.iter().map(|(name, _)| name).eq(shown.iter())
    }
}

/// What to do when the connected screens are now `found`, with `shown` on the screen and the
/// output numbers of `remembered` (every screen seen so far, shown or not).
///
/// * A screen that stays keeps its output number, so the core does not see it as new.
/// * A screen that comes back gets the number it had, if nobody else has it now, so the
///   core shows it the workspace it had.
/// * A screen never seen gets the lowest number that no screen shown and no screen away
///   has, and only when every number is spoken for the lowest one no screen shown has.
/// * The order is the one [`order`] gives, so the settings still decide who is first.
pub(super) fn plan_screens(
    remembered: &[(String, OutputId)],
    shown: &[String],
    found: &[String],
    settings: &ScreenSettings,
) -> ScreenPlan {
    let names: Vec<&str> = found.iter().map(String::as_str).collect();
    let ordered: Vec<&str> = order(&names, settings)
        .into_iter()
        .map(|index| names[index])
        .collect();

    let closed = shown
        .iter()
        .filter(|name| !found.contains(name))
        .cloned()
        .collect();
    let opened = ordered
        .iter()
        .filter(|name| !shown.iter().any(|shown| shown == **name))
        .map(|name| (*name).to_owned())
        .collect();

    // First the numbers screens already have; a number two names claim goes to the first.
    let mut taken: Vec<u32> = Vec::new();
    let mut numbers: Vec<Option<u32>> = ordered
        .iter()
        .map(|name| {
            let number = remembered
                .iter()
                .find(|(known, _)| known == name)
                .map(|(_, id)| id.get())?;
            if taken.contains(&number) {
                return None;
            }
            taken.push(number);
            Some(number)
        })
        .collect();
    let spoken_for: Vec<u32> = remembered.iter().map(|(_, id)| id.get()).collect();
    for number in &mut numbers {
        if number.is_none() {
            let free = |candidate: &u32| !taken.contains(candidate);
            let fresh = (0u32..)
                .filter(free)
                .find(|candidate| !spoken_for.contains(candidate));
            // Every number is spoken for only when `remembered` is as long as `u32` allows,
            // which the bound on the table prevents; the fallback is for completeness.
            let chosen = fresh.unwrap_or_else(|| (0u32..).find(free).unwrap_or(0));
            taken.push(chosen);
            *number = Some(chosen);
        }
    }
    let layout = ordered
        .into_iter()
        .zip(numbers)
        .map(|(name, number)| (name.to_owned(), OutputId::new(number.unwrap_or(0))))
        .collect();
    ScreenPlan {
        layout,
        closed,
        opened,
    }
}

/// `remembered` with the screens of `layout` set to the numbers they have now, and no more
/// than [`MAX_REMEMBERED`] entries: the oldest that are not in `layout` are forgotten first.
pub(super) fn remember(
    remembered: &[(String, OutputId)],
    layout: &[(String, OutputId)],
) -> Vec<(String, OutputId)> {
    let mut table: Vec<(String, OutputId)> = remembered
        .iter()
        .filter(|(name, _)| layout.iter().all(|(shown, _)| shown != name))
        .cloned()
        .collect();
    let room = MAX_REMEMBERED.saturating_sub(layout.len());
    let excess = table.len().saturating_sub(room);
    table.drain(..excess);
    table.extend(layout.iter().cloned());
    table
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

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn table(list: &[(&str, u32)]) -> Vec<(String, OutputId)> {
        list.iter()
            .map(|(name, id)| ((*name).to_owned(), OutputId::new(*id)))
            .collect()
    }

    #[test]
    fn a_first_look_numbers_the_screens_in_the_order_laid_out() {
        let settings = ScreenSettings::new().with_screen("HDMIA-1", None);
        let plan = plan_screens(&[], &[], &names(&["eDP-1", "HDMIA-1"]), &settings);
        assert_eq!(plan.layout, table(&[("HDMIA-1", 0), ("eDP-1", 1)]));
        assert_eq!(plan.opened, names(&["HDMIA-1", "eDP-1"]));
        assert!(plan.closed.is_empty());
    }

    #[test]
    fn a_screen_that_stays_keeps_its_number_when_one_before_it_goes() {
        let known = table(&[("A", 0), ("B", 1)]);
        let plan = plan_screens(
            &known,
            &names(&["A", "B"]),
            &names(&["B"]),
            &ScreenSettings::new(),
        );
        assert_eq!(plan.layout, table(&[("B", 1)]));
        assert_eq!(plan.closed, names(&["A"]));
        assert!(plan.opened.is_empty());
    }

    #[test]
    fn a_screen_that_comes_back_gets_its_old_number() {
        let known = table(&[("A", 0), ("B", 1)]);
        let plan = plan_screens(
            &known,
            &names(&["B"]),
            &names(&["A", "B"]),
            &ScreenSettings::new(),
        );
        assert_eq!(plan.layout, table(&[("A", 0), ("B", 1)]));
        assert_eq!(plan.opened, names(&["A"]));
    }

    #[test]
    fn a_new_screen_does_not_take_the_number_of_one_that_is_only_away() {
        let known = table(&[("A", 0), ("B", 1)]);
        let plan = plan_screens(
            &known,
            &names(&["B"]),
            &names(&["B", "C"]),
            &ScreenSettings::new(),
        );
        assert_eq!(plan.layout, table(&[("B", 1), ("C", 2)]));
    }

    #[test]
    fn nothing_changed_is_said_so() {
        let known = table(&[("A", 0), ("B", 1)]);
        let shown = names(&["A", "B"]);
        let plan = plan_screens(&known, &shown, &shown, &ScreenSettings::new());
        assert!(plan.is_unchanged(&shown));
        let other_order = ScreenSettings::new().with_screen("B", None);
        assert!(!plan_screens(&known, &shown, &shown, &other_order).is_unchanged(&shown));
    }

    #[test]
    fn the_table_forgets_the_oldest_screen_that_is_away_when_it_is_full() {
        let old: Vec<(String, OutputId)> = (0..MAX_REMEMBERED as u32)
            .map(|n| (format!("old-{n}"), OutputId::new(n)))
            .collect();
        let layout = table(&[("new", 100)]);
        let kept = remember(&old, &layout);
        assert_eq!(kept.len(), MAX_REMEMBERED);
        assert_eq!(kept.last(), Some(&("new".to_owned(), OutputId::new(100))));
        assert!(
            kept.iter().all(|(name, _)| name != "old-0"),
            "the oldest went"
        );
        assert!(kept.iter().any(|(name, _)| name == "old-1"));
    }

    proptest! {
        /// Whatever came and went, the plan lays out exactly the connected screens, gives
        /// each its own output number, keeps the number of a screen that stays and says
        /// precisely which screens close and which open.
        #[test]
        fn a_plan_is_consistent_with_what_was_and_what_is(
            shown_mask in proptest::collection::vec(any::<bool>(), 6),
            found_mask in proptest::collection::vec(any::<bool>(), 6),
            named in proptest::collection::vec(0usize..6, 0..4),
        ) {
            let all: Vec<String> = (0..6).map(|n| format!("S{n}")).collect();
            let pick = |mask: &[bool]| -> Vec<String> {
                all.iter().zip(mask).filter(|(_, on)| **on).map(|(name, _)| name.clone()).collect()
            };
            let (shown, found) = (pick(&shown_mask), pick(&found_mask));
            let mut settings = ScreenSettings::new();
            for index in named {
                settings = settings.with_screen(all[index].clone(), None);
            }
            // The screens shown have numbers 0.. in the order they were laid out.
            let remembered: Vec<(String, OutputId)> = (0u32..)
                .zip(&shown)
                .map(|(number, name)| (name.clone(), OutputId::new(number)))
                .collect();

            let plan = plan_screens(&remembered, &shown, &found, &settings);

            let mut laid: Vec<&String> = plan.layout.iter().map(|(name, _)| name).collect();
            laid.sort();
            let mut expected: Vec<&String> = found.iter().collect();
            expected.sort();
            prop_assert_eq!(laid, expected, "exactly the connected screens");

            let mut numbers: Vec<u32> = plan.layout.iter().map(|(_, id)| id.get()).collect();
            numbers.sort_unstable();
            numbers.dedup();
            prop_assert_eq!(numbers.len(), plan.layout.len(), "numbers are unique");

            for (name, id) in &plan.layout {
                if let Some((_, before)) = remembered.iter().find(|(known, _)| known == name) {
                    prop_assert_eq!(id, before, "{} kept its number", name);
                }
            }
            for name in &plan.closed {
                prop_assert!(shown.contains(name) && !found.contains(name));
            }
            for name in &plan.opened {
                prop_assert!(found.contains(name) && !shown.contains(name));
            }
            // What stays is counted from both sides: the shown ones that did not close, and the
            // connected ones that did not open.
            prop_assert_eq!(
                plan.closed.len() + found.len() - plan.opened.len(),
                shown.len(),
                "what stays is counted once"
            );
        }

        /// The table never grows past its bound and always holds the screens laid out, with
        /// the numbers they have.
        #[test]
        fn the_table_stays_bounded_and_holds_the_present(
            old in 0usize..80,
            laid in 0usize..6,
        ) {
            let remembered: Vec<(String, OutputId)> = (0..old)
                .map(|n| (format!("old-{n}"), OutputId::new(u32::try_from(n).unwrap_or(0))))
                .collect();
            let layout: Vec<(String, OutputId)> = (0..laid)
                .map(|n| (format!("now-{n}"), OutputId::new(1000 + u32::try_from(n).unwrap_or(0))))
                .collect();
            let table = remember(&remembered, &layout);
            prop_assert!(table.len() <= MAX_REMEMBERED.max(layout.len()));
            for entry in &layout {
                prop_assert!(table.contains(entry));
            }
        }
    }
}
