//! The fallback layout: every window in a column of equal width.

use super::Layout;
use crate::layout::MasterRatio;
use crate::shared::contracts::scene::Placement;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;

/// Equal columns side by side, in reading order.
///
/// This is the one layout the core keeps for itself, so the screen is never left without
/// an arrangement: it is what a workspace gets when the layout it names is not registered,
/// and what an engine with no layouts registered uses. The leftover pixels go one each to
/// the first columns, so the rectangles cover the area exactly.
#[derive(Debug, Clone, Copy, Default)]
pub struct Columns;

impl Layout for Columns {
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        _focus: Option<WinId>,
        _ratio: MasterRatio,
    ) -> Result<Vec<Placement>> {
        let count = windows.len();
        if count == 0 {
            return Ok(Vec::new());
        }
        let too_small = || Error::AreaTooSmall {
            windows: count,
            width: area.width(),
            height: area.height(),
        };
        let parts = u32::try_from(count).map_err(|_| too_small())?;
        if area.width() < parts {
            return Err(too_small());
        }
        let (base, extra) = (area.width() / parts, area.width() % parts);
        let mut at = i64::from(area.x());
        windows
            .iter()
            .zip(0..parts)
            .map(|(&id, part)| {
                let width = base + u32::from(part < extra);
                let x = i32::try_from(at).map_err(|_| Error::AreaOutOfRange(area))?;
                at += i64::from(width);
                Ok(Placement {
                    id,
                    rect: Rect::new(x, area.y(), width, area.height())?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn columns_cover_the_area_exactly_and_keep_the_order(
            x in -1_000_000_i32..1_000_000, y in -1_000_000_i32..1_000_000,
            w in 1_u32..3000, h in 1_u32..2000, count in 0_u64..16,
        ) {
            let area = Rect::new(x, y, w, h).expect("non-zero size");
            let windows: Vec<_> = (0..count).map(WinId::new).collect();
            match Columns.arrange(area, &windows, None, MasterRatio::default()) {
                Ok(placed) => {
                    prop_assert!(u64::from(w) >= count);
                    prop_assert_eq!(placed.iter().map(|p| p.id).collect::<Vec<_>>(), windows);
                    let covered: u64 = placed.iter().map(|p| u64::from(p.rect.width())).sum();
                    prop_assert!(count == 0 || covered == u64::from(w));
                    for pair in placed.windows(2) {
                        prop_assert_eq!(
                            i64::from(pair[0].rect.x()) + i64::from(pair[0].rect.width()),
                            i64::from(pair[1].rect.x())
                        );
                    }
                }
                Err(err) => {
                    prop_assert!(u64::from(w) < count);
                    let is_too_small = matches!(err, Error::AreaTooSmall { .. });
                    prop_assert!(is_too_small);
                }
            }
        }

        #[test]
        fn never_panics_near_the_coordinate_limit(
            back in 0_i32..64, w in 1_u32..2000, count in 0_u64..8,
        ) {
            let area = Rect::new(i32::MAX - back, 0, w, 10).expect("non-zero size");
            let windows: Vec<_> = (0..count).map(WinId::new).collect();
            let _ = Columns.arrange(area, &windows, None, MasterRatio::default());
        }
    }
}
