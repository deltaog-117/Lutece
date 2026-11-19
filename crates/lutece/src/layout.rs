//! Layouts: pure functions from a workspace's windows and an output rectangle to the
//! rectangles those windows should fill. No display and no state.
//!
//! The core knows how to *use* a layout and keeps one of its own, [`Columns`]. Every other
//! layout is an implementation of [`Layout`] registered on the engine in a [`LayoutSet`]
//! under a [`LayoutName`]; the usual ones are in the `lutece-layouts` crate, and a crate of
//! your own can add more without touching the core.

mod columns;
mod name;
mod ratio;
mod set;

use crate::shared::contracts::scene::Placement;
use crate::shared::error::Result;
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;
use std::fmt::Debug;

pub use columns::Columns;
pub use name::{LayoutName, MAX_LAYOUT_NAME_BYTES};
pub use ratio::MasterRatio;
pub use set::LayoutSet;

/// How windows are arranged inside an area.
///
/// An implementation is a pure function of its arguments: no clock, no randomness and no
/// state kept between calls, because a replay of the same events must give the same
/// scene. The trait has `Send + Sync` so an engine that holds layouts can move between
/// threads, as it does in the host.
///
/// # Examples
///
/// A layout that stacks every window in one row of equal height:
///
/// ```
/// use lutece::{Layout, MasterRatio, Placement, Rect, Result, WinId};
///
/// #[derive(Debug)]
/// struct Rows;
///
/// impl Layout for Rows {
///     fn arrange(
///         &self,
///         area: Rect,
///         windows: &[WinId],
///         _focus: Option<WinId>,
///         _ratio: MasterRatio,
///     ) -> Result<Vec<Placement>> {
///         let height = area.height() / u32::try_from(windows.len().max(1)).unwrap_or(1);
///         windows
///             .iter()
///             .enumerate()
///             .map(|(row, &id)| {
///                 let y = area.y() + i32::try_from(row).unwrap_or(0) * i32::try_from(height).unwrap_or(0);
///                 Ok(Placement { id, rect: Rect::new(area.x(), y, area.width(), height.max(1))? })
///             })
///             .collect()
///     }
/// }
///
/// let area = Rect::new(0, 0, 100, 100)?;
/// let placed = Rows.arrange(area, &[WinId::new(1), WinId::new(2)], None, MasterRatio::default())?;
/// assert_eq!(placed[1].rect.y(), 50);
/// # Ok::<(), lutece::Error>(())
/// ```
pub trait Layout: Debug + Send + Sync {
    /// Places `windows` inside `area`.
    ///
    /// `windows` is in reading order, master first, and does not repeat a window. The
    /// result lists the windows bottom to top, ready for
    /// [`Scene::try_new`](crate::Scene::try_new), and every rectangle must lie inside
    /// `area`. `focus` only matters to layouts where windows overlap. `ratio` is the share
    /// of the width the workspace gives its master window; a layout with no master column
    /// ignores it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AreaTooSmall`](crate::Error::AreaTooSmall) if `area` has fewer
    /// pixels than the layout needs, or [`Error::AreaOutOfRange`](crate::Error::AreaOutOfRange)
    /// if it reaches past the coordinate range.
    fn arrange(
        &self,
        area: Rect,
        windows: &[WinId],
        focus: Option<WinId>,
        ratio: MasterRatio,
    ) -> Result<Vec<Placement>>;

    /// Whether the layout has a master column whose width the master ratio sets. Only
    /// then do the grow and shrink actions change a workspace's ratio.
    fn uses_master_ratio(&self) -> bool {
        false
    }
}
