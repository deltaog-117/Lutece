//! Which outputs each window is on, told to its client.
//!
//! A client that does not bind `wp_fractional_scale` learns how finely to draw from
//! `wl_output.scale`, but only for the outputs its surface has entered. Without an enter
//! it keeps drawing at 1 and is enlarged when shown. The core knows where each window sits
//! and the backend knows where each output is, so the enters are worked out from those two
//! rectangles; nothing here needs the renderer.

use crate::compositor::State;
use lutece::{Rect, WinId};
use smithay::desktop::PopupManager;
use smithay::output::Output as WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Transform;
use smithay::wayland::compositor::{
    TraversalAction, get_parent, send_surface_state, with_states, with_surface_tree_downward,
};

/// Whether two rectangles share at least one pixel. Touching edges share none.
pub(crate) fn overlaps(a: Rect, b: Rect) -> bool {
    // Widened so a rectangle at the edge of `i32` cannot overflow.
    let end = |start: i32, length: u32| i64::from(start) + i64::from(length);
    i64::from(a.x()) < end(b.x(), b.width())
        && i64::from(b.x()) < end(a.x(), a.width())
        && i64::from(a.y()) < end(b.y(), b.height())
        && i64::from(b.y()) < end(a.y(), a.height())
}

impl State {
    /// Enters the surfaces of window `id` on every output its rectangle overlaps and
    /// leaves the others, then tells them the integer scale to draw at. A window that is
    /// hidden is on no output.
    ///
    /// Entering twice or leaving twice is harmless, so this is called whenever anything
    /// it depends on may have changed.
    pub(crate) fn refresh_presence(&self, id: WinId) {
        let Some(entry) = self.windows.get(id) else {
            return;
        };
        let Some(root) = entry.shell.wl_surface() else {
            return;
        };
        let surfaces = window_surfaces(&root);
        for (area, output) in &self.wl_outputs {
            let on = entry.rect.is_some_and(|rect| overlaps(rect, *area));
            for surface in &surfaces {
                if on {
                    output.enter(surface);
                } else {
                    output.leave(surface);
                }
            }
        }
        let integer = self.scale.integer();
        for surface in &surfaces {
            with_states(surface, |states| {
                send_surface_state(surface, states, integer, Transform::Normal);
            });
        }
    }

    /// [`refresh_presence`](Self::refresh_presence) for every managed window, after the
    /// outputs or the scale changed.
    pub(crate) fn refresh_all_presence(&self) {
        for (id, _) in self.windows.entries() {
            self.refresh_presence(id);
        }
    }

    /// Takes every window off outputs that are about to disappear, so no client is left
    /// believing a surface is on a screen that is gone.
    pub(crate) fn leave_outputs(&self, gone: &[WlOutput]) {
        for (_, entry) in self.windows.entries() {
            let Some(root) = entry.shell.wl_surface() else {
                continue;
            };
            for surface in window_surfaces(&root) {
                for output in gone {
                    output.leave(&surface);
                }
            }
        }
    }

    /// Refreshes the window `surface` belongs to, for a surface that just became part of
    /// it (a subsurface or a popup) and has not been placed by a scene.
    pub(crate) fn refresh_presence_of(&self, surface: &WlSurface) {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        if let Some(popup) = self.popups.find_popup(&root)
            && let Ok(toplevel) = smithay::desktop::find_popup_root_surface(&popup)
        {
            root = toplevel;
        }
        if let Some(id) = self.windows.id_of(&root) {
            self.refresh_presence(id);
        }
    }
}

/// A window's surface, its subsurfaces, its popups and their subsurfaces.
fn window_surfaces(root: &WlSurface) -> Vec<WlSurface> {
    let mut roots = vec![root.clone()];
    roots.extend(
        PopupManager::popups_for_surface(root).map(|(popup, _)| popup.wl_surface().clone()),
    );
    let mut surfaces = Vec::new();
    for root in &roots {
        with_surface_tree_downward(
            root,
            (),
            |_, _, ()| TraversalAction::DoChildren(()),
            |surface, _, ()| surfaces.push(surface.clone()),
            |_, _, ()| true,
        );
    }
    surfaces
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect::new(x, y, width, height).expect("a non-empty rectangle")
    }

    #[test]
    fn rectangles_that_only_touch_do_not_overlap() {
        assert!(!overlaps(rect(0, 0, 100, 100), rect(100, 0, 100, 100)));
        assert!(!overlaps(rect(0, 0, 100, 100), rect(0, 100, 100, 100)));
    }

    #[test]
    fn one_shared_pixel_is_an_overlap() {
        assert!(overlaps(rect(0, 0, 100, 100), rect(99, 99, 100, 100)));
    }

    #[test]
    fn a_rectangle_at_the_edge_of_i32_does_not_overflow() {
        assert!(overlaps(
            rect(i32::MAX - 1, 0, u32::MAX, 10),
            rect(i32::MAX, 0, 10, 10)
        ));
        assert!(!overlaps(
            rect(i32::MIN, 0, 10, 10),
            rect(i32::MAX, 0, 10, 10)
        ));
    }

    proptest! {
        /// Overlap does not depend on which rectangle is asked about first.
        #[test]
        fn overlap_is_symmetric(
            ax in -2000i32..2000, ay in -2000i32..2000, aw in 1u32..2000, ah in 1u32..2000,
            bx in -2000i32..2000, by in -2000i32..2000, bw in 1u32..2000, bh in 1u32..2000,
        ) {
            let (a, b) = (rect(ax, ay, aw, ah), rect(bx, by, bw, bh));
            prop_assert_eq!(overlaps(a, b), overlaps(b, a));
        }

        /// A rectangle always overlaps itself, and is on any rectangle it sits inside.
        #[test]
        fn a_rectangle_overlaps_what_contains_it(
            x in -2000i32..2000, y in -2000i32..2000, w in 1u32..2000, h in 1u32..2000,
            margin in 0i32..500,
        ) {
            let inner = rect(x, y, w, h);
            let outer = rect(x - margin, y - margin, w + 2 * margin as u32, h + 2 * margin as u32);
            prop_assert!(overlaps(inner, inner));
            prop_assert!(overlaps(inner, outer));
        }

        /// Rectangles split by a gap on the x axis never overlap.
        #[test]
        fn a_gap_means_no_overlap(
            x in -2000i32..2000, w in 1u32..2000, gap in 0i32..500, w2 in 1u32..2000,
            y in -2000i32..2000, h in 1u32..2000,
        ) {
            let right = x + i32::try_from(w).expect("small") + gap;
            prop_assert!(!overlaps(rect(x, y, w, h), rect(right, y, w2, h)));
        }
    }
}
