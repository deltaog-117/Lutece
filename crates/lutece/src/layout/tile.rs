//! Tile: the first window takes a column on the left and the rest share the right.

use crate::shared::contracts::scene::Placement;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;

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

    /// The master width for an area `width` wide; `width` must be at least 2.
    /// Clamped so the master and the rest each keep at least one pixel.
    fn master_width(self, width: u32) -> u32 {
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

/// Places `windows` (master first) so the rectangles exactly cover `area`.
///
/// The leftover pixels from dividing the column height go one each to the first rows.
pub(crate) fn arrange(ratio: MasterRatio, area: Rect, windows: &[WinId]) -> Result<Vec<Placement>> {
    let Some((&master, rest)) = windows.split_first() else {
        return Ok(Vec::new());
    };
    if rest.is_empty() {
        return Ok(vec![Placement {
            id: master,
            rect: area,
        }]);
    }

    let too_small = || Error::AreaTooSmall {
        windows: windows.len(),
        width: area.width(),
        height: area.height(),
    };
    let rows = u32::try_from(rest.len()).map_err(|_| too_small())?;
    if area.width() < 2 || area.height() < rows {
        return Err(too_small());
    }

    let master_width = ratio.master_width(area.width());
    let column_width = area.width() - master_width;
    let column_x = coordinate(i64::from(area.x()) + i64::from(master_width), area)?;
    let (base, extra) = (area.height() / rows, area.height() % rows);

    let master_rect = Rect::new(area.x(), area.y(), master_width, area.height())?;
    let column = rest
        .iter()
        .zip(0..rows)
        .scan(i64::from(area.y()), |top, (&id, row)| {
            let height = base + u32::from(row < extra);
            let y = *top;
            *top += i64::from(height);
            Some(coordinate(y, area).and_then(|y| {
                Ok(Placement {
                    id,
                    rect: Rect::new(column_x, y, column_width, height)?,
                })
            }))
        });

    let master = Placement {
        id: master,
        rect: master_rect,
    };
    std::iter::once(Ok(master)).chain(column).collect()
}

/// Narrows a computed coordinate; it only fails when `area` reaches past `i32::MAX`.
fn coordinate(value: i64, area: Rect) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::AreaOutOfRange(area))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ids(count: u64) -> Vec<WinId> {
        (0..count).map(WinId::new).collect()
    }

    fn overlap(a: Rect, b: Rect) -> bool {
        let edge = |origin: i32, len: u32| i64::from(origin) + i64::from(len);
        i64::from(a.x()) < edge(b.x(), b.width())
            && i64::from(b.x()) < edge(a.x(), a.width())
            && i64::from(a.y()) < edge(b.y(), b.height())
            && i64::from(b.y()) < edge(a.y(), a.height())
    }

    fn inside(inner: Rect, outer: Rect) -> bool {
        let edge = |origin: i32, len: u32| i64::from(origin) + i64::from(len);
        inner.x() >= outer.x()
            && inner.y() >= outer.y()
            && edge(inner.x(), inner.width()) <= edge(outer.x(), outer.width())
            && edge(inner.y(), inner.height()) <= edge(outer.y(), outer.height())
    }

    proptest! {
        #[test]
        fn ratio_accepts_exactly_1_to_999(permille in any::<u16>()) {
            prop_assert_eq!(MasterRatio::try_new(permille).is_ok(), (1..=999).contains(&permille));
        }

        #[test]
        fn tiles_exactly_cover_the_area_without_overlap(
            x in -1_000_000_i32..1_000_000, y in -1_000_000_i32..1_000_000,
            w in 1_u32..2000, h in 1_u32..2000,
            count in 0_u64..12, permille in 1_u16..1000,
        ) {
            let area = Rect::new(x, y, w, h).expect("non-zero size");
            let ratio = MasterRatio::try_new(permille).expect("in range");
            let windows = ids(count);
            let fits = count <= 1 || (w >= 2 && u64::from(h) >= count - 1);

            match arrange(ratio, area, &windows) {
                Ok(placed) => {
                    prop_assert!(fits);
                    prop_assert_eq!(placed.iter().map(|p| p.id).collect::<Vec<_>>(), windows);
                    prop_assert!(placed.iter().all(|p| inside(p.rect, area)));
                    for (i, a) in placed.iter().enumerate() {
                        for b in &placed[i + 1..] {
                            prop_assert!(!overlap(a.rect, b.rect));
                        }
                    }
                    if count > 0 {
                        let covered: u64 = placed
                            .iter()
                            .map(|p| u64::from(p.rect.width()) * u64::from(p.rect.height()))
                            .sum();
                        prop_assert_eq!(covered, u64::from(w) * u64::from(h));
                    }
                }
                Err(err) => {
                    prop_assert!(!fits);
                    let is_too_small = matches!(err, Error::AreaTooSmall { .. });
                    prop_assert!(is_too_small);
                }
            }
        }

        #[test]
        fn never_panics_near_the_coordinate_limit(
            back in 0_i32..64, w in 1_u32..2000, h in 1_u32..2000, count in 0_u64..8,
        ) {
            let area = Rect::new(i32::MAX - back, i32::MAX - back, w, h).expect("non-zero size");
            let _ = arrange(MasterRatio::default(), area, &ids(count));
        }
    }

    #[test]
    fn a_lone_window_fills_the_area() {
        let area = Rect::new(0, 0, 100, 50).expect("non-zero size");
        let placed = arrange(MasterRatio::default(), area, &ids(1)).expect("fits");
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].rect, area);
    }

    #[test]
    fn leftover_rows_go_to_the_first_windows() {
        let area = Rect::new(0, 0, 100, 10).expect("non-zero size");
        let placed = arrange(MasterRatio::default(), area, &ids(4)).expect("fits");
        let heights: Vec<_> = placed[1..].iter().map(|p| p.rect.height()).collect();
        assert_eq!(heights, [4, 3, 3]);
    }

    #[test]
    fn extreme_ratios_keep_both_columns_visible() {
        let area = Rect::new(0, 0, 2, 10).expect("non-zero size");
        for permille in [1, 999] {
            let ratio = MasterRatio::try_new(permille).expect("in range");
            let placed = arrange(ratio, area, &ids(2)).expect("fits");
            assert_eq!((placed[0].rect.width(), placed[1].rect.width()), (1, 1));
        }
    }

    #[test]
    fn too_many_windows_for_the_height_is_an_error() {
        let area = Rect::new(0, 0, 100, 2).expect("non-zero size");
        let err = arrange(MasterRatio::default(), area, &ids(4)).expect_err("3 rows in 2px");
        assert_eq!(
            err,
            Error::AreaTooSmall {
                windows: 4,
                width: 100,
                height: 2
            }
        );
    }
}
