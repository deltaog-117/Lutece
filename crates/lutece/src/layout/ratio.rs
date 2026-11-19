//! The share of the width a master window takes.

use crate::shared::error::{Error, Result};

/// Share of the area width given to the master window, in thousandths.
///
/// Whole thousandths instead of a float keep layouts comparable with `Eq` and make the
/// pixel arithmetic exact. Never 0 or 1000, so neither column can vanish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MasterRatio(u16);

impl MasterRatio {
    /// Builds a ratio from thousandths of the width.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidRatio`] unless `permille` is in `1..=999`.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::layout::MasterRatio;
    ///
    /// assert_eq!(MasterRatio::try_new(550)?.permille(), 550);
    /// assert!(MasterRatio::try_new(0).is_err());
    /// assert!(MasterRatio::try_new(1000).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub const fn try_new(permille: u16) -> Result<Self> {
        if permille == 0 || permille >= 1000 {
            return Err(Error::InvalidRatio(permille));
        }
        Ok(Self(permille))
    }

    /// The ratio in thousandths.
    #[must_use]
    pub const fn permille(self) -> u16 {
        self.0
    }

    /// Moves the ratio by `delta` thousandths, stopping at 1 and 999 so neither column
    /// can vanish.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::layout::MasterRatio;
    ///
    /// let ratio = MasterRatio::try_new(550)?;
    /// assert_eq!(ratio.step(50).permille(), 600);
    /// assert_eq!(ratio.step(-9000).permille(), 1);
    /// assert_eq!(ratio.step(9000).permille(), 999);
    /// # Ok::<(), lutece::Error>(())
    /// ```
    #[must_use]
    pub fn step(self, delta: i16) -> Self {
        let moved = (i32::from(self.0) + i32::from(delta)).clamp(1, 999);
        // The clamp keeps it in `1..=999`, so the conversion cannot fail.
        Self(u16::try_from(moved).unwrap_or(self.0))
    }

    /// The master width for an area `width` wide; `width` must be at least 2.
    /// Clamped so the master and the rest each keep at least one pixel.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::MasterRatio;
    ///
    /// assert_eq!(MasterRatio::try_new(500)?.master_width(100), 50);
    /// assert_eq!(MasterRatio::try_new(1)?.master_width(2), 1);
    /// # Ok::<(), lutece::Error>(())
    /// ```
    #[must_use]
    pub fn master_width(self, width: u32) -> u32 {
        let scaled = u64::from(width) * u64::from(self.0) / 1000;
        let clamped = scaled.clamp(1, u64::from(width - 1));
        // The clamp keeps it below `width`, so the conversion cannot fail.
        u32::try_from(clamped).unwrap_or(width - 1)
    }
}

impl Default for MasterRatio {
    fn default() -> Self {
        Self(550)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn ratio_accepts_exactly_1_to_999(permille in any::<u16>()) {
            prop_assert_eq!(MasterRatio::try_new(permille).is_ok(), (1..=999).contains(&permille));
        }

        #[test]
        fn stepping_stays_in_range_and_moves_by_delta_when_it_can(
            permille in 1_u16..1000, delta in any::<i16>(),
        ) {
            let ratio = MasterRatio::try_new(permille).expect("in range");
            let stepped = ratio.step(delta).permille();
            prop_assert!((1..=999).contains(&stepped));
            let exact = i32::from(permille) + i32::from(delta);
            if (1..=999).contains(&exact) {
                prop_assert_eq!(i32::from(stepped), exact);
            }
        }

        #[test]
        fn master_width_leaves_a_pixel_on_each_side(
            permille in 1_u16..1000, width in 2_u32..100_000,
        ) {
            let ratio = MasterRatio::try_new(permille).expect("in range");
            let master = ratio.master_width(width);
            prop_assert!(master >= 1 && master < width);
        }
    }
}
