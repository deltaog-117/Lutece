//! Cutting a length into equal parts, shared by the layouts that build rows and columns.

use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;

/// Narrows a computed coordinate; it only fails when `area` reaches past `i32::MAX`.
pub(super) fn coordinate(value: i64, area: Rect) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::AreaOutOfRange(area))
}

/// Cuts `total` pixels starting at `start` into `parts` runs that touch end to end and
/// cover it exactly. The leftover pixels go one each to the first runs.
///
/// Each run is `(start, length)`. The caller guarantees `parts` is at least 1 and no more
/// than `total`, so no run is empty.
pub(super) fn split(start: i32, total: u32, parts: u32, area: Rect) -> Result<Vec<(i32, u32)>> {
    let (base, extra) = (total / parts, total % parts);
    let mut at = i64::from(start);
    (0..parts)
        .map(|part| {
            let length = base + u32::from(part < extra);
            let here = coordinate(at, area)?;
            at += i64::from(length);
            Ok((here, length))
        })
        .collect()
}
