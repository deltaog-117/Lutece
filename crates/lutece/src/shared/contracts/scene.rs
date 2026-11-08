//! The screen as the core wants it: which windows are visible, where, in what
//! stacking order, and which one has focus.

use crate::shared::contracts::style::Style;
use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::WinId;
use std::collections::{HashMap, HashSet};

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
///
/// Some of the listed windows may be fullscreen: the placement of such a window already
/// covers its whole output, and the scene also says so, so the backend can tell the
/// client (`_NET_WM_STATE_FULLSCREEN` on X11, the fullscreen state on Wayland).
///
/// A listed window may also carry a [`Style`]: its look and the animation that carries it
/// to its next place. A window with no style has the default one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Scene {
    placements: Vec<Placement>,
    focus: Option<WinId>,
    fullscreen: HashSet<WinId>,
    // Only styles that differ from the default are kept, so two scenes that look the same
    // compare equal however they were built.
    styles: HashMap<WinId, Style>,
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
        Ok(Self {
            placements,
            focus,
            fullscreen: HashSet::new(),
            styles: HashMap::new(),
        })
    }

    /// Marks these listed windows as fullscreen.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FullscreenWindowMissing`] if a window is not listed.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{Placement, Rect, Scene, WinId};
    ///
    /// let win = WinId::new(1);
    /// let rect = Rect::new(0, 0, 800, 600)?;
    /// let scene = Scene::try_new(vec![Placement { id: win, rect }], Some(win))?
    ///     .with_fullscreen([win])?;
    /// assert!(scene.is_fullscreen(win));
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn with_fullscreen(mut self, windows: impl IntoIterator<Item = WinId>) -> Result<Self> {
        for id in windows {
            if self.get(id).is_none() {
                return Err(Error::FullscreenWindowMissing(id));
            }
            self.fullscreen.insert(id);
        }
        Ok(self)
    }

    /// Gives these listed windows a style. A style equal to the default is the same as none.
    ///
    /// # Errors
    ///
    /// Returns [`Error::StyleWindowMissing`] if a window is not listed.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::{Placement, Rect, Scene, Style, WinId};
    ///
    /// let win = WinId::new(1);
    /// let rect = Rect::new(0, 0, 800, 600)?;
    /// let style = Style::default().with_radius(12);
    /// let scene = Scene::try_new(vec![Placement { id: win, rect }], Some(win))?
    ///     .with_style([(win, style)])?;
    /// assert_eq!(scene.style_of(win), style);
    /// assert!(scene.clone().with_style([(WinId::new(2), style)]).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn with_style(mut self, styles: impl IntoIterator<Item = (WinId, Style)>) -> Result<Self> {
        for (id, style) in styles {
            if self.get(id).is_none() {
                return Err(Error::StyleWindowMissing(id));
            }
            if style == Style::default() {
                self.styles.remove(&id);
            } else {
                self.styles.insert(id, style);
            }
        }
        Ok(self)
    }

    /// The style of `id`: the default if it has none or is not listed.
    #[must_use]
    pub fn style_of(&self, id: WinId) -> Style {
        self.styles.get(&id).copied().unwrap_or_default()
    }

    /// Whether `id` is listed and fullscreen.
    #[must_use]
    pub fn is_fullscreen(&self, id: WinId) -> bool {
        self.fullscreen.contains(&id)
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
    fn a_fullscreen_window_must_be_listed() {
        let scene = Scene::try_new(vec![placement(1)], None).expect("valid scene");
        let err = scene
            .clone()
            .with_fullscreen([WinId::new(2)])
            .expect_err("not listed");
        assert_eq!(err, Error::FullscreenWindowMissing(WinId::new(2)));

        let scene = scene.with_fullscreen([WinId::new(1)]).expect("listed");
        assert!(scene.is_fullscreen(WinId::new(1)));
        assert!(!scene.is_fullscreen(WinId::new(2)));
    }

    #[test]
    fn a_styled_window_must_be_listed() {
        let scene = Scene::try_new(vec![placement(1)], None).expect("valid scene");
        let style = Style::default().with_radius(4);
        let err = scene
            .clone()
            .with_style([(WinId::new(2), style)])
            .expect_err("not listed");
        assert_eq!(err, Error::StyleWindowMissing(WinId::new(2)));

        let scene = scene.with_style([(WinId::new(1), style)]).expect("listed");
        assert_eq!(scene.style_of(WinId::new(1)), style);
        assert_eq!(scene.style_of(WinId::new(2)), Style::default());
    }

    #[test]
    fn a_default_style_equals_no_style() {
        let plain = Scene::try_new(vec![placement(1)], None).expect("valid scene");
        let explicit = plain
            .clone()
            .with_style([(WinId::new(1), Style::default())])
            .expect("listed");
        assert_eq!(plain, explicit);
    }

    #[test]
    fn stacking_order_is_kept() {
        let scene = Scene::try_new(vec![placement(3), placement(1)], None).expect("valid scene");
        let ids: Vec<_> = scene.placements().iter().map(|p| p.id.get()).collect();
        assert_eq!(ids, [3, 1]);
    }
}
