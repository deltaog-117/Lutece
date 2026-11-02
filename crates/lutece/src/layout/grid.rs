//! Grid: the windows fill rows of equal height, as square as the count allows.

use super::span::split;
use crate::shared::contracts::scene::Placement;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;

/// Places `windows` row by row so the rectangles exactly cover `area`.
///
/// There are as many columns as the smallest whole number whose square reaches the
/// window count. The last row may hold fewer windows; they widen to fill it.
pub(crate) fn arrange(area: Rect, windows: &[WinId]) -> Result<Vec<Placement>> {
    let count = windows.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    let too_small = || Error::AreaTooSmall {
        windows: count,
        width: area.width(),
        height: area.height(),
    };

    let columns = (1..=count)
        .find(|columns| columns * columns >= count)
        .unwrap_or(count);
    let rows = count.div_ceil(columns);
    let row_count = u32::try_from(rows).map_err(|_| too_small())?;
    let column_count = u32::try_from(columns).map_err(|_| too_small())?;
    if area.width() < column_count || area.height() < row_count {
        return Err(too_small());
    }

    let mut placed = Vec::with_capacity(count);
    let heights = split(area.y(), area.height(), row_count, area);
    for (row, (y, height)) in windows.chunks(columns).zip(heights?) {
        let in_row = u32::try_from(row.len()).map_err(|_| too_small())?;
        for (&id, (x, width)) in row.iter().zip(split(area.x(), area.width(), in_row, area)?) {
            placed.push(Placement {
                id,
                rect: Rect::new(x, y, width, height)?,
            });
        }
    }
    Ok(placed)
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
        fn cells_exactly_cover_the_area_without_overlap(
            x in -1_000_000_i32..1_000_000, y in -1_000_000_i32..1_000_000,
            w in 1_u32..2000, h in 1_u32..2000, count in 0_u64..30,
        ) {
            let area = Rect::new(x, y, w, h).expect("non-zero size");
            let windows = ids(count);
            let columns = (1..=count.max(1)).find(|c| c * c >= count).unwrap_or(1);
            let rows = count.div_ceil(columns);
            let fits = count == 0 || (u64::from(w) >= columns && u64::from(h) >= rows);

            match arrange(area, &windows) {
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
            back in 0_i32..64, w in 1_u32..2000, h in 1_u32..2000, count in 0_u64..12,
        ) {
            let area = Rect::new(i32::MAX - back, i32::MAX - back, w, h).expect("non-zero size");
            let _ = arrange(area, &ids(count));
        }
    }

    #[test]
    fn a_lone_window_fills_the_area() {
        let area = Rect::new(0, 0, 100, 50).expect("non-zero size");
        let placed = arrange(area, &ids(1)).expect("fits");
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].rect, area);
    }

    #[test]
    fn five_windows_make_two_rows_of_three_and_two() {
        let area = Rect::new(0, 0, 90, 60).expect("non-zero size");
        let placed = arrange(area, &ids(5)).expect("fits");
        let widths: Vec<_> = placed.iter().map(|p| p.rect.width()).collect();
        assert_eq!(widths, [30, 30, 30, 45, 45]);
        assert!(placed.iter().all(|p| p.rect.height() == 30));
    }

    #[test]
    fn too_little_height_for_the_rows_is_an_error() {
        let area = Rect::new(0, 0, 100, 1).expect("non-zero size");
        let err = arrange(area, &ids(4)).expect_err("2 rows in 1px");
        assert_eq!(
            err,
            Error::AreaTooSmall {
                windows: 4,
                width: 100,
                height: 1
            }
        );
    }
}
