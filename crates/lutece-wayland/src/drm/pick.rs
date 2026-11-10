//! The choices the hardware session makes from plain numbers: which mode a screen runs,
//! which scanout engine drives it, and what colour the test pattern shows.
//!
//! They take no device, so they are checked by property tests; the code that reads the
//! device and applies them is in [`super::pattern`].

/// One video mode a connector offers, reduced to what the choice looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Refresh rate in hertz, rounded.
    pub refresh_hz: u32,
    /// Whether the monitor marks the mode as its preferred one.
    pub preferred: bool,
}

impl ModeInfo {
    fn area(self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }
}

/// Index of the mode to run: the first the monitor prefers, otherwise the largest picture,
/// then the highest refresh rate, then the earliest listed. `None` only for no modes.
///
/// A monitor's own preference is trusted first because it knows its native size; the
/// fallback is only for monitors that mark none.
#[must_use]
pub fn best_mode(modes: &[ModeInfo]) -> Option<usize> {
    if let Some(index) = modes.iter().position(|mode| mode.preferred) {
        return Some(index);
    }
    modes
        .iter()
        .enumerate()
        // `max_by_key` keeps the last of equal keys, so the index is reversed to keep the
        // earliest instead.
        .max_by_key(|(index, mode)| (mode.area(), mode.refresh_hz, usize::MAX - index))
        .map(|(index, _)| index)
}

/// Index of the first scanout engine (CRTC) that an encoder can drive and that no other
/// screen already uses.
///
/// `possible` and `taken` are bit masks over the device's CRTC list, bit `i` standing for
/// the `i`-th; `count` is how many exist, so bits past it are ignored.
#[must_use]
pub fn pick_crtc(possible: u32, taken: u32, count: usize) -> Option<usize> {
    let free = possible & !taken;
    (0..count.min(32)).find(|index| free & (1 << index) != 0)
}

/// The colour of frame `frame` of `frames`, as red, green, blue and alpha in `0.0..=1.0`.
///
/// The hue turns once over the run, so a stuck picture is told from a live one by eye: a
/// frozen colour means the page flips stopped.
#[must_use]
pub fn pattern_color(frame: u32, frames: u32) -> [f32; 4] {
    let turn = if frames == 0 {
        0.0
    } else {
        (f64::from(frame) / f64::from(frames)).clamp(0.0, 1.0)
    };
    let sector = turn * 6.0;
    // Fraction of the way through the current sixth of the colour wheel.
    let rise = (sector % 1.0) as f32;
    let fall = 1.0 - rise;
    let (red, green, blue) = match sector as u32 {
        0 => (1.0, rise, 0.0),
        1 => (fall, 1.0, 0.0),
        2 => (0.0, 1.0, rise),
        3 => (0.0, fall, 1.0),
        4 => (rise, 0.0, 1.0),
        _ => (1.0, 0.0, fall),
    };
    [red, green, blue, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn mode() -> impl Strategy<Value = ModeInfo> {
        (1u16..8000, 1u16..8000, 1u32..500, any::<bool>()).prop_map(
            |(width, height, refresh_hz, preferred)| ModeInfo {
                width,
                height,
                refresh_hz,
                preferred,
            },
        )
    }

    proptest! {
        #[test]
        fn a_mode_is_chosen_exactly_when_one_is_listed(modes in prop::collection::vec(mode(), 0..16)) {
            let chosen = best_mode(&modes);
            prop_assert_eq!(chosen.is_some(), !modes.is_empty());
            if let Some(index) = chosen {
                prop_assert!(index < modes.len());
            }
        }

        #[test]
        fn the_first_preferred_mode_wins(modes in prop::collection::vec(mode(), 1..16)) {
            if let Some(first) = modes.iter().position(|mode| mode.preferred) {
                prop_assert_eq!(best_mode(&modes), Some(first));
            }
        }

        #[test]
        fn without_a_preference_no_other_mode_is_larger(
            modes in prop::collection::vec(mode(), 1..16)
        ) {
            let modes: Vec<_> = modes.into_iter().map(|m| ModeInfo { preferred: false, ..m }).collect();
            let index = best_mode(&modes).unwrap();
            prop_assert!(modes.iter().all(|other| other.area() <= modes[index].area()));
            // Among the largest, none refreshes faster, and none listed earlier ties.
            let best = modes[index];
            for (other_index, other) in modes.iter().enumerate() {
                if other.area() == best.area() {
                    prop_assert!(other.refresh_hz <= best.refresh_hz);
                    if other.refresh_hz == best.refresh_hz {
                        prop_assert!(other_index >= index);
                    }
                }
            }
        }

        #[test]
        fn a_crtc_is_possible_free_and_in_range(possible: u32, taken: u32, count in 0usize..40) {
            if let Some(index) = pick_crtc(possible, taken, count) {
                prop_assert!(index < count.min(32));
                prop_assert!(possible & (1 << index) != 0);
                prop_assert!(taken & (1 << index) == 0);
                // The earliest such one.
                for earlier in 0..index {
                    prop_assert!((possible & !taken) & (1 << earlier) == 0);
                }
            } else {
                for index in 0..count.min(32) {
                    prop_assert!((possible & !taken) & (1 << index) == 0);
                }
            }
        }

        #[test]
        fn the_pattern_stays_a_valid_opaque_colour(frame: u32, frames: u32) {
            let [red, green, blue, alpha] = pattern_color(frame, frames);
            for channel in [red, green, blue] {
                prop_assert!((0.0..=1.0).contains(&channel));
            }
            prop_assert!((alpha - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn the_pattern_starts_red_and_changes() {
        assert_eq!(pattern_color(0, 60), [1.0, 0.0, 0.0, 1.0]);
        assert_ne!(pattern_color(10, 60), pattern_color(30, 60));
    }

    #[test]
    fn no_modes_means_no_choice() {
        assert_eq!(best_mode(&[]), None);
        assert_eq!(pick_crtc(0, 0, 4), None);
    }
}
