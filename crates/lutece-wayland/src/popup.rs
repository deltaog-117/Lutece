//! Keeping popups on the output: where a menu or tooltip may go, and the position the
//! client's positioner rules give it within that.
//!
//! Positions are relative to the parent's window geometry, which the backend places at the
//! parent's rectangle, so the output is expressed in the same terms.

use crate::compositor::State;
use lutece::{Rect, WinId};
use smithay::desktop::{
    PopupKind, PopupManager, find_popup_root_surface, get_popup_toplevel_coords,
};
use smithay::utils::{Logical, Point, Rectangle};
use smithay::wayland::shell::xdg::{PopupSurface, PositionerState};

/// The output a popup of the window at `window` must stay on, relative to the window's
/// origin: the output holding that origin, or the first one if none does. `None` when
/// there is no output.
fn target(window: Rect, screens: &[Rect]) -> Option<Rectangle<i32, Logical>> {
    let screen = screens
        .iter()
        .find(|screen| screen.contains(window.origin()))
        .or_else(|| screens.first())?;
    let length = |pixels: u32| i32::try_from(pixels).unwrap_or(i32::MAX);
    Some(Rectangle::new(
        Point::from((
            screen.x().saturating_sub(window.x()),
            screen.y().saturating_sub(window.y()),
        )),
        (length(screen.width()), length(screen.height())).into(),
    ))
}

impl State {
    /// Where `popup` goes, relative to its parent: what its positioner asks, moved by the
    /// positioner's own adjustments (slide, flip, resize) to stay on the output.
    ///
    /// A popup whose window is not placed keeps the position it asked for. When the window
    /// moves later, [`dismiss_misplaced_popups`](Self::dismiss_misplaced_popups) closes the ones the move put in the wrong place.
    pub(crate) fn popup_geometry(
        &self,
        popup: &PopupSurface,
        positioner: &PositionerState,
    ) -> Rectangle<i32, Logical> {
        match self.popup_target(popup) {
            Some(target) => (*positioner).get_unconstrained_geometry(target),
            None => positioner.get_geometry(),
        }
    }

    /// Dismisses the open popups of window `id` that no longer sit where their rules put
    /// them, after the window moved (a floating drag, a new layout).
    ///
    /// A popup follows its window by itself, but its position was worked out against
    /// the output at the old place: a menu slid back from the edge now hangs in the
    /// air, and one that fitted may now leave the output. The protocol lets a compositor
    /// configure a popup only when it opens or when its client asks for a new
    /// position, so the way to keep a popup off the wrong place is to close it, as a
    /// menu closes when its window is dragged. A popup that has not had its first
    /// configure yet is simply given the new place.
    pub(crate) fn dismiss_misplaced_popups(&mut self, id: WinId) {
        let Some(root) = self
            .windows
            .get(id)
            .and_then(|entry| entry.shell.wl_surface())
        else {
            return;
        };
        let popups: Vec<_> = PopupManager::popups_for_surface(&root)
            .filter_map(|(kind, _)| match kind {
                PopupKind::Xdg(popup) => Some(popup),
                PopupKind::InputMethod(_) => None,
            })
            .collect();
        for popup in popups {
            let positioner = popup.with_pending_state(|state| state.positioner);
            let geometry = self.popup_geometry(&popup, &positioner);
            let moved = popup.with_pending_state(|state| {
                std::mem::replace(&mut state.geometry, geometry) != geometry
            });
            if moved && popup.is_initial_configure_sent() {
                popup.send_popup_done();
                self.pointer
                    .grabbed_popups
                    .retain(|grabbed| grabbed != &popup);
            }
        }
    }

    /// The output, relative to the popup's parent.
    fn popup_target(&self, popup: &PopupSurface) -> Option<Rectangle<i32, Logical>> {
        let parent = popup.get_parent_surface()?;
        // A menu opened from a menu is positioned from that menu, so the output is moved
        // by where the menu sits in its window.
        let (root, offset) = match self.popups.find_popup(&parent) {
            Some(kind) => (
                find_popup_root_surface(&kind).ok()?,
                get_popup_toplevel_coords(&kind),
            ),
            None => (parent, Point::default()),
        };
        let window = self.windows.get(self.windows.id_of(&root)?)?.rect?;
        let mut target = target(window, &self.screens)?;
        target.loc -= offset;
        Some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect::new(x, y, width, height).expect("a non-empty rectangle")
    }

    #[test]
    fn a_window_on_the_only_output_sees_it_whole() {
        let target = target(rect(0, 0, 400, 300), &[rect(0, 0, 1000, 600)]);
        assert_eq!(
            target,
            Some(Rectangle::new((0, 0).into(), (1000, 600).into()))
        );
    }

    #[test]
    fn the_output_is_measured_from_the_window() {
        let screens = [rect(0, 0, 500, 600), rect(500, 0, 500, 600)];
        let target = target(rect(600, 40, 100, 100), &screens);
        assert_eq!(
            target,
            Some(Rectangle::new((-100, -40).into(), (500, 600).into()))
        );
    }

    #[test]
    fn a_window_off_every_output_uses_the_first() {
        let screens = [rect(0, 0, 500, 600), rect(500, 0, 500, 600)];
        let target = target(rect(5000, 0, 100, 100), &screens);
        assert_eq!(target.map(|t| t.size), Some((500, 600).into()));
    }

    #[test]
    fn no_output_means_no_target() {
        assert_eq!(target(rect(0, 0, 10, 10), &[]), None);
    }

    proptest! {
        /// Moving the target back by the window's origin gives one of the outputs, and
        /// the window's own origin is inside it when some output holds that origin.
        #[test]
        fn the_target_is_one_of_the_outputs(
            window in (-3000i32..3000, -3000i32..3000),
            screens in proptest::collection::vec(
                (-3000i32..3000, -3000i32..3000, 1u32..3000, 1u32..3000), 1..5),
        ) {
            let window = rect(window.0, window.1, 10, 10);
            let screens: Vec<Rect> = screens.iter().map(|s| rect(s.0, s.1, s.2, s.3)).collect();
            let found = target(window, &screens).expect("there are outputs");
            let back = (found.loc.x + window.x(), found.loc.y + window.y());
            let is_an_output = screens.iter().any(|s| {
                (s.x(), s.y()) == back
                    && (i32::try_from(s.width()), i32::try_from(s.height()))
                        == (Ok(found.size.w), Ok(found.size.h))
            });
            prop_assert!(is_an_output);
            if screens.iter().any(|s| s.contains(window.origin())) {
                prop_assert!(found.contains(Point::from((0, 0))));
            }
        }
    }
}
