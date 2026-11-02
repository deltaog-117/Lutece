//! Centered master: the first window takes a column in the middle and the rest share
//! the two columns beside it.

use super::span::split;
use super::tile::{self, MasterRatio};
use crate::shared::contracts::scene::Placement;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;

/// Places `windows` (master first) so the rectangles exactly cover `area`.
///
/// The second window goes to the right column, the third to the left, and so on
/// alternating. With one or two windows there is no pair to flank the master, so the
/// layout is the same as tile.
pub(crate) fn arrange(ratio: MasterRatio, area: Rect, windows: &[WinId]) -> Result<Vec<Placement>> {
    let Some((&master, rest)) = windows.split_first() else {
        return Ok(Vec::new());
    };
    if rest.len() < 2 {
        return tile::arrange(ratio, area, windows);
    }

    let too_small = || Error::AreaTooSmall {
        windows: windows.len(),
        width: area.width(),
        height: area.height(),
    };
    let right_rows = u32::try_from(rest.len().div_ceil(2)).map_err(|_| too_small())?;
    let left_rows = u32::try_from(rest.len() / 2).map_err(|_| too_small())?;
    if area.width() < 3 || area.height() < right_rows {
        return Err(too_small());
    }

    // The master keeps one pixel off each side, so both side columns stay visible.
    let master_width = ratio.master_width(area.width()).min(area.width() - 2);
    let side = area.width() - master_width;
    let (left_width, right_width) = (side / 2, side - side / 2);
    let master_x = shifted(area, left_width)?;
    let right_x = shifted(area, left_width + master_width)?;

    let right = split(area.y(), area.height(), right_rows, area)?;
    let left = split(area.y(), area.height(), left_rows, area)?;

    let mut placed = Vec::with_capacity(windows.len());
    placed.push(Placement {
        id: master,
        rect: Rect::new(master_x, area.y(), master_width, area.height())?,
    });
    for (index, &id) in rest.iter().enumerate() {
        let (y, height) = if index % 2 == 0 {
            right[index / 2]
        } else {
            left[index / 2]
        };
        let (x, width) = if index % 2 == 0 {
            (right_x, right_width)
        } else {
            (area.x(), left_width)
        };
        placed.push(Placement {
            id,
            rect: Rect::new(x, y, width, height)?,
        });
    }
    Ok(placed)
}

/// `area`'s left edge moved right by `by` pixels.
fn shifted(area: Rect, by: u32) -> Result<i32> {
    super::span::coordinate(i64::from(area.x()) + i64::from(by), area)
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
        fn columns_exactly_cover_the_area_without_overlap(
            x in -1_000_000_i32..1_000_000, y in -1_000_000_i32..1_000_000,
            w in 1_u32..2000, h in 1_u32..2000,
            count in 0_u64..14, permille in 1_u16..1000,
        ) {
            let area = Rect::new(x, y, w, h).expect("non-zero size");
            let ratio = MasterRatio::try_new(permille).expect("in range");
            let windows = ids(count);
            let fits = match count {
                0 | 1 => true,
                2 => w >= 2 && h >= 1,
                _ => w >= 3 && u64::from(h) >= count / 2,
            };

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
    fn two_windows_are_laid_out_like_tile() {
        let area = Rect::new(0, 0, 100, 50).expect("non-zero size");
        let ratio = MasterRatio::default();
        assert_eq!(
            arrange(ratio, area, &ids(2)).expect("fits"),
            tile::arrange(ratio, area, &ids(2)).expect("fits")
        );
    }

    #[test]
    fn the_master_sits_between_the_side_columns() {
        let area = Rect::new(0, 0, 1000, 100).expect("non-zero size");
        let ratio = MasterRatio::try_new(500).expect("in range");
        let placed = arrange(ratio, area, &ids(4)).expect("fits");
        let [master, right_top, left, right_bottom] = placed.as_slice() else {
            panic!("four windows give four placements");
        };
        assert_eq!((master.rect.x(), master.rect.width()), (250, 500));
        assert_eq!((left.rect.x(), left.rect.width()), (0, 250));
        assert_eq!((right_top.rect.x(), right_top.rect.width()), (750, 250));
        assert_eq!(right_top.rect.x(), right_bottom.rect.x());
        assert_eq!(left.rect.height(), 100);
        assert_eq!(right_top.rect.height() + right_bottom.rect.height(), 100);
    }

    #[test]
    fn extreme_ratios_keep_every_column_visible() {
        let area = Rect::new(0, 0, 3, 10).expect("non-zero size");
        for permille in [1, 999] {
            let ratio = MasterRatio::try_new(permille).expect("in range");
            let placed = arrange(ratio, area, &ids(3)).expect("fits");
            assert!(placed.iter().all(|p| p.rect.width() == 1));
        }
    }

    #[test]
    fn a_width_under_three_cannot_hold_three_columns() {
        let area = Rect::new(0, 0, 2, 10).expect("non-zero size");
        let err = arrange(MasterRatio::default(), area, &ids(3)).expect_err("3 columns in 2px");
        assert_eq!(
            err,
            Error::AreaTooSmall {
                windows: 3,
                width: 2,
                height: 10
            }
        );
    }
}
