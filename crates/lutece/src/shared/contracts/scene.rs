//! The screen as the core wants it: which windows are visible, where, in what
//! stacking order, and which one has focus.

use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;
use std::collections::HashSet;

/// One visible window and the area it fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Placement {
    /// The window.
    pub id: WinId,
    /// Where it goes.
    pub rect: Rect,
}

/// A snapshot of what should be on screen.
///
/// Placements are ordered bottom to top. A window that is managed but not listed
/// (for example, on another workspace) is hidden. Each window is listed once, and the
/// focused window, if any, is one of the listed windows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Scene {
    placements: Vec<Placement>,
    focus: Option<WinId>,
}

impl Scene {
    /// A scene with nothing on screen.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds a scene, bottom window first.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DuplicateWindow`] if a window is listed twice, or
    /// [`Error::FocusedWindowMissing`] if `focus` names a window that is not listed.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{Placement, Rect, Scene, WinId};
    ///
    /// let win = WinId::new(1);
    /// let rect = Rect::new(0, 0, 800, 600)?;
    /// let scene = Scene::try_new(vec![Placement { id: win, rect }], Some(win))?;
    /// assert_eq!(scene.focus(), Some(win));
    /// assert!(Scene::try_new(vec![], Some(win)).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn try_new(placements: Vec<Placement>, focus: Option<WinId>) -> Result<Self> {
        let mut seen = HashSet::with_capacity(placements.len());
        for placement in &placements {
            if !seen.insert(placement.id) {
                return Err(Error::DuplicateWindow(placement.id));
            }
        }
        if let Some(id) = focus.filter(|id| !seen.contains(id)) {
            return Err(Error::FocusedWindowMissing(id));
        }
        Ok(Self { placements, focus })
    }

    /// The visible windows, bottom to top.
    #[must_use]
    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }

    /// The focused window, if any.
    #[must_use]
    pub const fn focus(&self) -> Option<WinId> {
        self.focus
    }

    /// The placement of `id`, if it is visible.
    #[must_use]
    pub fn get(&self, id: WinId) -> Option<&Placement> {
        self.placements.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(id: u64) -> Placement {
        Placement {
            id: WinId::new(id),
            rect: Rect::new(0, 0, 10, 10).expect("non-zero size"),
        }
    }

    #[test]
    fn a_duplicate_window_is_rejected() {
        let err = Scene::try_new(vec![placement(1), placement(1)], None).expect_err("duplicate");
        assert_eq!(err, Error::DuplicateWindow(WinId::new(1)));
    }

    #[test]
    fn focus_on_a_hidden_window_is_rejected() {
        let err =
            Scene::try_new(vec![placement(1)], Some(WinId::new(2))).expect_err("focus missing");
        assert_eq!(err, Error::FocusedWindowMissing(WinId::new(2)));
    }

    #[test]
    fn stacking_order_is_kept() {
        let scene = Scene::try_new(vec![placement(3), placement(1)], None).expect("valid scene");
        let ids: Vec<_> = scene.placements().iter().map(|p| p.id.get()).collect();
        assert_eq!(ids, [3, 1]);
    }
}
