//! Which outputs each window is on, told to its client.
//!
//! A client that does not bind `wp_fractional_scale` learns how finely to draw from
//! `wl_output.scale`, but only for the outputs its surface has entered. Without an enter
//! it keeps drawing at 1 and is enlarged when shown. The core knows where each window sits
//! and the backend knows where each output is, so the enters are worked out from those two
//! rectangles; nothing here needs the renderer.

use crate::compositor::State;
use crate::scale::Scale;
use lutece::{Rect, WinId};
use smithay::desktop::PopupManager;
use smithay::output::Output as WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Transform;
use smithay::wayland::compositor::{
    TraversalAction, get_parent, send_surface_state, with_states, with_surface_tree_downward,
};
use smithay::wayland::fractional_scale::with_fractional_scale;

/// Whether two rectangles share at least one pixel. Touching edges share none.
pub(crate) fn overlaps(a: Rect, b: Rect) -> bool {
    // Widened so a rectangle at the edge of `i32` cannot overflow.
    let end = |start: i32, length: u32| i64::from(start) + i64::from(length);
    i64::from(a.x()) < end(b.x(), b.width())
        && i64::from(b.x()) < end(a.x(), a.width())
        && i64::from(a.y()) < end(b.y(), b.height())
        && i64::from(b.y()) < end(a.y(), a.height())
}

/// How many pixels `a` and `b` share; none when they only touch.
fn shared_area(a: Rect, b: Rect) -> u128 {
    let span = |start_a: i32, len_a: u32, start_b: i32, len_b: u32| {
        let low = i64::from(start_a).max(i64::from(start_b));
        let high =
            (i64::from(start_a) + i64::from(len_a)).min(i64::from(start_b) + i64::from(len_b));
        u128::try_from(high - low).unwrap_or(0)
    };
    span(a.x(), a.width(), b.x(), b.width()) * span(a.y(), a.height(), b.y(), b.height())
}

/// The scale a window at `window` is drawn at: that of the screen it covers most, the
/// earlier one when two share it equally. A window on no screen has the `fallback`, which is
/// the scale outputs have when nothing says otherwise.
///
/// A window that straddles two screens is drawn on each at that screen's own scale, but its
/// client can only be told one, and the one it covers most is the one it is looked at on.
pub(crate) fn scale_for(screens: &[(Rect, Scale)], fallback: Scale, window: Rect) -> Scale {
    let mut best: Option<(u128, Scale)> = None;
    for &(area, scale) in screens {
        let shared = shared_area(window, area);
        if shared > 0 && best.is_none_or(|(most, _)| shared > most) {
            best = Some((shared, scale));
        }
    }
    best.map_or(fallback, |(_, scale)| scale)
}

impl State {
    /// The scale window `id` is drawn at; see [`scale_for`]. A hidden window has the
    /// default.
    pub(crate) fn scale_of_window(&self, rect: Option<Rect>) -> Scale {
        rect.map_or(self.scale, |rect| {
            scale_for(&self.screen_scales, self.scale, rect)
        })
    }

    /// Enters the surfaces of window `id` on every output its rectangle overlaps and
    /// leaves the others, then tells them the scale to draw at: the one of the output the
    /// window is mostly on. A window that is hidden is on no output.
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
        let scale = self.scale_of_window(entry.rect);
        let integer = scale.integer();
        for surface in &surfaces {
            with_states(surface, |states| {
                with_fractional_scale(states, |fractional| {
                    fractional.set_preferred_scale(scale.factor());
                });
                send_surface_state(surface, states, integer, Transform::Normal);
            });
        }
    }

    /// Tells the surface tree under `root`, and its popups, the scale of the output it is on.
    /// Layer surfaces are put on their output by the layer map; this is the part of
    /// presence that is left for them.
    pub(crate) fn tell_scale(&self, root: &WlSurface, scale: Scale) {
        let integer = scale.integer();
        for surface in window_surfaces(root) {
            with_states(&surface, |states| {
                with_fractional_scale(states, |fractional| {
                    fractional.set_preferred_scale(scale.factor());
                });
                send_surface_state(&surface, states, integer, Transform::Normal);
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

    fn scale(factor: f64) -> Scale {
        Scale::from_factor(factor).expect("a usable scale")
    }

    #[test]
    fn a_window_on_no_screen_has_the_fallback_scale() {
        let screens = [(rect(0, 0, 1000, 600), scale(2.0))];
        assert_eq!(
            scale_for(&screens, scale(1.0), rect(5000, 0, 100, 100)),
            scale(1.0)
        );
        assert_eq!(scale_for(&[], scale(1.5), rect(0, 0, 10, 10)), scale(1.5));
    }

    #[test]
    fn a_window_takes_the_scale_of_the_screen_it_covers_most() {
        let screens = [
            (rect(0, 0, 1000, 600), scale(1.0)),
            (rect(1000, 0, 800, 600), scale(2.0)),
        ];
        // 100 pixels wide on the first screen, 300 on the second.
        let window = rect(900, 0, 400, 100);
        assert_eq!(scale_for(&screens, scale(1.0), window), scale(2.0));
    }

    #[test]
    fn a_tie_goes_to_the_earlier_screen() {
        let screens = [
            (rect(0, 0, 1000, 600), scale(1.5)),
            (rect(1000, 0, 1000, 600), scale(2.0)),
        ];
        let window = rect(900, 0, 200, 100);
        assert_eq!(scale_for(&screens, scale(1.0), window), scale(1.5));
    }

    proptest! {
        /// Whatever the screens and the window, the answer is the fallback or the scale of a
        /// screen the window really is on, and a window on none gets the fallback.
        #[test]
        fn the_scale_comes_from_a_screen_the_window_is_on(
            screens in prop::collection::vec(
                (-3000i32..3000, -3000i32..3000, 1u32..3000, 1u32..3000, 1u32..8),
                0..5,
            ),
            wx in -3000i32..3000, wy in -3000i32..3000, ww in 1u32..3000, wh in 1u32..3000,
        ) {
            let screens: Vec<(Rect, Scale)> = screens
                .into_iter()
                .map(|(x, y, w, h, quarters)| (rect(x, y, w, h), scale(f64::from(quarters) / 4.0)))
                .collect();
            let window = rect(wx, wy, ww, wh);
            let fallback = scale(16.0);
            let chosen = scale_for(&screens, fallback, window);
            let on: Vec<Scale> = screens
                .iter()
                .filter(|(area, _)| overlaps(window, *area))
                .map(|&(_, s)| s)
                .collect();
            if on.is_empty() {
                prop_assert_eq!(chosen, fallback);
            } else {
                prop_assert!(on.contains(&chosen));
            }
        }

        /// Two rectangles share pixels exactly when they overlap.
        #[test]
        fn shared_pixels_mean_overlap(
            ax in -2000i32..2000, ay in -2000i32..2000, aw in 1u32..2000, ah in 1u32..2000,
            bx in -2000i32..2000, by in -2000i32..2000, bw in 1u32..2000, bh in 1u32..2000,
        ) {
            let (a, b) = (rect(ax, ay, aw, ah), rect(bx, by, bw, bh));
            prop_assert_eq!(shared_area(a, b) > 0, overlaps(a, b));
            prop_assert_eq!(shared_area(a, b), shared_area(b, a));
        }
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
