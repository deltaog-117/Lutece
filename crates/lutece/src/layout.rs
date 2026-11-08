//! Layouts: pure functions from a workspace's windows and an output rectangle to the
//! rectangles those windows should fill. No display and no state.

mod centered;
mod grid;
mod monocle;
mod span;
mod tile;

use crate::shared::contracts::scene::Placement;
use crate::shared::error::Result;
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;

pub use tile::MasterRatio;

/// How a workspace arranges its windows.
///
/// A closed enum, so a workspace can hold, compare and switch layouts as plain values and
/// the compiler flags every `match` that misses a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Layout {
    /// The first window on the left, the rest stacked on the right.
    Tile(MasterRatio),
    /// Every window full size, the focused one on top.
    Monocle,
    /// Rows of equal height, as square as the window count allows.
    Grid,
    /// The first window in a middle column, the rest alternating to its right and left.
    CenteredMaster(MasterRatio),
    /// A plugin of the window manager decides: the engine asks for the rectangles with
    /// [`Effect::LayoutRequested`](crate::Effect::LayoutRequested) and uses the answer of
    /// [`Event::LayoutProvided`](crate::Event::LayoutProvided). Until an answer fits the
    /// windows, the workspace is laid out like [`Layout::default`].
    External,
}

impl Default for Layout {
    fn default() -> Self {
        Self::Tile(MasterRatio::default())
    }
}

impl Layout {
    /// Places `windows` inside `area`.
    ///
    /// `windows` is in reading order, master first, and must not repeat a window. The
    /// result lists the windows bottom to top, ready for [`Scene::try_new`](crate::Scene::try_new).
    /// `focus` only matters to layouts where windows overlap.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AreaTooSmall`](crate::Error::AreaTooSmall) if `area` has fewer
    /// pixels than the layout needs, or [`Error::AreaOutOfRange`](crate::Error::AreaOutOfRange)
    /// if it reaches past the coordinate range.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{Layout, Rect, WinId};
    ///
    /// let area = Rect::new(0, 0, 1000, 600)?;
    /// let windows = [WinId::new(1), WinId::new(2)];
    /// let placed = Layout::default().arrange(area, &windows, Some(WinId::new(1)))?;
    /// assert_eq!(placed[0].rect.width() + placed[1].rect.width(), 1000);
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn arrange(
        self,
        area: Rect,
        windows: &[WinId],
        focus: Option<WinId>,
    ) -> Result<Vec<Placement>> {
        match self {
            Self::Tile(ratio) => tile::arrange(ratio, area, windows),
            Self::Monocle => Ok(monocle::arrange(area, windows, focus)),
            Self::Grid => grid::arrange(area, windows),
            Self::CenteredMaster(ratio) => centered::arrange(ratio, area, windows),
            // The engine places an external layout from its answer; asked directly, the
            // fallback is the default layout.
            Self::External => Self::default().arrange(area, windows, focus),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_variant_dispatches_to_its_own_arrangement() {
        let area = Rect::new(0, 0, 100, 100).expect("non-zero size");
        let windows = [WinId::new(1), WinId::new(2)];

        let tiled = Layout::default()
            .arrange(area, &windows, None)
            .expect("fits");
        assert_ne!(tiled[0].rect, tiled[1].rect);

        let stacked = Layout::Monocle
            .arrange(area, &windows, None)
            .expect("never fails");
        assert!(stacked.iter().all(|p| p.rect == area));

        let three = [WinId::new(1), WinId::new(2), WinId::new(3)];
        let centered = Layout::CenteredMaster(MasterRatio::default())
            .arrange(area, &three, None)
            .expect("fits");
        assert!(
            centered[0].rect.x() > 0,
            "the master is not on the left edge"
        );

        let gridded = Layout::Grid.arrange(area, &three, None).expect("fits");
        assert_eq!(gridded[0].rect.y(), gridded[1].rect.y());
        assert!(gridded[2].rect.y() > gridded[0].rect.y());
    }
}
