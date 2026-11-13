//! Keyboard and pointer settings, checked once when they are built.
//!
//! A value outside the range a setting makes sense in cannot be held by [`InputSettings`], so
//! the code that applies it never has to ask. A device setting left as `None` is not touched:
//! the device keeps what its driver chose, which is what the machine did before there were
//! settings.

use thiserror::Error;

/// How long a key is held before it repeats, as most desktops default to it.
pub(crate) const DEFAULT_REPEAT_DELAY_MS: u32 = 600;
/// How many times a second a held key repeats, as most desktops default to it.
pub(crate) const DEFAULT_REPEAT_RATE: u32 = 25;

/// Shortest and longest delay before a key repeats; below the first a key repeats while it is
/// still being pressed, above the second it seems broken.
const REPEAT_DELAY_RANGE: std::ops::RangeInclusive<u32> = 100..=5000;
/// Fewest and most repeats a second; 0 turns repeating off, which the protocol allows.
const REPEAT_RATE_RANGE: std::ops::RangeInclusive<u32> = 0..=200;
/// The range libinput takes for the pointer's speed: -1 is slowest, 1 fastest.
const POINTER_SPEED_RANGE: std::ops::RangeInclusive<f64> = -1.0..=1.0;

/// Why a setting was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
pub enum SettingsError {
    /// The delay before a key repeats is outside 100 to 5000 milliseconds.
    #[error("the repeat delay must be 100 to 5000 ms, not {0}")]
    RepeatDelay(u32),
    /// The repeat rate is outside 0 to 200 a second.
    #[error("the repeat rate must be 0 to 200 a second, not {0}")]
    RepeatRate(u32),
    /// The pointer speed is outside -1 to 1, or is not a number.
    #[error("the pointer speed must be from -1 to 1, not {0}")]
    PointerSpeed(f64),
}

/// How the keyboard repeats and how the pointing devices behave.
///
/// The repeat settings go to every client, on the nested window and on the machine's screen.
/// The device settings need libinput, so only the machine's own screen uses them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputSettings {
    repeat_delay_ms: u32,
    repeat_rate: u32,
    tap_to_click: Option<bool>,
    natural_scroll: Option<bool>,
    pointer_speed: Option<f64>,
}

impl Default for InputSettings {
    /// What the backend did before there were settings: the usual repeat, and every device as
    /// its driver left it.
    fn default() -> Self {
        Self {
            repeat_delay_ms: DEFAULT_REPEAT_DELAY_MS,
            repeat_rate: DEFAULT_REPEAT_RATE,
            tap_to_click: None,
            natural_scroll: None,
            pointer_speed: None,
        }
    }
}

impl InputSettings {
    /// Builds settings, refusing a value outside its range. `None` for a device setting
    /// leaves the device as it is.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError`] naming the first value that is out of range.
    pub fn try_new(
        repeat_delay_ms: u32,
        repeat_rate: u32,
        tap_to_click: Option<bool>,
        natural_scroll: Option<bool>,
        pointer_speed: Option<f64>,
    ) -> Result<Self, SettingsError> {
        if !REPEAT_DELAY_RANGE.contains(&repeat_delay_ms) {
            return Err(SettingsError::RepeatDelay(repeat_delay_ms));
        }
        if !REPEAT_RATE_RANGE.contains(&repeat_rate) {
            return Err(SettingsError::RepeatRate(repeat_rate));
        }
        // A NaN is in no range, so it is refused here too.
        if let Some(speed) = pointer_speed
            && !POINTER_SPEED_RANGE.contains(&speed)
        {
            return Err(SettingsError::PointerSpeed(speed));
        }
        Ok(Self {
            repeat_delay_ms,
            repeat_rate,
            tap_to_click,
            natural_scroll,
            pointer_speed,
        })
    }

    /// Milliseconds a key is held before it repeats.
    #[must_use]
    pub const fn repeat_delay_ms(&self) -> u32 {
        self.repeat_delay_ms
    }

    /// Repeats a second once a key repeats; 0 means it does not.
    #[must_use]
    pub const fn repeat_rate(&self) -> u32 {
        self.repeat_rate
    }

    /// Whether a tap on a touchpad is a click, or `None` to leave each touchpad as it is.
    #[must_use]
    pub const fn tap_to_click(&self) -> Option<bool> {
        self.tap_to_click
    }

    /// Whether scrolling moves the content with the fingers, or `None` to leave each device
    /// as it is.
    #[must_use]
    pub const fn natural_scroll(&self) -> Option<bool> {
        self.natural_scroll
    }

    /// The pointer's speed from -1 to 1, or `None` to leave each device as it is.
    #[must_use]
    pub const fn pointer_speed(&self) -> Option<f64> {
        self.pointer_speed
    }

    /// The rate and delay in the order and type the seat takes them. Both are small, so the
    /// conversion never saturates.
    pub(crate) fn repeat_info(&self) -> (i32, i32) {
        (
            i32::try_from(self.repeat_rate).unwrap_or(i32::MAX),
            i32::try_from(self.repeat_delay_ms).unwrap_or(i32::MAX),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn the_default_is_what_the_backend_did_before_settings() {
        let settings = InputSettings::default();
        assert_eq!(settings.repeat_info(), (25, 600));
        assert_eq!(settings.tap_to_click(), None);
        assert_eq!(settings.natural_scroll(), None);
        assert_eq!(settings.pointer_speed(), None);
    }

    #[test]
    fn a_pointer_speed_that_is_not_a_number_is_refused() {
        let refused = InputSettings::try_new(600, 25, None, None, Some(f64::NAN));
        assert!(matches!(refused, Err(SettingsError::PointerSpeed(_))));
    }

    proptest! {
        #[test]
        fn values_in_range_are_held_as_given(
            delay in 100u32..=5000,
            rate in 0u32..=200,
            tap in proptest::option::of(any::<bool>()),
            natural in proptest::option::of(any::<bool>()),
            speed in proptest::option::of(-1.0f64..=1.0),
        ) {
            let settings = InputSettings::try_new(delay, rate, tap, natural, speed)
                .expect("every value is in range");
            prop_assert_eq!(settings.repeat_delay_ms(), delay);
            prop_assert_eq!(settings.repeat_rate(), rate);
            prop_assert_eq!(settings.tap_to_click(), tap);
            prop_assert_eq!(settings.natural_scroll(), natural);
            prop_assert_eq!(settings.pointer_speed(), speed);
        }

        #[test]
        fn a_delay_out_of_range_is_refused(
            delay in prop_oneof![0u32..100, 5001u32..=u32::MAX],
        ) {
            prop_assert_eq!(
                InputSettings::try_new(delay, 25, None, None, None),
                Err(SettingsError::RepeatDelay(delay))
            );
        }

        #[test]
        fn a_rate_out_of_range_is_refused(rate in 201u32..=u32::MAX) {
            prop_assert_eq!(
                InputSettings::try_new(600, rate, None, None, None),
                Err(SettingsError::RepeatRate(rate))
            );
        }

        #[test]
        fn a_speed_out_of_range_is_refused(
            speed in prop_oneof![-1.0e9f64..-1.0001, 1.0001f64..1.0e9],
        ) {
            prop_assert_eq!(
                InputSettings::try_new(600, 25, None, None, Some(speed)),
                Err(SettingsError::PointerSpeed(speed))
            );
        }
    }
}
